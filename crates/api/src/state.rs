use std::sync::Arc;

use gateway::{client::{GatewayClient, GatewayConfig}, providers::registry::ProviderRegistry};
use registry::ModelRegistry;
use calibration::CalibrationEngine;
use planner::Planner;
use executor::DagExecutor;
use synthesizer::Synthesizer;
use storage::{Db, DbConfig};

/// Shared application state — one instance, cloned into every handler via Arc.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub gateway: Arc<GatewayClient>,
    pub model_registry: Arc<ModelRegistry>,
    pub calibration: Arc<CalibrationEngine>,
    pub planner: Arc<Planner>,
    pub executor: Arc<DagExecutor>,
    pub synthesizer: Arc<Synthesizer>,
    pub config: Arc<ServerConfig>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    /// If set, require this value as a Bearer token or x-api-key header.
    pub require_api_key: bool,
    /// Master password hash (argon2) for dashboard login.
    pub password_hash: Option<String>,
    /// JWT signing secret.
    pub jwt_secret: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3456,
            host: "0.0.0.0".into(),
            require_api_key: false,
            password_hash: None,
            jwt_secret: "change-me-in-production".into(),
        }
    }
}

impl AppState {
    pub async fn new(db_config: DbConfig, server_config: ServerConfig) -> anyhow::Result<Self> {
        let db = storage::db::open(&db_config).await?;

        let provider_registry = Arc::new(ProviderRegistry::new());
        let gateway = Arc::new(GatewayClient::new(
            db.clone(),
            provider_registry,
            GatewayConfig::default(),
        ));

        let model_registry = Arc::new(ModelRegistry::new(db.clone()));
        let calibration = Arc::new(CalibrationEngine::new(
            Arc::clone(&gateway),
            Arc::clone(&model_registry),
        ));

        let planner = Arc::new(Planner::new(Arc::clone(&gateway)).with_cache(db.clone()));
        let executor = Arc::new(DagExecutor::new(Arc::clone(&gateway), Arc::clone(&model_registry)));
        let synthesizer = Arc::new(Synthesizer::new(Arc::clone(&gateway)));

        Ok(Self {
            db,
            gateway,
            model_registry,
            calibration,
            planner,
            executor,
            synthesizer,
            config: Arc::new(server_config),
        })
    }
}
