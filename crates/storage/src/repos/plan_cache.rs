use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::{db::Db, error::{Result, StorageError}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPlan {
    pub id: Option<RecordId>,
    /// SHA-256 hex of the normalized goal string.
    pub goal_hash: String,
    pub goal: String,
    /// Full `TaskGraph` serialized as JSON.
    pub graph_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub hit_count: u64,
}

pub struct PlanCacheRepo {
    db: Db,
}

impl PlanCacheRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub async fn get(&self, goal_hash: &str) -> Result<Option<StoredPlan>> {
        let hash = goal_hash.to_string();
        let mut resp = self
            .db
            .query("SELECT * FROM plan_cache WHERE goal_hash = $h LIMIT 1")
            .bind(("h", hash))
            .await
            .map_err(StorageError::Surreal)?;
        let mut results: Vec<StoredPlan> = resp.take(0).map_err(StorageError::Surreal)?;
        Ok(results.pop())
    }

    pub async fn put(&self, goal_hash: &str, goal: &str, graph_json: serde_json::Value) -> Result<()> {
        let plan = StoredPlan {
            id: None,
            goal_hash: goal_hash.to_string(),
            goal: goal.to_string(),
            graph_json,
            created_at: Utc::now(),
            hit_count: 0,
        };
        let _: Option<StoredPlan> = self
            .db
            .create("plan_cache")
            .content(plan)
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn increment_hit(&self, goal_hash: &str) -> Result<()> {
        let hash = goal_hash.to_string();
        self.db
            .query("UPDATE plan_cache SET hit_count = hit_count + 1 WHERE goal_hash = $h")
            .bind(("h", hash))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }
}
