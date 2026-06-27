use std::sync::Arc;
use std::time::Duration;

use calibration::CalibrationEngine;
use core_types::SkillDimension;
use registry::ModelRegistry;
use tracing::{info, warn};

/// Spawn a background task that re-probes stale capability profiles every `interval`.
///
/// A profile is considered stale when `calibrated_at` is older than `stale_after`.
/// The scheduler re-calibrates one random dimension per profile per cycle to spread load.
pub fn spawn_recalibration_scheduler(
    calibration: Arc<CalibrationEngine>,
    registry: Arc<ModelRegistry>,
    interval: Duration,
    stale_after: Duration,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // skip immediate first tick

        loop {
            ticker.tick().await;
            run_recalibration_cycle(&calibration, &registry, stale_after).await;
        }
    });
}

async fn run_recalibration_cycle(
    calibration: &CalibrationEngine,
    registry: &ModelRegistry,
    stale_after: Duration,
) {
    let profiles = match registry.list_routable().await {
        Ok(p) => p,
        Err(e) => {
            warn!("recalibration: failed to list profiles: {e}");
            return;
        }
    };

    let cutoff = chrono::Utc::now() - chrono::Duration::from_std(stale_after).unwrap_or_default();
    let stale: Vec<_> = profiles
        .iter()
        .filter(|p| p.calibrated_at < cutoff)
        .collect();

    if stale.is_empty() {
        return;
    }

    info!(n = stale.len(), "recalibration: re-probing stale profiles");

    for profile in &stale {
        // Pick a random dimension weighted toward ones with low confidence.
        let dim = lowest_confidence_dim(profile);
        if let Err(e) = calibration
            .recalibrate_dimension(
                &profile.connection_id.0,
                &profile.model_id,
                dim,
            )
            .await
        {
            warn!(
                connection = %profile.connection_id,
                model = %profile.model_id,
                "recalibration failed: {e}"
            );
        }
    }
}

fn lowest_confidence_dim(profile: &core_types::CapabilityProfile) -> SkillDimension {
    let dims = SkillDimension::all();
    dims.iter()
        .min_by(|a, b| {
            let ca = profile.skills.get(**a).confidence;
            let cb = profile.skills.get(**b).confidence;
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
        .unwrap_or(SkillDimension::Reasoning)
}
