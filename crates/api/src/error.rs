use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Gateway error: {0}")]
    Gateway(#[from] gateway::GatewayError),

    #[error("Storage error: {0}")]
    Storage(#[from] storage::StorageError),

    #[error("Registry error: {0}")]
    Registry(#[from] registry::RegistryError),

    #[error("Calibration error: {0}")]
    Calibration(#[from] calibration::CalibrationError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            ApiError::Gateway(gateway::GatewayError::NoAvailableConnections) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "no_available_connections",
                "All connections are rate-limited or unavailable".to_string(),
            ),
            ApiError::Gateway(gateway::GatewayError::RateLimited { retry_after_secs }) => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("Rate limited, retry after {retry_after_secs}s"),
            ),
            // Provider returned a 4xx (bad key, quota, etc.) → 502 upstream error.
            ApiError::Gateway(gateway::GatewayError::ProviderError { status, .. })
                if *status >= 400 && *status < 500 =>
            (
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                self.to_string(),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                self.to_string(),
            ),
        };

        // OpenAI-compatible error format
        let body = json!({
            "error": {
                "message": message,
                "type": code,
                "code": code,
            }
        });

        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
