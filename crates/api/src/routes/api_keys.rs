use axum::{
    extract::State,
    Json,
};
use serde::{Deserialize, Serialize};
use storage::repos::ApiKeyRepo;
use uuid::Uuid;

use crate::{error::ApiResult, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    /// The raw API key — shown once, cannot be retrieved later.
    pub api_key: String,
    pub label: String,
}

/// Create a new API key. Returns the raw key once.
pub async fn create_api_key(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> ApiResult<Json<CreateApiKeyResponse>> {
    let raw_key = Uuid::new_v4().to_string();
    let repo = ApiKeyRepo::new(state.db.clone());
    repo.create(&raw_key, &req.label).await?;
    Ok(Json(CreateApiKeyResponse {
        api_key: raw_key,
        label: req.label,
    }))
}

#[derive(Debug, Serialize)]
pub struct ListApiKeysResponse {
    pub key_hash: String,
    pub label: String,
    pub is_active: bool,
    pub created_at: String,
}

/// List all API keys (hash only, never the raw key).
pub async fn list_api_keys(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ListApiKeysResponse>>> {
    let repo = ApiKeyRepo::new(state.db.clone());
    let keys = repo.list().await?;
    Ok(Json(
        keys.into_iter()
            .map(|k| ListApiKeysResponse {
                key_hash: k.key_hash,
                label: k.label,
                is_active: k.is_active,
                created_at: k.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}
