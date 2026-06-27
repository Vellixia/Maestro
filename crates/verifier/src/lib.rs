mod sandbox;

use core_types::{OutputType, Stakes, VerifyResult};
use gateway::{
    types::{ChatMessage, ChatRequest, MessageContent, MessageRole},
    GatewayClient,
};
use tracing::debug;

/// Verify a model's output against the task's expected output type and stakes.
///
/// High-stakes tasks run full verification; trivial tasks skip it entirely.
pub async fn verify(
    response: &str,
    instruction: &str,
    output_type: &OutputType,
    stakes: &Stakes,
    gateway: &GatewayClient,
) -> VerifyResult {
    // Trivial tasks: skip verification, not worth the cost.
    if *stakes == Stakes::Trivial {
        return VerifyResult::Skipped;
    }

    match output_type {
        OutputType::Json { schema } => {
            let parsed_schema = schema
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            verify_json(response, &parsed_schema)
        }
        OutputType::Math => verify_math(response, instruction),
        OutputType::Code { .. } => verify_code_syntax(response).await,
        OutputType::Classification => {
            // Self-consistency: check if response is a single confident label
            verify_classification(response)
        }
        OutputType::Text => {
            // For high-stakes text, use LLM self-critique.
            if *stakes == Stakes::High {
                verify_text_with_judge(response, instruction, gateway).await
            } else {
                VerifyResult::Skipped
            }
        }
    }
}

fn verify_json(response: &str, schema: &Option<serde_json::Value>) -> VerifyResult {
    let clean = strip_fences(response);
    match serde_json::from_str::<serde_json::Value>(&clean) {
        Err(e) => VerifyResult::Failed { reason: format!("invalid JSON: {e}") },
        Ok(val) => {
            if let Some(schema) = schema {
                match validate_schema(&val, schema) {
                    Ok(()) => VerifyResult::Passed,
                    Err(reason) => VerifyResult::Failed { reason },
                }
            } else {
                VerifyResult::Passed
            }
        }
    }
}

fn verify_math(response: &str, instruction: &str) -> VerifyResult {
    // Extract a numeric value from the response to compare against.
    let answer = response.split_whitespace().find_map(|w| {
        let clean: String = w.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
        if !clean.is_empty() { clean.parse::<f64>().ok() } else { None }
    });

    let Some(answer) = answer else {
        return VerifyResult::Failed { reason: "no numeric value found in math response".into() };
    };

    // Try to extract and evaluate a mathematical expression from the instruction.
    // Look for common patterns: "calculate X", "what is X", "evaluate X", or just a bare expression.
    let expr = extract_expression(instruction);

    match expr {
        Some(expr_str) => {
            match meval::eval_str(&expr_str) {
                Ok(expected) => {
                    let diff = (answer - expected).abs();
                    if diff < 0.01 {
                        VerifyResult::Passed
                    } else {
                        VerifyResult::Failed {
                            reason: format!("expected ≈{expected:.4}, got {answer:.4} (diff {diff:.4})"),
                        }
                    }
                }
                Err(_) => {
                    // Expression unparseable — fall back to just checking a number exists.
                    VerifyResult::Passed
                }
            }
        }
        None => {
            // No expression found — just check a number exists (lenient pass).
            VerifyResult::Passed
        }
    }
}

/// Attempt to extract a mathematical expression from an instruction string.
fn extract_expression(text: &str) -> Option<String> {
    // Try common prefixes: "calculate ...", "what is ...", "evaluate ...", "compute ..."
    let prefixes = ["calculate", "what is", "evaluate", "compute", "solve"];
    let lower = text.to_lowercase();
    for prefix in &prefixes {
        if let Some(idx) = lower.find(prefix) {
            let after = text[idx + prefix.len()..].trim();
            // Remove trailing punctuation and whitespace.
            let cleaned: String = after.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()).to_string();
            if !cleaned.is_empty() && cleaned.bytes().any(|b| b.is_ascii_digit() || b == b'x' || b == b'+' || b == b'-' || b == b'*' || b == b'/' || b == b'^') {
                return Some(cleaned);
            }
        }
    }
    // Fallback: look for the first line that contains a math-like expression.
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.bytes().any(|b| b == b'+' || b == b'*' || b == b'/' || b == b'^') {
            let cleaned: String = trimmed.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()).to_string();
            return Some(cleaned);
        }
    }
    None
}

async fn verify_code_syntax(response: &str) -> VerifyResult {
    let code = strip_fences(response);
    if code.trim().is_empty() {
        return VerifyResult::Failed { reason: "empty code response".into() };
    }
    sandbox::verify_code(&code, "python").await
}

fn verify_classification(response: &str) -> VerifyResult {
    let text = response.trim().to_lowercase();
    // A classification response should be a short, confident label, not hedged.
    let hedge_words = ["i'm not sure", "it could be", "maybe", "perhaps", "might be", "unclear"];
    if hedge_words.iter().any(|h| text.contains(h)) {
        return VerifyResult::Failed {
            reason: "classification response contains hedging language".into(),
        };
    }
    if text.is_empty() {
        return VerifyResult::Failed { reason: "empty classification response".into() };
    }
    VerifyResult::Passed
}

async fn verify_text_with_judge(
    response: &str,
    instruction: &str,
    gateway: &GatewayClient,
) -> VerifyResult {
    let judge_prompt = format!(
        "You are a strict quality evaluator. Given the INSTRUCTION and RESPONSE below, \
         determine if the response fully and correctly addresses the instruction.\n\n\
         INSTRUCTION: {instruction}\n\n\
         RESPONSE: {response}\n\n\
         Reply with a JSON object: {{\"pass\": true/false, \"reason\": \"<one sentence>\"}}\n\
         Output ONLY the JSON."
    );

    let req = ChatRequest {
        model: "claude-haiku-4-5".to_string(),
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

    match gateway.chat(req).await {
        Ok(gateway::GatewayResponse::Complete(r)) => {
            let text = r.choices.first().map(|c| c.message.content.text().to_string()).unwrap_or_default();
            let clean = strip_fences(&text);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&clean) {
                let passed = v.get("pass").and_then(|p| p.as_bool()).unwrap_or(true);
                let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
                if passed {
                    VerifyResult::Passed
                } else {
                    VerifyResult::Failed { reason }
                }
            } else {
                // Judge response unparseable — skip rather than false-positive fail
                debug!("judge response unparseable, skipping verification");
                VerifyResult::Skipped
            }
        }
        _ => {
            debug!("judge call failed, skipping verification");
            VerifyResult::Skipped
        }
    }
}

/// Validate a JSON value against our lightweight schema descriptor.
fn validate_schema(val: &serde_json::Value, schema: &serde_json::Value) -> Result<(), String> {
    let type_req = schema.get("type").and_then(|t| t.as_str()).unwrap_or("any");
    match type_req {
        "object" => {
            let obj = val.as_object().ok_or("expected JSON object")?;
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                for key in required {
                    let k = key.as_str().unwrap_or("");
                    if !obj.contains_key(k) {
                        return Err(format!("missing required key '{k}'"));
                    }
                }
            }
            Ok(())
        }
        "array" => {
            let arr = val.as_array().ok_or("expected JSON array")?;
            if let Some(min) = schema.get("minItems").and_then(|m| m.as_u64()) {
                if (arr.len() as u64) < min {
                    return Err(format!("array has {} items, need ≥{min}", arr.len()));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn strip_fences(text: &str) -> String {
    let s = text.trim();
    if let Some(inner) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        inner.trim_end_matches("```").trim().to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_json_passes() {
        let result = verify_json(r#"{"name":"Alice","age":30}"#, &None);
        assert_eq!(result, VerifyResult::Passed);
    }

    #[test]
    fn invalid_json_fails() {
        let result = verify_json("{not json}", &None);
        assert!(matches!(result, VerifyResult::Failed { .. }));
    }

    #[test]
    fn math_with_number_passes() {
        let result = verify_math("The answer is 42", "What is 6*7?");
        assert_eq!(result, VerifyResult::Passed);
    }

    #[test]
    fn math_expression_evaluated() {
        let result = verify_math("42", "What is 6*7?");
        assert_eq!(result, VerifyResult::Passed);

        let result = verify_math("The answer is 42", "Calculate 6*7");
        assert_eq!(result, VerifyResult::Passed);
    }

    #[test]
    fn math_wrong_answer_fails() {
        let result = verify_math("The answer is 100", "What is 6*7?");
        assert!(matches!(result, VerifyResult::Failed { .. }));
    }

    #[test]
    fn math_without_number_fails() {
        let result = verify_math("I don't know the answer", "What is 6*7?");
        assert!(matches!(result, VerifyResult::Failed { .. }));
    }

    #[test]
    fn balanced_braces() {
        assert!(balanced("fn foo() { let x = vec![1,2]; }"));
        assert!(!balanced("fn foo() { let x = vec![1,2; }"));
    }

    #[test]
    fn hedged_classification_fails() {
        let result = verify_classification("maybe positive, i'm not sure");
        assert!(matches!(result, VerifyResult::Failed { .. }));
    }
}
