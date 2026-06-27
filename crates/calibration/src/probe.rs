use core_types::SkillDimension;

/// How a probe response is graded.
#[derive(Debug, Clone)]
pub enum GraderKind {
    /// Response must contain one of these strings (case-insensitive, trimmed).
    ContainsAny(Vec<String>),
    /// Response (stripped) must exactly equal this string (case-insensitive).
    ExactMatch(String),
    /// Response must parse as a number within tolerance of the expected value.
    Numeric { expected: f64, tolerance: f64 },
    /// Response must be valid JSON satisfying this schema (subset check).
    JsonSchema(serde_json::Value),
    /// Graded by an LLM judge using this rubric. Skipped if no anchor available.
    LlmJudge { rubric: String, pass_threshold: f32 },
}

#[derive(Debug, Clone)]
pub struct ProbeItem {
    pub dimension: SkillDimension,
    /// The user prompt sent to the model under test.
    pub prompt: String,
    /// Optional system instruction for the probe.
    pub system: Option<String>,
    pub grader: GraderKind,
}

impl ProbeItem {
    pub fn contains(dim: SkillDimension, prompt: &str, any_of: &[&str]) -> Self {
        Self {
            dimension: dim,
            prompt: prompt.to_string(),
            system: None,
            grader: GraderKind::ContainsAny(any_of.iter().map(|s| s.to_string()).collect()),
        }
    }

    pub fn exact(dim: SkillDimension, prompt: &str, expected: &str) -> Self {
        Self {
            dimension: dim,
            prompt: prompt.to_string(),
            system: None,
            grader: GraderKind::ExactMatch(expected.to_string()),
        }
    }

    pub fn numeric(dim: SkillDimension, prompt: &str, expected: f64, tol: f64) -> Self {
        Self {
            dimension: dim,
            prompt: prompt.to_string(),
            system: None,
            grader: GraderKind::Numeric { expected, tolerance: tol },
        }
    }

    pub fn json_schema(dim: SkillDimension, prompt: &str, schema: serde_json::Value) -> Self {
        Self {
            dimension: dim,
            prompt: prompt.to_string(),
            system: None,
            grader: GraderKind::JsonSchema(schema),
        }
    }

    pub fn llm_judge(dim: SkillDimension, prompt: &str, rubric: &str) -> Self {
        Self {
            dimension: dim,
            prompt: prompt.to_string(),
            system: None,
            grader: GraderKind::LlmJudge {
                rubric: rubric.to_string(),
                pass_threshold: 0.6,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub dimension: SkillDimension,
    pub passed: bool,
    pub score: f32,
    pub reason: String,
}
