use core_types::{RequirementProfile, SkillDimension};
use std::collections::HashMap;

/// Classify a task instruction into a RequirementProfile.
///
/// Phase 2 uses heuristics only (keyword scan + structural signals).
/// An optional LLM-powered path is left as a future upgrade.
pub fn classify(instruction: &str, context_tokens: u32) -> RequirementProfile {
    let lower = instruction.to_lowercase();
    let word_count = instruction.split_whitespace().count();

    // ── Hard-constraint detection ────────────────────────────────────────
    let needs_json_mode = lower.contains("json")
        || lower.contains("structured")
        || lower.contains("schema")
        || lower.contains("object with")
        || lower.contains("array of");

    let needs_tools = lower.contains("tool")
        || lower.contains("function call")
        || lower.contains("search")
        || lower.contains("execute")
        || lower.contains("run command");

    let needs_vision = lower.contains("image")
        || lower.contains("photo")
        || lower.contains("screenshot")
        || lower.contains("visual")
        || lower.contains("picture")
        || lower.contains("diagram");

    let needs_audio = lower.contains("audio")
        || lower.contains("speech")
        || lower.contains("voice")
        || lower.contains("transcribe");

    // Estimate context window needed: instruction tokens + context + headroom.
    let instruction_tokens = (word_count as u32 * 4 / 3).max(64);
    let min_context_tokens = (instruction_tokens + context_tokens + 512).min(200_000);

    // Expected output tokens (rough heuristic).
    let expected_output_tokens = if lower.contains("brief") || lower.contains("summary") || lower.contains("one sentence") {
        128
    } else if lower.contains("essay") || lower.contains("detailed") || lower.contains("comprehensive") || lower.contains("full") {
        2048
    } else if lower.contains("code") || lower.contains("implement") || lower.contains("write a function") {
        1024
    } else {
        512
    };

    // ── Skill dimension requirements ────────────────────────────────────
    let mut skill_minimums: HashMap<String, f32> = HashMap::new();

    // Reasoning
    if lower.contains("reason")
        || lower.contains("analyz")
        || lower.contains("logic")
        || lower.contains("deduc")
        || lower.contains("infer")
        || lower.contains("step by step")
        || lower.contains("explain why")
    {
        skill_minimums.insert(dim_key(SkillDimension::Reasoning), 70.0);
    }

    // Coding
    if lower.contains("code")
        || lower.contains("program")
        || lower.contains("function")
        || lower.contains("implement")
        || lower.contains("bug")
        || lower.contains("debug")
        || lower.contains("script")
        || lower.contains("algorithm")
        || lower.contains("class")
        || lower.contains("test")
    {
        skill_minimums.insert(dim_key(SkillDimension::Coding), 65.0);
    }

    // Math
    if lower.contains("math")
        || lower.contains("calcul")
        || lower.contains("equation")
        || lower.contains("formula")
        || lower.contains("statistic")
        || lower.contains("percent")
        || lower.contains("probability")
        || lower.contains("integral")
        || lower.contains("derivative")
    {
        skill_minimums.insert(dim_key(SkillDimension::Math), 65.0);
    }

    // Instruction following
    if lower.contains("exact")
        || lower.contains("follow")
        || lower.contains("format")
        || lower.contains("output only")
        || lower.contains("return only")
        || lower.contains("respond with")
        || needs_json_mode
    {
        skill_minimums.insert(dim_key(SkillDimension::InstructionFollowing), 60.0);
    }

    // Tool calling
    if needs_tools {
        skill_minimums.insert(dim_key(SkillDimension::ToolCalling), 65.0);
    }

    // Structured output
    if needs_json_mode {
        skill_minimums.insert(dim_key(SkillDimension::StructuredOutput), 65.0);
    }

    // Long context recall
    if min_context_tokens > 16_000 || lower.contains("document") || lower.contains("long") {
        skill_minimums.insert(dim_key(SkillDimension::LongContextRecall), 60.0);
    }

    // Factuality
    if lower.contains("fact")
        || lower.contains("accurate")
        || lower.contains("research")
        || lower.contains("what is")
        || lower.contains("who is")
        || lower.contains("when did")
        || lower.contains("capital of")
    {
        skill_minimums.insert(dim_key(SkillDimension::Factuality), 60.0);
    }

    // Multilingual
    if lower.contains("translat")
        || lower.contains("spanish")
        || lower.contains("french")
        || lower.contains("german")
        || lower.contains("chinese")
        || lower.contains("japanese")
        || lower.contains("arabic")
        || lower.contains("language")
    {
        skill_minimums.insert(dim_key(SkillDimension::Multilingual), 60.0);
    }

    // Writing
    if lower.contains("write")
        || lower.contains("essay")
        || lower.contains("article")
        || lower.contains("story")
        || lower.contains("blog")
        || lower.contains("email")
        || lower.contains("report")
        || lower.contains("creative")
    {
        skill_minimums.insert(dim_key(SkillDimension::Writing), 60.0);
    }

    // ── Stakes estimation ────────────────────────────────────────────────
    let stakes = estimate_stakes(&lower);

    // Safety margin scales with stakes: higher stakes = more buffer.
    let safety_margin = match (stakes * 100.0) as u32 {
        0..=30 => 0.0,
        31..=60 => 5.0,
        _ => 10.0,
    };

    RequirementProfile {
        skill_minimums,
        safety_margin,
        needs_vision,
        needs_audio,
        needs_tools,
        needs_json_mode,
        min_context_tokens,
        expected_output_tokens,
        stakes,
    }
}

fn dim_key(d: SkillDimension) -> String {
    format!("{d:?}").to_lowercase()
}

/// Estimate stakes 0.0–1.0 from keywords.
fn estimate_stakes(lower: &str) -> f32 {
    // High stakes
    if lower.contains("production")
        || lower.contains("critical")
        || lower.contains("security")
        || lower.contains("financial")
        || lower.contains("medical")
        || lower.contains("legal")
        || lower.contains("deploy")
        || lower.contains("important")
    {
        return 0.8;
    }
    // Medium stakes
    if lower.contains("business")
        || lower.contains("customer")
        || lower.contains("report")
        || lower.contains("analysis")
        || lower.contains("accurate")
    {
        return 0.5;
    }
    // Low stakes (explicitly casual)
    if lower.contains("test")
        || lower.contains("example")
        || lower.contains("demo")
        || lower.contains("play")
        || lower.contains("try")
        || lower.contains("quick")
        || lower.contains("simple")
        || lower.contains("just")
    {
        return 0.1;
    }
    // Default: medium-low
    0.3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coding_task_detected() {
        let r = classify("Write a Python function to sort a list", 0);
        assert!(r.skill_minimums.contains_key("coding"));
    }

    #[test]
    fn json_task_sets_json_mode() {
        let r = classify("Return a JSON object with keys name and age", 0);
        assert!(r.needs_json_mode);
        assert!(r.skill_minimums.contains_key("structuredoutput"));
    }

    #[test]
    fn vision_task_sets_vision() {
        let r = classify("Describe what is in this image", 0);
        assert!(r.needs_vision);
    }

    #[test]
    fn high_stakes_increases_safety_margin() {
        let high = classify("This is a production security critical task", 0);
        let low = classify("This is a quick test example", 0);
        assert!(high.safety_margin >= low.safety_margin);
        assert!(high.stakes > low.stakes);
    }
}
