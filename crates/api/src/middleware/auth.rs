use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use storage::repos::ApiKeyRepo;

use crate::state::AppState;

/// Middleware: check for a valid API key when `require_api_key` is enabled.
/// Accepts key as `Authorization: Bearer <key>` or `x-api-key: <key>`.
pub async fn auth_layer(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.require_api_key {
        return Ok(next.run(req).await);
    }

    let provided = extract_key(&headers);

    match provided {
        Some(key) => {
            let repo = ApiKeyRepo::new(state.db.clone());
            let valid = repo.validate(&key).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if valid {
                Ok(next.run(req).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

fn extract_key(headers: &HeaderMap) -> Option<String> {
    if let Some(val) = headers.get("authorization") {
        let s = val.to_str().ok()?;
        if let Some(key) = s.strip_prefix("Bearer ") {
            return Some(key.trim().to_string());
        }
        return Some(s.to_string());
    }
    if let Some(val) = headers.get("x-api-key") {
        return Some(val.to_str().ok()?.to_string());
    }
    None
}
