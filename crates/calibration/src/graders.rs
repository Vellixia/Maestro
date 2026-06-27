use crate::probe::{GraderKind, ProbeResult};
use core_types::SkillDimension;

/// Grade a model response synchronously.
/// Returns None for LlmJudge (requires async; handled in engine.rs).
pub fn grade_sync(
    dimension: SkillDimension,
    response: &str,
    grader: &GraderKind,
) -> Option<ProbeResult> {
    let text = response.trim().to_lowercase();
    match grader {
        GraderKind::ContainsAny(candidates) => {
            let passed = candidates.iter().any(|c| text.contains(c.to_lowercase().as_str()));
            Some(ProbeResult {
                dimension,
                passed,
                score: if passed { 100.0 } else { 0.0 },
                reason: if passed {
                    "matched expected token".into()
                } else {
                    format!("none of {:?} found in response", candidates)
                },
            })
        }
        GraderKind::ExactMatch(expected) => {
            let passed = text == expected.to_lowercase().trim();
            Some(ProbeResult {
                dimension,
                passed,
                score: if passed { 100.0 } else { 0.0 },
                reason: if passed {
                    "exact match".into()
                } else {
                    format!("expected {:?}, got {:?}", expected, text)
                },
            })
        }
        GraderKind::Numeric { expected, tolerance } => {
            let parsed = extract_number(&text);
            let passed = parsed.map(|v| (v - expected).abs() <= *tolerance).unwrap_or(false);
            Some(ProbeResult {
                dimension,
                passed,
                score: if passed { 100.0 } else { 0.0 },
                reason: match parsed {
                    Some(v) => format!("parsed {v}, expected {expected} ± {tolerance}"),
                    None => format!("could not parse number from {:?}", text),
                },
            })
        }
        GraderKind::JsonSchema(schema) => {
            let result = grade_json_schema(response.trim(), schema);
            Some(ProbeResult {
                dimension,
                passed: result.0,
                score: if result.0 { 100.0 } else { 0.0 },
                reason: result.1,
            })
        }
        GraderKind::LlmJudge { .. } => None, // handled async by the engine
    }
}

fn extract_number(text: &str) -> Option<f64> {
    // Find first numeric token, handling negatives and decimals.
    for token in text.split_whitespace() {
        let clean: String = token
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        if let Ok(n) = clean.parse::<f64>() {
            return Some(n);
        }
    }
    None
}

/// Grade a JSON response against a lightweight schema descriptor.
/// Schema format (our internal subset, not full JSON Schema):
///   { "type": "object", "required_keys": ["k1", "k2"] }
///   { "type": "array",  "min_items": 3 }
fn grade_json_schema(response: &str, schema: &serde_json::Value) -> (bool, String) {
    // Try to extract JSON from the response (model may wrap it in ```json ... ```)
    let json_str = extract_json(response);
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => return (false, format!("invalid JSON: {e}")),
    };

    let type_req = schema.get("type").and_then(|v| v.as_str()).unwrap_or("any");
    match type_req {
        "object" => {
            let Some(obj) = parsed.as_object() else {
                return (false, "expected JSON object".into());
            };
            if let Some(required) = schema.get("required_keys").and_then(|v| v.as_array()) {
                for key in required {
                    let k = key.as_str().unwrap_or("");
                    if !obj.contains_key(k) {
                        return (false, format!("missing required key '{k}'"));
                    }
                }
            }
            (true, "valid object with required keys".into())
        }
        "array" => {
            let Some(arr) = parsed.as_array() else {
                return (false, "expected JSON array".into());
            };
            if let Some(min) = schema.get("min_items").and_then(|v| v.as_u64()) {
                if (arr.len() as u64) < min {
                    return (false, format!("array has {} items, need ≥{min}", arr.len()));
                }
            }
            (true, format!("valid array with {} items", arr.len()))
        }
        _ => {
            if parsed.is_null() {
                (false, "null value".into())
            } else {
                (true, "valid JSON".into())
            }
        }
    }
}

/// Strip markdown code fences if present and return the inner JSON string.
fn extract_json(text: &str) -> String {
    let stripped = text.trim();
    if let Some(inner) = stripped
        .strip_prefix("```json")
        .or_else(|| stripped.strip_prefix("```JSON"))
        .or_else(|| stripped.strip_prefix("```"))
    {
        inner.trim_end_matches("```").trim().to_string()
    } else {
        stripped.to_string()
    }
}

/// Grade a response using an LLM anchor model (async).
/// Returns a score 0.0–1.0 from the judge.
pub async fn grade_with_judge(
    response: &str,
    rubric: &str,
    pass_threshold: f32,
    anchor: &gateway::GatewayClient,
    anchor_model: &str,
) -> ProbeResult {
    use gateway::types::{ChatMessage, ChatRequest, MessageContent, MessageRole};

    let judge_prompt = format!(
        "You are a strict evaluator. Given the RUBRIC and MODEL RESPONSE below, \
         output a single JSON object with keys:\n\
         - \"score\": float 0.0–1.0\n\
         - \"reason\": string\n\n\
         RUBRIC:\n{rubric}\n\n\
         MODEL RESPONSE:\n{response}\n\n\
         Output ONLY the JSON object."
    );

    let req = ChatRequest {
        model: anchor_model.to_string(),
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Text(judge_prompt),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }],
        temperature: Some(0.0),
        max_tokens: Some(256),
        stream: Some(false),
        top_p: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        extra: Default::default(),
    };

    let judge_response = match anchor.chat(req).await {
        Ok(gateway::GatewayResponse::Complete(r)) => r,
        _ => {
            return ProbeResult {
                dimension: core_types::SkillDimension::Writing,
                passed: false,
                score: 0.0,
                reason: "judge call failed".into(),
            };
        }
    };

    let text = judge_response.choices.first()
        .map(|c| c.message.content.text())
        .unwrap_or("");

    let json_str = extract_json(text);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
        let score = v.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0) as f32;
        let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
        ProbeResult {
            dimension: core_types::SkillDimension::Writing,
            passed: score >= pass_threshold,
            score: score * 100.0,
            reason,
        }
    } else {
        ProbeResult {
            dimension: core_types::SkillDimension::Writing,
            passed: false,
            score: 0.0,
            reason: format!("could not parse judge JSON: {:?}", text),
        }
    }
}
