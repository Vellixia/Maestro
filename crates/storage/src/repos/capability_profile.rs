use core_types::CapabilityProfile;
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::{
    db::Db,
    error::{Result, StorageError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCapabilityProfile {
    pub id: Option<RecordId>,
    #[serde(flatten)]
    pub profile: CapabilityProfile,
    /// BGE-small embedding of the skill vector for similarity search.
    pub embedding: Option<Vec<f32>>,
}

pub struct CapabilityProfileRepo {
    db: Db,
}

impl CapabilityProfileRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn upsert(&self, profile: &CapabilityProfile) -> Result<()> {
        let record_id = format!("{}_{}", profile.connection_id.0, profile.model_id);
        let stored = StoredCapabilityProfile {
            id: None,
            profile: profile.clone(),
            embedding: None,
        };
        self.db
            .upsert::<Option<StoredCapabilityProfile>>(("capability_profile", record_id.as_str()))
            .content(stored)
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn get(&self, connection_id: &str, model_id: &str) -> Result<Option<CapabilityProfile>> {
        let record_id = format!("{connection_id}_{model_id}");
        let result: Option<StoredCapabilityProfile> = self
            .db
            .select(("capability_profile", record_id.as_str()))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(result.map(|r| r.profile))
    }

    pub async fn list_all(&self) -> Result<Vec<CapabilityProfile>> {
        let results: Vec<StoredCapabilityProfile> = self
            .db
            .query("SELECT * FROM capability_profile")
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results.into_iter().map(|r| r.profile).collect())
    }

    pub async fn list_for_connection(&self, connection_id: &str) -> Result<Vec<CapabilityProfile>> {
        let cid = connection_id.to_string();
        let results: Vec<StoredCapabilityProfile> = self
            .db
            .query("SELECT * FROM capability_profile WHERE connection_id = $cid")
            .bind(("cid", cid))
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results.into_iter().map(|r| r.profile).collect())
    }
}
