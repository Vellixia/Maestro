use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SurrealDB error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Storage error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;
