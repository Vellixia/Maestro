pub mod engine;
pub mod fusion;
pub mod graders;
pub mod probe;
pub mod priors;
pub mod suite;

pub use engine::CalibrationEngine;

#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    #[error("gateway: {0}")]
    Gateway(#[from] gateway::GatewayError),
    #[error("registry: {0}")]
    Registry(#[from] registry::RegistryError),
    #[error("storage: {0}")]
    Storage(#[from] storage::StorageError),
    #[error("not found: {0}")]
    NotFound(String),
}
