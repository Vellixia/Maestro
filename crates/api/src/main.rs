use std::net::SocketAddr;

use anyhow::Context;
use storage::{DbConfig, DbMode};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use api::{build_router, state::{AppState, ServerConfig}};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Tracing setup
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("maestro=debug".parse()?))
        .init();

    dotenvy::dotenv().ok();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3456);

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| {
        dirs_next().unwrap_or_else(|| std::path::PathBuf::from(".")).to_string_lossy().into()
    });

    let db_mode = if let Ok(url) = std::env::var("SURREALDB_URL") {
        DbMode::Remote(url)
    } else {
        // In-memory by default; set SURREALDB_URL for persistent storage.
        DbMode::Memory
    };

    let db_config = DbConfig {
        mode: db_mode,
        namespace: "maestro".into(),
        database: "main".into(),
        username: std::env::var("DB_USER").unwrap_or_else(|_| "root".into()),
        password: std::env::var("DB_PASS").unwrap_or_else(|_| "root".into()),
    };

    let server_config = ServerConfig {
        port,
        host: "0.0.0.0".into(),
        require_api_key: std::env::var("REQUIRE_API_KEY").is_ok(),
        password_hash: std::env::var("PASSWORD_HASH").ok(),
        jwt_secret: std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "change-me-in-production".into()),
    };

    info!("Starting Maestro on port {port}");

    let state = AppState::new(db_config, server_config)
        .await
        .context("Failed to initialize app state")?;

    // Background recalibration: check every 6 hours, re-probe if stale > 24h.
    api::background::spawn_recalibration_scheduler(
        std::sync::Arc::clone(&state.calibration),
        std::sync::Arc::clone(&state.model_registry),
        std::time::Duration::from_secs(6 * 3600),
        std::time::Duration::from_secs(24 * 3600),
    );

    let router = build_router(state);
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = TcpListener::bind(addr).await.context("Failed to bind port")?;

    info!("Listening on http://{addr}");
    axum::serve(listener, router).await.context("Server error")?;

    Ok(())
}

fn dirs_next() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".maestro"))
}
