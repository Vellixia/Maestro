use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a provider connection (a registered API key / OAuth account).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(pub String);

impl ConnectionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Which provider this connection belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Native OpenAI (api.openai.com)
    OpenAi,
    /// Native Anthropic (api.anthropic.com)
    Anthropic,
    /// Native Google Gemini (generativelanguage.googleapis.com)
    Gemini,
    /// Any provider with an OpenAI-compatible endpoint (base URL + API key)
    OpenAiCompat {
        base_url: String,
        name: String,
    },
    /// Custom — raw HTTP with provider-specific translation
    Custom {
        id: String,
        name: String,
    },
}

impl ProviderKind {
    pub fn display_name(&self) -> &str {
        match self {
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::Gemini => "Google Gemini",
            ProviderKind::OpenAiCompat { name, .. } => name.as_str(),
            ProviderKind::Custom { name, .. } => name.as_str(),
        }
    }
}

/// Skill dimensions — each scored 0–100, with a confidence 0.0–1.0.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillVector {
    pub reasoning: SkillScore,
    pub coding: SkillScore,
    pub math: SkillScore,
    pub instruction_following: SkillScore,
    pub long_context_recall: SkillScore,
    pub tool_calling: SkillScore,
    pub structured_output: SkillScore,
    pub factuality: SkillScore,
    pub multilingual: SkillScore,
    pub writing: SkillScore,
}

impl SkillVector {
    /// Return the score for a named dimension. Returns default (50, 0.0 confidence) if unknown.
    pub fn get(&self, dim: SkillDimension) -> &SkillScore {
        match dim {
            SkillDimension::Reasoning => &self.reasoning,
            SkillDimension::Coding => &self.coding,
            SkillDimension::Math => &self.math,
            SkillDimension::InstructionFollowing => &self.instruction_following,
            SkillDimension::LongContextRecall => &self.long_context_recall,
            SkillDimension::ToolCalling => &self.tool_calling,
            SkillDimension::StructuredOutput => &self.structured_output,
            SkillDimension::Factuality => &self.factuality,
            SkillDimension::Multilingual => &self.multilingual,
            SkillDimension::Writing => &self.writing,
        }
    }

    pub fn get_mut(&mut self, dim: SkillDimension) -> &mut SkillScore {
        match dim {
            SkillDimension::Reasoning => &mut self.reasoning,
            SkillDimension::Coding => &mut self.coding,
            SkillDimension::Math => &mut self.math,
            SkillDimension::InstructionFollowing => &mut self.instruction_following,
            SkillDimension::LongContextRecall => &mut self.long_context_recall,
            SkillDimension::ToolCalling => &mut self.tool_calling,
            SkillDimension::StructuredOutput => &mut self.structured_output,
            SkillDimension::Factuality => &mut self.factuality,
            SkillDimension::Multilingual => &mut self.multilingual,
            SkillDimension::Writing => &mut self.writing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillDimension {
    Reasoning,
    Coding,
    Math,
    InstructionFollowing,
    LongContextRecall,
    ToolCalling,
    StructuredOutput,
    Factuality,
    Multilingual,
    Writing,
}

impl SkillDimension {
    pub fn all() -> &'static [SkillDimension] {
        &[
            SkillDimension::Reasoning,
            SkillDimension::Coding,
            SkillDimension::Math,
            SkillDimension::InstructionFollowing,
            SkillDimension::LongContextRecall,
            SkillDimension::ToolCalling,
            SkillDimension::StructuredOutput,
            SkillDimension::Factuality,
            SkillDimension::Multilingual,
            SkillDimension::Writing,
        ]
    }
}

/// A scored skill value with confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillScore {
    /// Measured quality score 0–100. 50 = unknown/neutral prior.
    pub score: f32,
    /// Confidence in the score. 0.0 = pure prior, 1.0 = well-measured.
    pub confidence: f32,
    /// Sample count used to derive this score.
    pub n_samples: u32,
}

impl Default for SkillScore {
    fn default() -> Self {
        Self { score: 50.0, confidence: 0.0, n_samples: 0 }
    }
}

/// Hard capabilities — boolean constraints, never traded against price.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardConstraints {
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub supports_vision: bool,
    pub supports_audio_input: bool,
    pub supports_audio_output: bool,
    pub supports_tools: bool,
    pub supports_json_mode: bool,
    pub supports_streaming: bool,
}

/// Operational characteristics — measured per-connection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperationalProfile {
    /// Dollars per million input tokens.
    pub cost_in_per_m: f64,
    /// Dollars per million output tokens.
    pub cost_out_per_m: f64,
    /// Observed tokens/second (median over recent samples).
    pub latency_tok_per_sec: f32,
    /// Observed error rate 0.0–1.0.
    pub error_rate: f32,
    /// If true, this connection is a free tier with rate limits.
    pub is_free_tier: bool,
    /// Requests per minute limit (0 = unknown).
    pub rate_limit_rpm: u32,
    /// Tokens per minute limit (0 = unknown).
    pub rate_limit_tpm: u64,
}

/// Full capability profile for one provider connection + model combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub id: String,
    pub connection_id: ConnectionId,
    pub model_id: String,
    pub skills: SkillVector,
    pub hard: HardConstraints,
    pub ops: OperationalProfile,
    /// When this profile was last calibrated.
    pub calibrated_at: DateTime<Utc>,
    /// Source of the scores: "prior", "probe", or "hybrid".
    pub calibration_source: String,
}

/// Requirement profile for a single subtask (output of the classifier).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequirementProfile {
    /// Minimum required score per skill dimension. None = don't care.
    pub skill_minimums: std::collections::HashMap<String, f32>,
    /// Safety margin added on top of minimums before hard-filter.
    pub safety_margin: f32,
    /// Hard-constraint requirements.
    pub needs_vision: bool,
    pub needs_audio: bool,
    pub needs_tools: bool,
    pub needs_json_mode: bool,
    /// Minimum context window needed (in tokens).
    pub min_context_tokens: u32,
    /// Approximate output tokens expected.
    pub expected_output_tokens: u32,
    /// How important quality is: 0.0 (cost only) – 1.0 (quality only).
    pub stakes: f32,
}
