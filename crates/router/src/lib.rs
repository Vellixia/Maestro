use core_types::{CapabilityProfile, OptimizationMode, Policy, RequirementProfile, SkillDimension};
use policy::{estimate_cost, is_permitted, within_budget};
use tracing::debug;

/// The result of routing a task to a model.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// The primary (cheapest capable) model to try first.
    pub primary: CapabilityProfile,
    /// Ordered list of fallbacks, cheapest→strongest, for escalation.
    pub escalation_ladder: Vec<CapabilityProfile>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("no models available")]
    NoModels,
    #[error("no model meets hard constraints: {0}")]
    HardConstraintFailed(String),
    #[error("no model meets capability requirements")]
    CapabilityNotMet,
    #[error("no model within budget")]
    BudgetExhausted,
}

/// Route a task to the best model given a pool of capability profiles and a policy.
///
/// Pipeline:
/// 1. Policy gate (privacy / allow-list / block-list / free-only)
/// 2. Hard-constraint filter (context window, modality, tools, JSON)
/// 3. Budget gate
/// 4. Capability dominance (score >= required + safety_margin on every required dim)
/// 5. Rank by policy objective (cheapest / fastest / highest-quality)
/// 6. Primary = rank[0], escalation_ladder = rank[1..=2]
pub fn route(
    req: &RequirementProfile,
    policy: &Policy,
    profiles: &[CapabilityProfile],
    spent_usd: f64,
) -> Result<RoutingDecision, RouterError> {
    if profiles.is_empty() {
        return Err(RouterError::NoModels);
    }

    // ── 1. Policy gate ───────────────────────────────────────────────────
    let after_policy: Vec<&CapabilityProfile> =
        profiles.iter().filter(|p| is_permitted(p, policy)).collect();

    // ── 2. Hard-constraint filter ────────────────────────────────────────
    let mut hard_fail_reasons = Vec::new();
    let after_hard: Vec<&CapabilityProfile> = after_policy
        .iter()
        .copied()
        .filter(|p| {
            if p.hard.context_window < req.min_context_tokens {
                hard_fail_reasons.push(format!(
                    "{}: context {} < required {}",
                    p.model_id, p.hard.context_window, req.min_context_tokens
                ));
                return false;
            }
            if req.needs_vision && !p.hard.supports_vision {
                hard_fail_reasons.push(format!("{}: no vision", p.model_id));
                return false;
            }
            if req.needs_audio && !p.hard.supports_audio_input {
                hard_fail_reasons.push(format!("{}: no audio", p.model_id));
                return false;
            }
            if req.needs_tools && !p.hard.supports_tools {
                hard_fail_reasons.push(format!("{}: no tools", p.model_id));
                return false;
            }
            if req.needs_json_mode && !p.hard.supports_json_mode {
                hard_fail_reasons.push(format!("{}: no JSON mode", p.model_id));
                return false;
            }
            true
        })
        .collect();

    if after_hard.is_empty() {
        return Err(RouterError::HardConstraintFailed(hard_fail_reasons.join("; ")));
    }

    // ── 3. Budget gate ───────────────────────────────────────────────────
    let after_budget: Vec<&CapabilityProfile> = after_hard
        .iter()
        .copied()
        .filter(|p| within_budget(p, policy, req.min_context_tokens, req.expected_output_tokens, spent_usd))
        .collect();

    // If everything is over budget, keep one anyway (cheapest) to let the caller decide.
    let pool = if after_budget.is_empty() { after_hard } else { after_budget };

    // ── 4. Capability dominance ──────────────────────────────────────────
    let capable: Vec<&CapabilityProfile> = pool
        .iter()
        .copied()
        .filter(|p| meets_requirements(p, req))
        .collect();

    // If no model meets all requirements, fall back to best available.
    let pool = if capable.is_empty() {
        debug!("no model fully meets requirements, using best available");
        pool
    } else {
        capable
    };

    // ── 5. Rank by objective ─────────────────────────────────────────────
    let mut ranked: Vec<&CapabilityProfile> = pool;
    match policy.mode {
        OptimizationMode::CheapestViable | OptimizationMode::FreeOnly => {
            ranked.sort_by(|a, b| {
                let cost_a = effective_cost(a, req);
                let cost_b = effective_cost(b, req);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        OptimizationMode::Fastest => {
            ranked.sort_by(|a, b| {
                b.ops
                    .latency_tok_per_sec
                    .partial_cmp(&a.ops.latency_tok_per_sec)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        OptimizationMode::HighestQuality => {
            ranked.sort_by(|a, b| {
                let qa = average_required_score(a, req);
                let qb = average_required_score(b, req);
                qb.partial_cmp(&qa).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    // ── 6. Build decision ────────────────────────────────────────────────
    let primary = (*ranked[0]).clone();
    let escalation_ladder: Vec<CapabilityProfile> = ranked[1..]
        .iter()
        .take(3)
        .map(|p| (*p).clone())
        .collect();

    debug!(
        primary = %primary.model_id,
        escalation_count = escalation_ladder.len(),
        "routing decision"
    );

    Ok(RoutingDecision { primary, escalation_ladder })
}

/// True if the profile satisfies all required skill minimums + safety margin.
fn meets_requirements(profile: &CapabilityProfile, req: &RequirementProfile) -> bool {
    for (dim_key, &min_score) in &req.skill_minimums {
        if let Some(dim) = parse_dim(dim_key) {
            let score = profile.skills.get(dim).score;
            if score < min_score + req.safety_margin {
                return false;
            }
        }
    }
    true
}

/// Effective cost for ranking (free → 0, paid → per-M rates * expected tokens).
fn effective_cost(profile: &CapabilityProfile, req: &RequirementProfile) -> f64 {
    estimate_cost(profile, req.min_context_tokens, req.expected_output_tokens)
}

/// Average score across required dimensions (for quality ranking).
fn average_required_score(profile: &CapabilityProfile, req: &RequirementProfile) -> f32 {
    if req.skill_minimums.is_empty() {
        // No specific requirements: use overall average.
        let dims = SkillDimension::all();
        let sum: f32 = dims.iter().map(|d| profile.skills.get(*d).score).sum();
        return sum / dims.len() as f32;
    }
    let sum: f32 = req
        .skill_minimums
        .keys()
        .filter_map(|k| parse_dim(k))
        .map(|d| profile.skills.get(d).score)
        .sum();
    let n = req.skill_minimums.len() as f32;
    sum / n.max(1.0)
}

fn parse_dim(key: &str) -> Option<SkillDimension> {
    match key {
        "reasoning" => Some(SkillDimension::Reasoning),
        "coding" => Some(SkillDimension::Coding),
        "math" => Some(SkillDimension::Math),
        "instructionfollowing" => Some(SkillDimension::InstructionFollowing),
        "longcontextrecall" => Some(SkillDimension::LongContextRecall),
        "toolcalling" => Some(SkillDimension::ToolCalling),
        "structuredoutput" => Some(SkillDimension::StructuredOutput),
        "factuality" => Some(SkillDimension::Factuality),
        "multilingual" => Some(SkillDimension::Multilingual),
        "writing" => Some(SkillDimension::Writing),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use core_types::{
        CapabilityProfile, ConnectionId, HardConstraints, OperationalProfile,
        RequirementProfile, SkillScore, SkillVector,
    };

    fn profile(model_id: &str, reasoning: f32, cost_per_m: f64, is_free: bool) -> CapabilityProfile {
        let mut skills = SkillVector::default();
        skills.reasoning = SkillScore { score: reasoning, confidence: 0.8, n_samples: 10 };
        CapabilityProfile {
            id: model_id.into(),
            connection_id: ConnectionId("conn1".into()),
            model_id: model_id.into(),
            skills,
            hard: HardConstraints {
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_json_mode: true,
                supports_tools: true,
                supports_streaming: true,
                ..Default::default()
            },
            ops: OperationalProfile {
                cost_in_per_m: cost_per_m,
                cost_out_per_m: cost_per_m * 4.0,
                is_free_tier: is_free,
                ..Default::default()
            },
            calibrated_at: Utc::now(),
            calibration_source: "prior".into(),
        }
    }

    fn req_reasoning(min: f32) -> RequirementProfile {
        let mut r = RequirementProfile::default();
        r.skill_minimums.insert("reasoning".into(), min);
        r.min_context_tokens = 1024;
        r.expected_output_tokens = 512;
        r
    }

    #[test]
    fn cheapest_capable_wins() {
        let profiles = vec![
            profile("cheap-weak", 60.0, 0.1, false),
            profile("expensive-strong", 90.0, 10.0, false),
            profile("mid", 75.0, 1.0, false),
        ];
        let req = req_reasoning(70.0);
        let decision = route(&req, &Policy::default(), &profiles, 0.0).unwrap();
        assert_eq!(decision.primary.model_id, "mid");
    }

    #[test]
    fn free_model_wins_when_capable() {
        let profiles = vec![
            profile("free-capable", 80.0, 0.0, true),
            profile("paid-capable", 80.0, 5.0, false),
        ];
        let req = req_reasoning(70.0);
        let decision = route(&req, &Policy::default(), &profiles, 0.0).unwrap();
        assert_eq!(decision.primary.model_id, "free-capable");
    }

    #[test]
    fn escalation_ladder_ordered() {
        let profiles = vec![
            profile("a", 90.0, 1.0, false),
            profile("b", 90.0, 5.0, false),
            profile("c", 90.0, 10.0, false),
        ];
        let req = req_reasoning(70.0);
        let decision = route(&req, &Policy::default(), &profiles, 0.0).unwrap();
        assert_eq!(decision.primary.model_id, "a");
        assert_eq!(decision.escalation_ladder[0].model_id, "b");
        assert_eq!(decision.escalation_ladder[1].model_id, "c");
    }

    #[test]
    fn hard_constraint_context_window_filters() {
        let mut p = profile("tiny-ctx", 90.0, 1.0, false);
        p.hard.context_window = 512;
        let profiles = vec![p];
        let mut req = req_reasoning(0.0);
        req.min_context_tokens = 4096;
        let result = route(&req, &Policy::default(), &profiles, 0.0);
        assert!(matches!(result, Err(RouterError::HardConstraintFailed(_))));
    }
}
