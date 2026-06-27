use chrono::Utc;
use core_types::{CapabilityProfile, ConnectionId, HardConstraints, OperationalProfile, SkillDimension, SkillVector};
use storage::{
    repos::{capability_profile::CapabilityProfileRepo, connection::ConnectionRepo},
    Db, StorageError,
};
use tracing::info;

pub struct ModelRegistry {
    profiles: CapabilityProfileRepo,
    _connections: ConnectionRepo,
}

impl ModelRegistry {
    pub fn new(db: Db) -> Self {
        Self {
            profiles: CapabilityProfileRepo::new(db.clone()),
            _connections: ConnectionRepo::new(db),
        }
    }

    /// Store a priors-only profile. Calibration runs separately.
    pub async fn register_with_prior(
        &self,
        connection_id: ConnectionId,
        model_id: &str,
        prior_skills: SkillVector,
        hard: HardConstraints,
        ops: OperationalProfile,
    ) -> Result<CapabilityProfile, RegistryError> {
        let profile = CapabilityProfile {
            id: format!("{}_{}", connection_id, model_id),
            connection_id: connection_id.clone(),
            model_id: model_id.to_string(),
            skills: prior_skills,
            hard,
            ops,
            calibrated_at: Utc::now(),
            calibration_source: "prior".to_string(),
        };
        self.profiles.upsert(&profile).await?;
        info!(connection_id = %connection_id, model = model_id, "registered with prior");
        Ok(profile)
    }

    /// Update an existing profile (called by calibration engine after probing).
    pub async fn update_profile(&self, profile: &CapabilityProfile) -> Result<(), RegistryError> {
        self.profiles.upsert(profile).await?;
        Ok(())
    }

    pub async fn get_profile(
        &self,
        connection_id: &str,
        model_id: &str,
    ) -> Result<Option<CapabilityProfile>, RegistryError> {
        Ok(self.profiles.get(connection_id, model_id).await?)
    }

    /// All profiles — used by the router's candidate pool.
    pub async fn list_routable(&self) -> Result<Vec<CapabilityProfile>, RegistryError> {
        Ok(self.profiles.list_all().await?)
    }

    pub async fn list_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<Vec<CapabilityProfile>, RegistryError> {
        Ok(self.profiles.list_for_connection(connection_id).await?)
    }

    /// Online update from a production verification outcome.
    /// Each verified subtask is a free labelled datapoint.
    pub async fn apply_online_update(
        &self,
        connection_id: &str,
        model_id: &str,
        dimension: SkillDimension,
        passed: bool,
    ) -> Result<(), RegistryError> {
        let Some(mut profile) = self.profiles.get(connection_id, model_id).await? else {
            return Ok(());
        };

        let score = profile.skills.get_mut(dimension);
        let new_n = score.n_samples + 1;
        let outcome = if passed { 100.0_f32 } else { 0.0_f32 };
        // Running average
        score.score = (score.score * score.n_samples as f32 + outcome) / new_n as f32;
        score.n_samples = new_n;
        // Confidence grows to 1.0 over 20 samples
        score.confidence = (new_n as f32 / 20.0).min(1.0);
        profile.calibration_source = "hybrid".to_string();

        self.profiles.upsert(&profile).await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("not found: {0}")]
    NotFound(String),
}
