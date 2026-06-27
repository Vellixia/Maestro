/// Static benchmark priors for known models, derived from public sources:
/// LMArena Elo, Artificial Analysis index, MMLU, HumanEval, SWE-bench, IFEval, MATH.
///
/// Scores are 0–100. Confidence starts at 0.3 (meaningful prior, room for probe updates).
/// Unknown models fall back to a neutral prior (50, confidence 0.05).
use core_types::{HardConstraints, OperationalProfile, SkillScore, SkillVector};

#[derive(Debug, Clone)]
pub struct ModelPrior {
    /// Lowercase pattern to match against model_id (prefix or exact).
    pub pattern: &'static str,
    pub skills: SkillVector,
    pub hard: HardConstraints,
    pub ops: OperationalProfile,
    pub prior_confidence: f32,
}

fn s(score: f32, confidence: f32) -> SkillScore {
    SkillScore { score, confidence, n_samples: 0 }
}

fn neutral() -> SkillScore {
    SkillScore { score: 50.0, confidence: 0.05, n_samples: 0 }
}

/// Ordered from most specific to least specific.
/// `lookup_prior` returns the first pattern that is a prefix of the model_id.
pub fn builtin_priors() -> Vec<ModelPrior> {
    vec![
        // ── OpenAI ──────────────────────────────────────────────────────────
        ModelPrior {
            pattern: "o4-mini",
            skills: SkillVector {
                reasoning:           s(95.0, 0.35),
                coding:              s(91.0, 0.35),
                math:                s(96.0, 0.35),
                instruction_following: s(88.0, 0.30),
                long_context_recall: s(82.0, 0.25),
                tool_calling:        s(90.0, 0.30),
                structured_output:   s(90.0, 0.30),
                factuality:          s(84.0, 0.30),
                multilingual:        s(75.0, 0.25),
                writing:             s(80.0, 0.25),
            },
            hard: HardConstraints {
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 1.10,
                cost_out_per_m: 4.40,
                ..Default::default()
            },
            prior_confidence: 0.35,
        },
        ModelPrior {
            pattern: "gpt-4o-mini",
            skills: SkillVector {
                reasoning:           s(72.0, 0.30),
                coding:              s(70.0, 0.30),
                math:                s(68.0, 0.30),
                instruction_following: s(80.0, 0.30),
                long_context_recall: s(72.0, 0.25),
                tool_calling:        s(82.0, 0.30),
                structured_output:   s(83.0, 0.30),
                factuality:          s(71.0, 0.25),
                multilingual:        s(72.0, 0.25),
                writing:             s(70.0, 0.25),
            },
            hard: HardConstraints {
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 0.15,
                cost_out_per_m: 0.60,
                ..Default::default()
            },
            prior_confidence: 0.30,
        },
        ModelPrior {
            pattern: "gpt-4o",
            skills: SkillVector {
                reasoning:           s(90.0, 0.35),
                coding:              s(88.0, 0.35),
                math:                s(82.0, 0.30),
                instruction_following: s(91.0, 0.35),
                long_context_recall: s(86.0, 0.30),
                tool_calling:        s(93.0, 0.35),
                structured_output:   s(92.0, 0.35),
                factuality:          s(87.0, 0.30),
                multilingual:        s(84.0, 0.30),
                writing:             s(85.0, 0.30),
            },
            hard: HardConstraints {
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 2.50,
                cost_out_per_m: 10.00,
                ..Default::default()
            },
            prior_confidence: 0.35,
        },
        // ── Anthropic ────────────────────────────────────────────────────────
        ModelPrior {
            pattern: "claude-opus-4",
            skills: SkillVector {
                reasoning:           s(96.0, 0.35),
                coding:              s(93.0, 0.35),
                math:                s(87.0, 0.35),
                instruction_following: s(94.0, 0.35),
                long_context_recall: s(91.0, 0.35),
                tool_calling:        s(93.0, 0.35),
                structured_output:   s(94.0, 0.35),
                factuality:          s(90.0, 0.35),
                multilingual:        s(86.0, 0.30),
                writing:             s(93.0, 0.35),
            },
            hard: HardConstraints {
                context_window: 200_000,
                max_output_tokens: 32_000,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 15.00,
                cost_out_per_m: 75.00,
                ..Default::default()
            },
            prior_confidence: 0.35,
        },
        ModelPrior {
            pattern: "claude-sonnet-4",
            skills: SkillVector {
                reasoning:           s(88.0, 0.35),
                coding:              s(86.0, 0.35),
                math:                s(78.0, 0.30),
                instruction_following: s(89.0, 0.35),
                long_context_recall: s(86.0, 0.30),
                tool_calling:        s(90.0, 0.35),
                structured_output:   s(91.0, 0.35),
                factuality:          s(85.0, 0.30),
                multilingual:        s(80.0, 0.30),
                writing:             s(87.0, 0.30),
            },
            hard: HardConstraints {
                context_window: 200_000,
                max_output_tokens: 64_000,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 3.00,
                cost_out_per_m: 15.00,
                ..Default::default()
            },
            prior_confidence: 0.35,
        },
        ModelPrior {
            pattern: "claude-haiku-4",
            skills: SkillVector {
                reasoning:           s(70.0, 0.30),
                coding:              s(68.0, 0.30),
                math:                s(62.0, 0.25),
                instruction_following: s(78.0, 0.30),
                long_context_recall: s(72.0, 0.25),
                tool_calling:        s(80.0, 0.30),
                structured_output:   s(82.0, 0.30),
                factuality:          s(68.0, 0.25),
                multilingual:        s(68.0, 0.25),
                writing:             s(68.0, 0.25),
            },
            hard: HardConstraints {
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 0.80,
                cost_out_per_m: 4.00,
                ..Default::default()
            },
            prior_confidence: 0.30,
        },
        ModelPrior {
            pattern: "claude-haiku-3",
            skills: SkillVector {
                reasoning:           s(60.0, 0.30),
                coding:              s(58.0, 0.30),
                math:                s(55.0, 0.25),
                instruction_following: s(72.0, 0.30),
                long_context_recall: s(62.0, 0.25),
                tool_calling:        s(70.0, 0.30),
                structured_output:   s(72.0, 0.30),
                factuality:          s(62.0, 0.25),
                multilingual:        s(62.0, 0.25),
                writing:             s(60.0, 0.25),
            },
            hard: HardConstraints {
                context_window: 200_000,
                max_output_tokens: 4_096,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 0.25,
                cost_out_per_m: 1.25,
                ..Default::default()
            },
            prior_confidence: 0.30,
        },
        // ── Google Gemini ─────────────────────────────────────────────────────
        ModelPrior {
            pattern: "gemini-2.5-pro",
            skills: SkillVector {
                reasoning:           s(93.0, 0.35),
                coding:              s(89.0, 0.35),
                math:                s(90.0, 0.35),
                instruction_following: s(88.0, 0.30),
                long_context_recall: s(92.0, 0.35),
                tool_calling:        s(88.0, 0.30),
                structured_output:   s(89.0, 0.30),
                factuality:          s(88.0, 0.30),
                multilingual:        s(85.0, 0.30),
                writing:             s(85.0, 0.30),
            },
            hard: HardConstraints {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_vision: true,
                supports_audio_input: true,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 1.25,
                cost_out_per_m: 10.00,
                ..Default::default()
            },
            prior_confidence: 0.35,
        },
        ModelPrior {
            pattern: "gemini-2.5-flash-lite",
            skills: SkillVector {
                reasoning:           s(60.0, 0.25),
                coding:              s(56.0, 0.25),
                math:                s(58.0, 0.25),
                instruction_following: s(68.0, 0.25),
                long_context_recall: s(70.0, 0.25),
                tool_calling:        s(65.0, 0.25),
                structured_output:   s(66.0, 0.25),
                factuality:          s(60.0, 0.20),
                multilingual:        s(62.0, 0.20),
                writing:             s(58.0, 0.20),
            },
            hard: HardConstraints {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 0.00,
                cost_out_per_m: 0.00,
                is_free_tier: true,
                rate_limit_rpm: 30,
                rate_limit_tpm: 1_000_000,
                ..Default::default()
            },
            prior_confidence: 0.25,
        },
        ModelPrior {
            pattern: "gemini-2.5-flash",
            skills: SkillVector {
                reasoning:           s(80.0, 0.30),
                coding:              s(76.0, 0.30),
                math:                s(74.0, 0.30),
                instruction_following: s(80.0, 0.30),
                long_context_recall: s(85.0, 0.30),
                tool_calling:        s(78.0, 0.25),
                structured_output:   s(80.0, 0.25),
                factuality:          s(76.0, 0.25),
                multilingual:        s(76.0, 0.25),
                writing:             s(74.0, 0.25),
            },
            hard: HardConstraints {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_vision: true,
                supports_audio_input: false,
                supports_audio_output: false,
                supports_tools: true,
                supports_json_mode: true,
                supports_streaming: true,
            },
            ops: OperationalProfile {
                cost_in_per_m: 0.30,
                cost_out_per_m: 2.50,
                ..Default::default()
            },
            prior_confidence: 0.30,
        },
    ]
}

/// Neutral fallback for unknown models.
pub fn neutral_prior() -> (SkillVector, f32) {
    let sv = SkillVector {
        reasoning: neutral(),
        coding: neutral(),
        math: neutral(),
        instruction_following: neutral(),
        long_context_recall: neutral(),
        tool_calling: neutral(),
        structured_output: neutral(),
        factuality: neutral(),
        multilingual: neutral(),
        writing: neutral(),
    };
    (sv, 0.05)
}

/// Look up a prior for a given model_id. Returns the first matching pattern.
pub fn lookup_prior(model_id: &str) -> Option<&'static ModelPrior> {
    let model_lower = model_id.to_lowercase();
    // SAFETY: builtin_priors() returns &'static str patterns but the Vec itself isn't 'static.
    // We leak a Box to get a 'static ref — acceptable for a small static table called once.
    static PRIORS: std::sync::OnceLock<Vec<ModelPrior>> = std::sync::OnceLock::new();
    let priors = PRIORS.get_or_init(builtin_priors);
    priors.iter().find(|p| model_lower.contains(p.pattern))
}
