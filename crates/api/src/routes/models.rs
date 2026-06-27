//! GET /v1/models — list all models available across registered connections.

use axum::{extract::State, Json};
use chrono::Utc;
use serde_json::{json, Value};
use tracing::debug;

use crate::{error::ApiResult, state::AppState};

pub async fn list_models(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let connections = state
        .gateway
        .list_connections()
        .await
        .unwrap_or_default();

    let mut model_objects: Vec<Value> = vec![];
    let registry = state.gateway.registry();

    for conn in &connections {
        if let Some(provider_cfg) = registry.get(&conn.provider_tag) {
            for model in &provider_cfg.models {
                model_objects.push(json!({
                    "id": format!("{}/{}", conn.provider_tag, model.id),
                    "object": "model",
                    "created": Utc::now().timestamp(),
                    "owned_by": conn.provider_tag,
                    "connection_id": conn.connection_id,
                }));
            }
        }
    }

    debug!("Listing {} models", model_objects.len());

    Ok(Json(json!({
        "object": "list",
        "data": model_objects,
    })))
}
