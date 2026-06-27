use std::path::PathBuf;
use surrealdb::{
    engine::any::{connect, Any},
    opt::auth::Root,
    Surreal,
};
use tracing::info;

use crate::error::{Result, StorageError};
use crate::schema::run_migrations;

pub type Db = Surreal<Any>;

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub mode: DbMode,
    pub namespace: String,
    pub database: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub enum DbMode {
    /// Fully in-memory — used in tests and dev without persistence.
    Memory,
    /// Embedded file-based — production single-node.
    /// On Windows: requires LLVM/clang for RocksDB; use Remote instead.
    /// On Linux/macOS: uses RocksDB natively (add "kv-rocksdb" feature + rebuild).
    File(PathBuf),
    /// Remote SurrealDB server — for production or Windows without LLVM.
    Remote(String),
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            mode: DbMode::Memory,
            namespace: "maestro".into(),
            database: "main".into(),
            username: "root".into(),
            password: "root".into(),
        }
    }
}

/// Open the database, sign in, select namespace/db, and run migrations.
pub async fn open(config: &DbConfig) -> Result<Db> {
    let endpoint = match &config.mode {
        DbMode::Memory => "mem://".to_string(),
        DbMode::File(path) => {
            // Requires surrealdb "kv-rocksdb" feature + LLVM on Windows
            let path_str = path.to_string_lossy();
            format!("rocksdb://{path_str}")
        }
        DbMode::Remote(url) => url.clone(),
    };

    info!("Connecting to SurrealDB: {endpoint}");

    let db: Surreal<Any> = connect(&*endpoint)
        .await
        .map_err(StorageError::Surreal)?;

    db.signin(Root {
        username: &config.username,
        password: &config.password,
    })
    .await
    .map_err(StorageError::Surreal)?;

    db.use_ns(&config.namespace)
        .use_db(&config.database)
        .await
        .map_err(StorageError::Surreal)?;

    run_migrations(&db).await?;

    info!("SurrealDB ready");
    Ok(db)
}
