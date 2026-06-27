use core_types::{SkillDimension, SkillScore, SkillVector};

use crate::probe::ProbeResult;

/// Fuse a prior skill score with probe observations.
///
/// Model: treat the prior as `effective_prior_n` virtual samples, then
/// average with actual probe results. Confidence saturates at 1.0 after 20 samples.
pub fn fuse_dimension(prior: &SkillScore, results: &[ProbeResult]) -> SkillScore {
    if results.is_empty() {
        return prior.clone();
    }

    // Prior is worth this many effective observations (higher confidence = more weight).
    let effective_prior_n = (prior.confidence * 10.0).max(0.5_f32);

    let obs_count = results.len() as f32;
    let obs_score = results.iter().map(|r| r.score).sum::<f32>() / obs_count;

    // Weighted blend.
    let total_weight = effective_prior_n + obs_count;
    let fused_score = (prior.score * effective_prior_n + obs_score * obs_count) / total_weight;

    // Confidence grows with total effective observations, capped at 1.0.
    let total_n = prior.n_samples + results.len() as u32;
    let fused_confidence = ((effective_prior_n + obs_count) / 20.0).min(1.0);

    SkillScore {
        score: fused_score.clamp(0.0, 100.0),
        confidence: fused_confidence,
        n_samples: total_n,
    }
}

/// Fuse a full SkillVector using grouped probe results.
pub fn fuse_skills(prior: &SkillVector, results: &[ProbeResult]) -> SkillVector {
    let for_dim = |dim: SkillDimension| -> Vec<ProbeResult> {
        results.iter().filter(|r| r.dimension == dim).cloned().collect()
    };

    SkillVector {
        reasoning: fuse_dimension(prior.get(SkillDimension::Reasoning), &for_dim(SkillDimension::Reasoning)),
        coding: fuse_dimension(prior.get(SkillDimension::Coding), &for_dim(SkillDimension::Coding)),
        math: fuse_dimension(prior.get(SkillDimension::Math), &for_dim(SkillDimension::Math)),
        instruction_following: fuse_dimension(
            prior.get(SkillDimension::InstructionFollowing),
            &for_dim(SkillDimension::InstructionFollowing),
        ),
        long_context_recall: fuse_dimension(
            prior.get(SkillDimension::LongContextRecall),
            &for_dim(SkillDimension::LongContextRecall),
        ),
        tool_calling: fuse_dimension(
            prior.get(SkillDimension::ToolCalling),
            &for_dim(SkillDimension::ToolCalling),
        ),
        structured_output: fuse_dimension(
            prior.get(SkillDimension::StructuredOutput),
            &for_dim(SkillDimension::StructuredOutput),
        ),
        factuality: fuse_dimension(prior.get(SkillDimension::Factuality), &for_dim(SkillDimension::Factuality)),
        multilingual: fuse_dimension(
            prior.get(SkillDimension::Multilingual),
            &for_dim(SkillDimension::Multilingual),
        ),
        writing: fuse_dimension(prior.get(SkillDimension::Writing), &for_dim(SkillDimension::Writing)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::SkillDimension;

    fn make_result(dim: SkillDimension, passed: bool) -> ProbeResult {
        ProbeResult {
            dimension: dim,
            passed,
            score: if passed { 100.0 } else { 0.0 },
            reason: String::new(),
        }
    }

    #[test]
    fn fuse_all_pass_lifts_score() {
        let prior = SkillScore { score: 50.0, confidence: 0.1, n_samples: 0 };
        let results: Vec<ProbeResult> = (0..5)
            .map(|_| make_result(SkillDimension::Reasoning, true))
            .collect();
        let fused = fuse_dimension(&prior, &results);
        assert!(fused.score > prior.score, "all-pass should lift score above prior");
        assert!(fused.confidence > prior.confidence, "confidence should increase");
    }

    #[test]
    fn fuse_all_fail_lowers_score() {
        let prior = SkillScore { score: 80.0, confidence: 0.2, n_samples: 2 };
        let results: Vec<ProbeResult> = (0..5)
            .map(|_| make_result(SkillDimension::Math, false))
            .collect();
        let fused = fuse_dimension(&prior, &results);
        assert!(fused.score < prior.score, "all-fail should lower score below prior");
    }

    #[test]
    fn no_probes_returns_prior() {
        let prior = SkillScore { score: 72.0, confidence: 0.3, n_samples: 3 };
        let fused = fuse_dimension(&prior, &[]);
        assert_eq!(fused.score, prior.score);
        assert_eq!(fused.confidence, prior.confidence);
    }
}
