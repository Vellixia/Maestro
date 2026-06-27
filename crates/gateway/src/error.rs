use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("All connections exhausted or rate-limited for provider")]
    NoAvailableConnections,

    #[error("Rate limited by provider: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Authentication failed for connection {connection_id}")]
    AuthFailed { connection_id: String },

    #[error("Provider returned error {status}: {body}")]
    ProviderError { status: u16, body: String },

    #[error("Request serialization error: {0}")]
    Serialization(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Translation error: {0}")]
    Translation(String),

    #[error("Storage error: {0}")]
    Storage(#[from] storage::StorageError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Timeout after {secs}s")]
    Timeout { secs: u64 },

    #[error("Gateway error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GatewayError>;
