//! POST /v1/chat/completions
//!
//! - model = "auto" → classify → route → execute → verify/escalate → return
//! - any other model → passthrough to gateway (direct routing)

use axum::{
    extract::State,
    response::{IntoResponse, Response, Sse},
    Json,
};
use axum::response::sse::Event;
use classifier::classify;
use core_types::{OutputType, Stakes};
use futures_util::stream;
use gateway::types::{ChatRequest, GatewayResponse};
use router::route;
use serde_json::json;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};
use verifier::verify;

use crate::{error::{ApiError, ApiResult}, state::AppState};

const AUTO_MODEL: &str = "auto";

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> ApiResult<Response> {
    info!(model = %req.model, stream = ?req.stream, "Chat completion");

    if req.model == AUTO_MODEL {
        return handle_auto(state, req).await;
    }

    // Direct passthrough for explicit model names.
    execute_and_respond(state, req).await
}

/// Auto routing: classify → route → execute → verify → escalate if needed.
async fn handle_auto(state: AppState, mut req: ChatRequest) -> ApiResult<Response> {
    // Extract the last user message as the task instruction.
    let instruction = req.messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, gateway::types::MessageRole::User))
        .map(|m| m.content.text().to_string())
        .unwrap_or_default();

    // 1. Classify the task.
    let context_token_estimate = req.messages.iter()
        .map(|m| m.content.text().len() as u32 / 4)
        .sum::<u32>();
    let requirement = classify(&instruction, context_token_estimate);

    debug!(
        stakes = ?requirement.stakes,
        needs_json = requirement.needs_json_mode,
        needs_tools = requirement.needs_tools,
        skill_dims = requirement.skill_minimums.len(),
        "classified task"
    );

    // 2. Fetch routable capability profiles.
    let profiles = state.model_registry.list_routable().await?;

    if profiles.is_empty() {
        // No models registered — passthrough with a sentinel model name that
        // the gateway will reject with a clear error.
        warn!("no models in registry, cannot auto-route");
        return Err(ApiError::Internal(
            "No models registered. Add a connection via POST /admin/connections first.".into(),
        ));
    }

    // 3. Route to primary + escalation ladder.
    let policy = core_types::Policy::default();
    let routing = match route(&requirement, &policy, &profiles, 0.0) {
        Ok(r) => r,
        Err(e) => return Err(ApiError::Internal(format!("routing failed: {e}"))),
    };

    info!(
        primary = %routing.primary.model_id,
        escalation_count = routing.escalation_ladder.len(),
        "auto-routed"
    );

    // 4. Execute with verify-and-escalate cascade.
    let output_type = infer_output_type(&req, &requirement);
    let stakes = infer_stakes(requirement.stakes);

    let mut tried: Vec<String> = Vec::new();
    let candidates = std::iter::once(&routing.primary)
        .chain(routing.escalation_ladder.iter());

    for profile in candidates {
        tried.push(profile.model_id.clone());

        let resp = match state.gateway
            .chat_on_connection(req.clone(), &profile.connection_id.0, &profile.model_id)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(model = %profile.model_id, "gateway call failed: {e}");
                continue;
            }
        };

        // For streaming, skip verification (can't buffer the stream).
        if req.stream.unwrap_or(false) {
            return respond_with_routing_header(resp, &profile.model_id, &tried);
        }

        let GatewayResponse::Complete(ref complete) = resp else { unreachable!() };

        let output_text = complete.choices
            .first()
            .map(|c| c.message.content.text().to_string())
            .unwrap_or_default();

        let verify_result = verify(
            &output_text,
            &instruction,
            &output_type,
            &stakes,
            &state.gateway,
        )
        .await;

        debug!(
            model = %profile.model_id,
            result = ?verify_result,
            "verification"
        );

        if verify_result.passed() {
            return respond_with_routing_header(resp, &profile.model_id, &tried);
        }

        warn!(
            model = %profile.model_id,
            reason = ?verify_result,
            "verification failed, escalating"
        );

        // Update online learning: this model failed this task.
        // (Best-effort — don't block the response.)
        let _ = state.model_registry.apply_online_update(
            &profile.connection_id.0,
            &profile.model_id,
            core_types::SkillDimension::Reasoning,
            false,
        )
        .await;
    }

    // All candidates exhausted — return last response even if verification failed.
    warn!(tried = ?tried, "all models failed verification, returning last response");
    req.model = tried.last().cloned().unwrap_or_else(|| routing.primary.model_id.clone());
    execute_and_respond(state, req).await
}

fn respond_with_routing_header(
    resp: GatewayResponse,
    model: &str,
    tried: &[String],
) -> ApiResult<Response> {
    match resp {
        GatewayResponse::Complete(mut r) => {
            // Inject routing provenance into the response model field.
            r.model = format!("auto→{model}");
            let mut response = Json(r).into_response();
            response.headers_mut().insert(
                "x-routed-to",
                model.parse().unwrap_or_else(|_| "unknown".parse().unwrap()),
            );
            response.headers_mut().insert(
                "x-tried-models",
                tried.join(",").parse().unwrap_or_else(|_| "unknown".parse().unwrap()),
            );
            Ok(response)
        }
        GatewayResponse::Stream(rx) => {
            let stream = ReceiverStream::new(rx)
                .map(|result| {
                    let data = match result {
                        Ok(chunk) => serde_json::to_string(&chunk).unwrap_or_default(),
                        Err(e) => json!({"error":{"message":e}}).to_string(),
                    };
                    Ok::<Event, std::convert::Infallible>(Event::default().data(data))
                })
                .chain(stream::once(async {
                    Ok::<Event, std::convert::Infallible>(Event::default().data("[DONE]"))
                }));
            Ok(Sse::new(stream).into_response())
        }
    }
}

async fn execute_and_respond(state: AppState, req: ChatRequest) -> ApiResult<Response> {
    let is_streaming = req.stream.unwrap_or(false);
    match state.gateway.chat(req).await? {
        GatewayResponse::Complete(resp) => Ok(Json(resp).into_response()),
        GatewayResponse::Stream(rx) => {
            if !is_streaming {
                return Err(ApiError::Internal(
                    "Provider returned stream for non-streaming request".into(),
                ));
            }
            let stream = ReceiverStream::new(rx)
                .map(|result| {
                    let data = match result {
                        Ok(chunk) => serde_json::to_string(&chunk).unwrap_or_default(),
                        Err(e) => json!({"error":{"message":e}}).to_string(),
                    };
                    Ok::<Event, std::convert::Infallible>(Event::default().data(data))
                })
                .chain(stream::once(async {
                    Ok::<Event, std::convert::Infallible>(Event::default().data("[DONE]"))
                }));
            Ok(Sse::new(stream).into_response())
        }
    }
}

fn infer_output_type(req: &ChatRequest, req_profile: &core_types::RequirementProfile) -> OutputType {
    if req_profile.needs_json_mode {
        return OutputType::Json { schema: None };
    }
    // If response_format is set to json_object, treat as JSON.
    if let Some(rf) = &req.response_format {
        if rf.kind == "json_object" || rf.kind == "json_schema" {
            return OutputType::Json {
                schema: rf.json_schema.as_ref().map(|s| s.to_string()),
            };
        }
    }
    OutputType::Text
}

fn infer_stakes(stakes_f32: f32) -> Stakes {
    match (stakes_f32 * 100.0) as u32 {
        0..=15 => Stakes::Trivial,
        16..=35 => Stakes::Low,
        36..=65 => Stakes::Medium,
        _ => Stakes::High,
    }
}
