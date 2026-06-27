use chrono::Utc;
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::{
    db::Db,
    error::{Result, StorageError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRun {
    pub id: Option<RecordId>,
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub policy: serde_json::Value,
    pub total_cost: f64,
    pub total_tokens: i64,
    pub wall_ms: Option<i64>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct RunRepo {
    db: Db,
}

impl RunRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create(&self, run: &StoredRun) -> Result<StoredRun> {
        let id = run.run_id.clone();
        let owned = run.clone();
        let created: Option<StoredRun> = self
            .db
            .create(("run", id.as_str()))
            .content(owned)
            .await
            .map_err(StorageError::Surreal)?;
        created.ok_or_else(|| StorageError::Other("create run returned nothing".into()))
    }

    pub async fn get(&self, run_id: &str) -> Result<StoredRun> {
        let result: Option<StoredRun> = self
            .db
            .select(("run", run_id))
            .await
            .map_err(StorageError::Surreal)?;
        result.ok_or_else(|| StorageError::NotFound(run_id.to_string()))
    }

    pub async fn update_status(&self, run_id: &str, status: &str) -> Result<()> {
        let (id, st) = (run_id.to_string(), status.to_string());
        self.db
            .query("UPDATE run SET status = $status WHERE run_id = $id")
            .bind(("status", st))
            .bind(("id", id))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn complete(&self, run_id: &str, cost: f64, tokens: i64, wall_ms: i64) -> Result<()> {
        let id = run_id.to_string();
        self.db
            .query("UPDATE run SET status = 'completed', total_cost = $cost, total_tokens = $tokens, wall_ms = $wall, completed_at = $now WHERE run_id = $id")
            .bind(("cost", cost))
            .bind(("tokens", tokens))
            .bind(("wall", wall_ms))
            .bind(("now", Utc::now()))
            .bind(("id", id))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn fail(&self, run_id: &str, error: &str) -> Result<()> {
        let (id, err) = (run_id.to_string(), error.to_string());
        self.db
            .query("UPDATE run SET status = 'failed', error = $err, completed_at = $now WHERE run_id = $id")
            .bind(("err", err))
            .bind(("now", Utc::now()))
            .bind(("id", id))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn list_recent(&self, limit: usize) -> Result<Vec<StoredRun>> {
        let results: Vec<StoredRun> = self
            .db
            .query("SELECT * FROM run ORDER BY created_at DESC LIMIT $limit")
            .bind(("limit", limit))
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results)
    }
}
