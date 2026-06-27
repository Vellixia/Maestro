use chrono::Utc;
use core_types::{ConnectionId, ProviderKind};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::{
    db::Db,
    error::{Result, StorageError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConnection {
    pub id: Option<RecordId>,
    pub connection_id: String,
    pub provider: ProviderKind,
    /// Flat string tag for fast SurrealQL filtering (e.g. "openai", "anthropic", "gemini").
    pub provider_tag: String,
    pub display_name: String,
    pub auth_type: String,
    /// Credential blob. In production, values should be encrypted at rest.
    pub credentials: serde_json::Value,
    pub priority: i32,
    pub is_active: bool,
    pub cooldown_until: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct ConnectionRepo {
    db: Db,
}

impl ConnectionRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create(&self, conn: &StoredConnection) -> Result<StoredConnection> {
        let id = conn.connection_id.clone();
        let owned = conn.clone();
        let created: Option<StoredConnection> = self
            .db
            .create(("connection", id.as_str()))
            .content(owned)
            .await
            .map_err(StorageError::Surreal)?;
        created.ok_or_else(|| StorageError::Other("create returned nothing".into()))
    }

    pub async fn get(&self, id: &ConnectionId) -> Result<StoredConnection> {
        let result: Option<StoredConnection> = self
            .db
            .select(("connection", id.0.as_str()))
            .await
            .map_err(StorageError::Surreal)?;
        result.ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    pub async fn list_active(&self) -> Result<Vec<StoredConnection>> {
        let results: Vec<StoredConnection> = self
            .db
            .query("SELECT * FROM connection WHERE is_active = true ORDER BY priority ASC")
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results)
    }

    pub async fn list_by_provider_kind(&self, kind: &str) -> Result<Vec<StoredConnection>> {
        let kind = kind.to_string();
        let results: Vec<StoredConnection> = self
            .db
            .query(
                "SELECT * FROM connection WHERE is_active = true AND provider_tag = $kind ORDER BY priority ASC",
            )
            .bind(("kind", kind))
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results)
    }

    pub async fn set_cooldown(
        &self,
        id: &ConnectionId,
        until: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let id_str = id.0.clone();
        self.db
            .query("UPDATE connection SET cooldown_until = $until, updated_at = $now WHERE connection_id = $id")
            .bind(("until", until))
            .bind(("now", Utc::now()))
            .bind(("id", id_str))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn clear_cooldown(&self, id: &ConnectionId) -> Result<()> {
        let id_str = id.0.clone();
        self.db
            .query("UPDATE connection SET cooldown_until = NONE, updated_at = $now WHERE connection_id = $id")
            .bind(("now", Utc::now()))
            .bind(("id", id_str))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn delete(&self, id: &ConnectionId) -> Result<()> {
        let id_str = id.0.clone();
        self.db
            .query("DELETE connection WHERE connection_id = $id")
            .bind(("id", id_str))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }
}
