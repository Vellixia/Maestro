use crate::db::Db;
use crate::error::Result;
use tracing::info;

/// Idempotent schema + index definitions.
/// SurrealDB's DEFINE ... IF NOT EXISTS makes these safe to re-run on every startup.
pub async fn run_migrations(db: &Db) -> Result<()> {
    info!("Running schema migrations");

    // ── Tables ────────────────────────────────────────────────────────────────

    // Schemaless tables: no type constraints that would reject nanosecond
    // datetime strings from chrono. Indexes are kept for query performance.
    let queries = r#"
        DEFINE TABLE IF NOT EXISTS settings       SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS api_key        SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS connection     SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS model          SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS capability_profile SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS benchmark_prior    SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS run                SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS subtask            SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS subtask_attempt    SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS usage              SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS plan_cache         SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS trace_event        SCHEMALESS;

        DEFINE TABLE IF NOT EXISTS depends_on    SCHEMALESS TYPE RELATION IN subtask OUT subtask;
        DEFINE TABLE IF NOT EXISTS escalated_to  SCHEMALESS TYPE RELATION IN subtask_attempt OUT subtask_attempt;

        -- Indexes for fast lookups
        DEFINE INDEX IF NOT EXISTS settings_key         ON TABLE settings          COLUMNS key UNIQUE;
        DEFINE INDEX IF NOT EXISTS api_key_hash         ON TABLE api_key           COLUMNS key_hash UNIQUE;
        DEFINE INDEX IF NOT EXISTS connection_tag       ON TABLE connection        COLUMNS provider_tag;
        DEFINE INDEX IF NOT EXISTS model_id_idx         ON TABLE model             COLUMNS model_id UNIQUE;
        DEFINE INDEX IF NOT EXISTS cap_conn_model       ON TABLE capability_profile COLUMNS connection_id, model_id UNIQUE;
        DEFINE INDEX IF NOT EXISTS prior_model_dim      ON TABLE benchmark_prior   COLUMNS model_id, dimension UNIQUE;
        DEFINE INDEX IF NOT EXISTS plan_cache_hash      ON TABLE plan_cache        COLUMNS goal_hash UNIQUE;
        DEFINE INDEX IF NOT EXISTS trace_run_id         ON TABLE trace_event       COLUMNS run_id;
    "#;

    db.query(queries).await.map_err(crate::error::StorageError::Surreal)?;

    info!("Schema migrations complete");
    Ok(())
}
