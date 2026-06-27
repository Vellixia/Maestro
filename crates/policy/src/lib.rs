use core_types::{CapabilityProfile, OptimizationMode, Policy, PrivacyTier};

/// Evaluate whether a candidate profile is permitted by the policy.
pub fn is_permitted(profile: &CapabilityProfile, policy: &Policy) -> bool {
    // Privacy: free/public tiers blocked at Commercial or higher.
    if policy.privacy >= PrivacyTier::Commercial && profile.ops.is_free_tier {
        return false;
    }

    // Explicit block list (connection id or model id).
    let conn = &profile.connection_id.0;
    if policy.blocked_providers.iter().any(|b| b == conn || b == &profile.model_id) {
        return false;
    }

    // Explicit allow list (non-empty = whitelist mode).
    if !policy.allowed_providers.is_empty() {
        if !policy.allowed_providers.iter().any(|a| a == conn || a == &profile.model_id) {
            return false;
        }
    }

    // FreeOnly mode: only zero-cost tiers pass.
    if policy.mode == OptimizationMode::FreeOnly && !profile.ops.is_free_tier {
        return false;
    }

    true
}

/// Estimate the cost in USD for a request given a profile.
pub fn estimate_cost(profile: &CapabilityProfile, prompt_tokens: u32, output_tokens: u32) -> f64 {
    let in_cost = (prompt_tokens as f64 / 1_000_000.0) * profile.ops.cost_in_per_m;
    let out_cost = (output_tokens as f64 / 1_000_000.0) * profile.ops.cost_out_per_m;
    in_cost + out_cost
}

/// Check whether estimated cost fits within remaining budget.
pub fn within_budget(
    profile: &CapabilityProfile,
    policy: &Policy,
    prompt_tokens: u32,
    output_tokens: u32,
    spent_usd: f64,
) -> bool {
    let Some(max) = policy.max_cost_usd else {
        return true;
    };
    let estimated = estimate_cost(profile, prompt_tokens, output_tokens);
    spent_usd + estimated <= max
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use core_types::{
        CapabilityProfile, ConnectionId, HardConstraints, OperationalProfile, PrivacyTier,
        SkillVector,
    };

    fn make_profile(is_free: bool) -> CapabilityProfile {
        CapabilityProfile {
            id: "test".into(),
            connection_id: ConnectionId("conn1".into()),
            model_id: "model-a".into(),
            skills: SkillVector::default(),
            hard: HardConstraints::default(),
            ops: OperationalProfile {
                cost_in_per_m: if is_free { 0.0 } else { 1.0 },
                cost_out_per_m: if is_free { 0.0 } else { 4.0 },
                is_free_tier: is_free,
                ..Default::default()
            },
            calibrated_at: Utc::now(),
            calibration_source: "prior".into(),
        }
    }

    #[test]
    fn free_blocked_by_commercial_privacy() {
        let p = make_profile(true);
        let policy = Policy { privacy: PrivacyTier::Commercial, ..Default::default() };
        assert!(!is_permitted(&p, &policy));
    }

    #[test]
    fn paid_allowed_by_commercial_privacy() {
        let p = make_profile(false);
        let policy = Policy { privacy: PrivacyTier::Commercial, ..Default::default() };
        assert!(is_permitted(&p, &policy));
    }

    #[test]
    fn free_only_mode_blocks_paid() {
        let p = make_profile(false);
        let policy = Policy { mode: OptimizationMode::FreeOnly, ..Default::default() };
        assert!(!is_permitted(&p, &policy));
    }

    #[test]
    fn blocked_provider_rejected() {
        let p = make_profile(false);
        let policy = Policy {
            blocked_providers: vec!["conn1".into()],
            ..Default::default()
        };
        assert!(!is_permitted(&p, &policy));
    }
}
