use serde::{Deserialize, Serialize};

/// Optimization mode for a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationMode {
    /// Use the cheapest model that meets quality requirements (default).
    #[default]
    CheapestViable,
    /// Minimize wall-clock latency.
    Fastest,
    /// Best quality within the budget.
    HighestQuality,
    /// Only use completely free models.
    FreeOnly,
}

/// Privacy tier — governs which providers data can be sent to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyTier {
    /// Any provider, including free/public.
    #[default]
    Public,
    /// Only providers with privacy guarantees (no free tiers).
    Commercial,
    /// Only providers configured as private/self-hosted.
    Private,
}

/// Per-request policy and budget controls.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Policy {
    /// Maximum total cost for this run in USD. None = unlimited.
    pub max_cost_usd: Option<f64>,
    /// Maximum wall-clock time in seconds. None = unlimited.
    pub max_latency_secs: Option<u64>,
    /// Minimum required skill score (0–100) on any required dimension.
    pub quality_floor: Option<f32>,
    /// Privacy level — restricts which providers may be used.
    pub privacy: PrivacyTier,
    /// Explicitly allowed provider ids (empty = all allowed by privacy tier).
    pub allowed_providers: Vec<String>,
    /// Explicitly blocked provider ids.
    pub blocked_providers: Vec<String>,
    /// Optimization objective.
    pub mode: OptimizationMode,
}
