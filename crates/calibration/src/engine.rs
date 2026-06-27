use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use core_types::{CapabilityProfile, ConnectionId, HardConstraints, OperationalProfile, SkillDimension};
use gateway::{
    types::{ChatMessage, ChatRequest, MessageContent, MessageRole},
    GatewayClient,
};
use registry::ModelRegistry;
use tracing::{debug, info, warn};

use crate::{
    fusion::fuse_skills,
    graders::{grade_sync, grade_with_judge},
    probe::{GraderKind, ProbeResult},
    priors::{lookup_prior, neutral_prior},
    suite::probes_for,
    CalibrationError,
};

/// The anchor model used as LLM judge for open-ended probes.
/// Falls back to nothing if None — writing probes are skipped.
const ANCHOR_MODEL: &str = "claude-haiku-4-5";

pub struct CalibrationEngine {
    gateway: Arc<GatewayClient>,
    registry: Arc<ModelRegistry>,
}

impl CalibrationEngine {
    pub fn new(gateway: Arc<GatewayClient>, registry: Arc<ModelRegistry>) -> Self {
        Self { gateway, registry }
    }

    /// Full calibration run for one connection/model pair.
    /// 1. Seed from benchmark priors.
    /// 2. Run probe suite via the gateway.
    /// 3. Fuse prior + observations.
    /// 4. Persist the profile via the registry.
    pub async fn calibrate(
        &self,
        connection_id: ConnectionId,
        model_id: &str,
    ) -> Result<CapabilityProfile, CalibrationError> {
        info!(connection_id = %connection_id, model = model_id, "starting calibration");
        let start = Instant::now();

        // ── 1. Priors ───────────────────────────────────────────────────────
        let (prior_skills, hard, ops) = if let Some(p) = lookup_prior(model_id) {
            (p.skills.clone(), p.hard.clone(), p.ops.clone())
        } else {
            let (sv, _) = neutral_prior();
            (sv, HardConstraints::default(), OperationalProfile::default())
        };

        // Store a priors-only profile immediately so the model is routable
        // even before probing completes.
        self.registry
            .register_with_prior(connection_id.clone(), model_id, prior_skills.clone(), hard.clone(), ops.clone())
            .await?;

        // ── 2. Probe suite ──────────────────────────────────────────────────
        let mut all_results: Vec<ProbeResult> = Vec::new();

        for dim in SkillDimension::all() {
            let probes = probes_for(*dim);
            let mut dim_results: Vec<ProbeResult> = Vec::new();

            for probe in &probes {
                let req = ChatRequest {
                    model: model_id.to_string(),
                    messages: vec![ChatMessage {
                        role: MessageRole::User,
                        content: MessageContent::Text(probe.prompt.clone()),
                        tool_call_id: None,
                        name: None,
                        tool_calls: None,
                    }],
                    temperature: Some(0.0),
                    max_tokens: Some(512),
                    stream: Some(false),
                    top_p: None,
                    tools: None,
                    tool_choice: None,
                    response_format: None,
                    extra: Default::default(),
                };

                let response_text = match self.gateway.chat(req).await {
                    Ok(gateway::GatewayResponse::Complete(r)) => r
                        .choices
                        .first()
                        .map(|c| c.message.content.text().to_string())
                        .unwrap_or_default(),
                    Ok(gateway::GatewayResponse::Stream(_)) => {
                        warn!("got stream for non-stream probe — skipping");
                        continue;
                    }
                    Err(e) => {
                        warn!(model = model_id, dim = ?dim, "probe call failed: {e}");
                        continue;
                    }
                };

                match &probe.grader {
                    GraderKind::LlmJudge { rubric, pass_threshold } => {
                        // Use cheapest anchor model as judge; skip if unavailable.
                        let judge_result = grade_with_judge(
                            &response_text,
                            rubric,
                            *pass_threshold,
                            &self.gateway,
                            ANCHOR_MODEL,
                        )
                        .await;
                        // Remap dimension to the actual probe dimension
                        dim_results.push(ProbeResult { dimension: probe.dimension, ..judge_result });
                    }
                    grader => {
                        if let Some(result) = grade_sync(probe.dimension, &response_text, grader) {
                            debug!(
                                dim = ?dim,
                                passed = result.passed,
                                reason = %result.reason,
                                "probe graded"
                            );
                            dim_results.push(result);
                        }
                    }
                }
            }
            all_results.extend(dim_results);
        }

        // ── 3. Fusion ───────────────────────────────────────────────────────
        let fused_skills = fuse_skills(&prior_skills, &all_results);

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let pass_count = all_results.iter().filter(|r| r.passed).count();
        info!(
            connection_id = %connection_id,
            model = model_id,
            probes = all_results.len(),
            passed = pass_count,
            elapsed_ms,
            "calibration complete"
        );

        // ── 4. Persist ──────────────────────────────────────────────────────
        let profile = CapabilityProfile {
            id: format!("{}_{}", connection_id, model_id),
            connection_id: connection_id.clone(),
            model_id: model_id.to_string(),
            skills: fused_skills,
            hard,
            ops,
            calibrated_at: Utc::now(),
            calibration_source: if all_results.is_empty() {
                "prior".to_string()
            } else {
                "hybrid".to_string()
            },
        };

        self.registry.update_profile(&profile).await?;
        Ok(profile)
    }

    /// Light re-probe — only runs probes for a single dimension.
    /// Used by the background refresh cron.
    pub async fn recalibrate_dimension(
        &self,
        connection_id: &str,
        model_id: &str,
        dim: SkillDimension,
    ) -> Result<(), CalibrationError> {
        let Some(mut profile) = self.registry.get_profile(connection_id, model_id).await? else {
            return Err(CalibrationError::NotFound(format!(
                "no profile for {connection_id}/{model_id}"
            )));
        };

        let probes = probes_for(dim);
        let mut results = Vec::new();

        for probe in &probes {
            let req = ChatRequest {
                model: model_id.to_string(),
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Text(probe.prompt.clone()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                }],
                temperature: Some(0.0),
                max_tokens: Some(512),
                stream: Some(false),
                top_p: None,
                tools: None,
                tool_choice: None,
                response_format: None,
                extra: Default::default(),
            };

            if let Ok(gateway::GatewayResponse::Complete(r)) = self.gateway.chat(req).await {
                let text = r.choices.first().map(|c| c.message.content.text().to_string()).unwrap_or_default();
                if let Some(result) = grade_sync(probe.dimension, &text, &probe.grader) {
                    results.push(result);
                }
            }
        }

        if !results.is_empty() {
            let prior = profile.skills.get(dim).clone();
            *profile.skills.get_mut(dim) = crate::fusion::fuse_dimension(&prior, &results);
            profile.calibrated_at = Utc::now();
            profile.calibration_source = "hybrid".to_string();
            self.registry.update_profile(&profile).await?;
        }

        Ok(())
    }
}
