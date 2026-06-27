use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::{
    db::Db,
    error::{Result, StorageError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredUsage {
    pub id: Option<RecordId>,
    pub usage_id: String,
    pub connection_id: String,
    pub model_id: String,
    pub run_id: Option<String>,
    pub subtask_id: Option<String>,
    pub endpoint: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_usd: f64,
    pub status: String,
    pub ts: chrono::DateTime<chrono::Utc>,
}

pub struct UsageRepo {
    db: Db,
}

impl UsageRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn clone_with_db(&self) -> Self {
        Self { db: self.db.clone() }
    }

    pub async fn record(&self, usage: &StoredUsage) -> Result<()> {
        let id = usage.usage_id.clone();
        let owned = usage.clone();
        self.db
            .create::<Option<StoredUsage>>(("usage", id.as_str()))
            .content(owned)
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn stats_last_n_days(&self, days: u32) -> Result<UsageStats> {
        let query = r#"
            SELECT
                math::sum(prompt_tokens) AS total_prompt_tokens,
                math::sum(completion_tokens) AS total_completion_tokens,
                math::sum(cost_usd) AS total_cost_usd,
                count() AS total_requests
            FROM usage
            WHERE ts > time::now() - duration::from::days($days)
        "#;

        let mut resp = self
            .db
            .query(query)
            .bind(("days", days as i64))
            .await
            .map_err(StorageError::Surreal)?;

        let stats: Option<UsageStats> = resp.take(0).map_err(StorageError::Surreal)?;
        Ok(stats.unwrap_or_default())
    }

    pub async fn list_recent(&self, limit: usize) -> Result<Vec<StoredUsage>> {
        let results: Vec<StoredUsage> = self
            .db
            .query("SELECT * FROM usage ORDER BY ts DESC LIMIT $limit")
            .bind(("limit", limit))
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_cost_usd: f64,
    pub total_requests: i64,
}
