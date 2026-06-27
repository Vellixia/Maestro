//! Admin CRUD for provider connections.
//! POST   /admin/connections              — register a new provider connection
//! GET    /admin/connections              — list all connections
//! DELETE /admin/connections/:id          — remove a connection
//! POST   /admin/connections/:id/calibrate — trigger calibration for a connection
//! GET    /admin/connections/:id/profiles  — list capability profiles

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use core_types::{ConnectionId, ProviderKind};
use serde::Deserialize;
use serde_json::json;
use storage::repos::connection::StoredConnection;
use tracing::info;

use crate::{error::{ApiError, ApiResult}, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateConnectionRequest {
    pub provider_tag: String,
    pub display_name: String,
    pub api_key: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub base_url: Option<String>,
    pub priority: Option<i32>,
    /// Model IDs to register on this connection. Calibration runs for each.
    pub models: Option<Vec<String>>,
    /// If true, skip probe suite and use priors only (faster onboarding).
    pub priors_only: Option<bool>,
}

pub async fn create_connection(
    State(state): State<AppState>,
    Json(req): Json<CreateConnectionRequest>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let conn_id = ConnectionId::new();

    let credentials = if let Some(key) = &req.api_key {
        json!({"api_key": key})
    } else if let Some(token) = &req.access_token {
        json!({
            "access_token": token,
            "refresh_token": req.refresh_token,
        })
    } else {
        return Err(ApiError::BadRequest("api_key or access_token required".into()));
    };

    let provider = build_provider_kind(&req.provider_tag, req.base_url.as_deref());

    let conn = StoredConnection {
        id: None,
        connection_id: conn_id.0.clone(),
        provider,
        provider_tag: req.provider_tag.clone(),
        display_name: req.display_name.clone(),
        auth_type: if req.api_key.is_some() { "apikey" } else { "oauth" }.into(),
        credentials,
        priority: req.priority.unwrap_or(100),
        is_active: true,
        cooldown_until: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let stored = state.gateway.connection_repo().create(&conn).await?;
    info!(connection_id = %conn_id, provider = %req.provider_tag, "Connection registered");

    // Spawn background calibration for each specified model.
    let models = req.models.unwrap_or_default();
    let priors_only = req.priors_only.unwrap_or(false);

    if !models.is_empty() {
        let calibration = state.calibration.clone();
        let cid = conn_id.clone();
        let model_list = models.clone();

        tokio::spawn(async move {
            for model_id in &model_list {
                if priors_only {
                    match calibration.register_priors_only(cid.clone(), model_id).await {
                        Ok(profile) => info!(
                            model = model_id,
                            source = %profile.calibration_source,
                            "priors-only profile registered"
                        ),
                        Err(e) => tracing::warn!(model = model_id, "priors-only registration failed: {e}"),
                    }
                } else {
                    match calibration.calibrate(cid.clone(), model_id).await {
                        Ok(profile) => info!(
                            model = model_id,
                            source = %profile.calibration_source,
                            "calibration complete"
                        ),
                        Err(e) => tracing::warn!(model = model_id, "calibration failed: {e}"),
                    }
                }
            }
        });
    }

    Ok((StatusCode::CREATED, Json(json!({
        "id": stored.connection_id,
        "provider_tag": stored.provider_tag,
        "display_name": stored.display_name,
        "models": models,
        "calibration": if priors_only { "priors_only" } else { "running" },
        "created_at": stored.created_at,
    }))))
}

pub async fn list_connections(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let conns = state.gateway.connection_repo().list_active().await?;
    let items: Vec<serde_json::Value> = conns
        .iter()
        .map(|c| json!({
            "id": c.connection_id,
            "provider_tag": c.provider_tag,
            "display_name": c.display_name,
            "priority": c.priority,
            "is_active": c.is_active,
            "created_at": c.created_at,
        }))
        .collect();
    Ok(Json(json!({"data": items})))
}

pub async fn delete_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.gateway.connection_repo().delete(&ConnectionId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Trigger full calibration for all models registered on a connection.
pub async fn calibrate_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CalibrateRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let cid = ConnectionId(id.clone());
    let calibration = state.calibration.clone();

    let models = req.models.clone();
    tokio::spawn(async move {
        for model_id in &models {
            match calibration.calibrate(cid.clone(), model_id).await {
                Ok(p) => info!(model = model_id, source = %p.calibration_source, "recalibrated"),
                Err(e) => tracing::warn!(model = model_id, "recalibration failed: {e}"),
            }
        }
    });

    Ok(Json(json!({
        "connection_id": id,
        "models": req.models,
        "status": "calibration_started",
    })))
}

#[derive(Debug, Deserialize)]
pub struct CalibrateRequest {
    pub models: Vec<String>,
}

/// List capability profiles for a connection.
pub async fn list_profiles(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let profiles = state.model_registry.list_for_connection(&id).await?;
    let items: Vec<serde_json::Value> = profiles
        .iter()
        .map(|p| json!({
            "model_id": p.model_id,
            "calibration_source": p.calibration_source,
            "calibrated_at": p.calibrated_at,
            "skills": {
                "reasoning": p.skills.reasoning,
                "coding": p.skills.coding,
                "math": p.skills.math,
                "instruction_following": p.skills.instruction_following,
                "factuality": p.skills.factuality,
                "writing": p.skills.writing,
            },
            "hard": {
                "context_window": p.hard.context_window,
                "supports_tools": p.hard.supports_tools,
                "supports_vision": p.hard.supports_vision,
                "supports_json_mode": p.hard.supports_json_mode,
            },
            "ops": {
                "cost_in_per_m": p.ops.cost_in_per_m,
                "cost_out_per_m": p.ops.cost_out_per_m,
                "is_free_tier": p.ops.is_free_tier,
            }
        }))
        .collect();
    Ok(Json(json!({"data": items})))
}

fn build_provider_kind(tag: &str, base_url: Option<&str>) -> ProviderKind {
    match tag {
        "openai" => ProviderKind::OpenAi,
        "anthropic" => ProviderKind::Anthropic,
        "gemini" => ProviderKind::Gemini,
        _ => {
            if let Some(url) = base_url {
                ProviderKind::OpenAiCompat {
                    base_url: url.to_string(),
                    name: tag.to_string(),
                }
            } else {
                ProviderKind::Custom { id: tag.to_string(), name: tag.to_string() }
            }
        }
    }
}
