use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::RecordId;

use crate::{
    db::Db,
    error::{Result, StorageError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredApiKey {
    pub id: Option<RecordId>,
    pub key_hash: String,
    /// Human label for identifying the key owner.
    pub label: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct ApiKeyRepo {
    db: Db,
}

impl ApiKeyRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Hash an API key with SHA-256 for storage.
    fn hash(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Create a new API key entry (store the hash).
    pub async fn create(&self, key: &str, label: &str) -> Result<StoredApiKey> {
        let key_hash = Self::hash(key);
        let record_id = key_hash[..16].to_string();
        let stored = StoredApiKey {
            id: None,
            key_hash: key_hash.clone(),
            label: label.to_string(),
            is_active: true,
            created_at: chrono::Utc::now(),
        };
        let created: Option<StoredApiKey> = self
            .db
            .create(("api_key", record_id.as_str()))
            .content(stored)
            .await
            .map_err(StorageError::Surreal)?;
        created.ok_or_else(|| StorageError::Other("create returned nothing".into()))
    }

    /// Validate a raw API key: hash it and check if the hash exists and is active.
    pub async fn validate(&self, key: &str) -> Result<bool> {
        let key_hash = Self::hash(key);
        let result: Option<StoredApiKey> = self
            .db
            .query("SELECT * FROM api_key WHERE key_hash = $hash AND is_active = true")
            .bind(("hash", key_hash))
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(result.is_some())
    }

    /// List all stored keys (without the raw key — hash only).
    pub async fn list(&self) -> Result<Vec<StoredApiKey>> {
        let results: Vec<StoredApiKey> = self
            .db
            .query("SELECT * FROM api_key ORDER BY created_at DESC")
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results)
    }

    /// Deactivate a key by hash prefix.
    pub async fn deactivate(&self, key_hash: &str) -> Result<()> {
        self.db
            .query("UPDATE api_key SET is_active = false WHERE key_hash = $hash")
            .bind(("hash", key_hash.to_string()))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }
}
