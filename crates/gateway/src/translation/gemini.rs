//! Google Gemini generateContent API ↔ canonical (OpenAI) schema.
//! Ref: https://ai.google.dev/api/generate-content

use crate::{
    error::{GatewayError, Result},
    types::{
        ChatChoice, ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatStreamChunk,
        MessageContent, MessageRole, StreamChoice, TokenUsage,
    },
};

// ── Encoding (canonical → Gemini) ───────────────────────────────────────────

pub fn encode(req: &ChatRequest) -> Result<serde_json::Value> {
    let system_instruction: Option<serde_json::Value> =
        req.messages.iter().find_map(|m| {
            if m.role == MessageRole::System {
                Some(serde_json::json!({
                    "parts": [{"text": m.content.text()}]
                }))
            } else {
                None
            }
        });

    let contents: Vec<serde_json::Value> = req
        .messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(encode_message)
        .collect::<Result<_>>()?;

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": req.max_tokens.unwrap_or(8192),
        }
    });

    if let Some(sys) = system_instruction {
        body["systemInstruction"] = sys;
    }
    if let Some(t) = req.temperature {
        body["generationConfig"]["temperature"] = serde_json::Value::from(t);
    }
    if let Some(tp) = req.top_p {
        body["generationConfig"]["topP"] = serde_json::Value::from(tp);
    }

    if let Some(tools) = &req.tools {
        let function_declarations: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "parameters": t.function.parameters,
                })
            })
            .collect();
        body["tools"] = serde_json::json!([{"functionDeclarations": function_declarations}]);
    }

    if let Some(rf) = &req.response_format {
        if rf.kind == "json_object" || rf.kind == "json_schema" {
            body["generationConfig"]["responseMimeType"] = serde_json::Value::String("application/json".into());
            if let Some(schema) = &rf.json_schema {
                body["generationConfig"]["responseSchema"] = schema.clone();
            }
        }
    }

    Ok(body)
}

fn encode_message(msg: &ChatMessage) -> Result<serde_json::Value> {
    let role = match msg.role {
        MessageRole::User | MessageRole::Tool => "user",
        MessageRole::Assistant => "model",
        MessageRole::System => unreachable!(),
    };

    let parts = match &msg.content {
        MessageContent::Text(t) => {
            if msg.role == MessageRole::Tool {
                serde_json::json!([{
                    "functionResponse": {
                        "name": msg.name.as_deref().unwrap_or("tool"),
                        "response": {"content": t}
                    }
                }])
            } else {
                serde_json::json!([{"text": t}])
            }
        }
        MessageContent::Parts(parts) => {
            let encoded: Vec<serde_json::Value> = parts
                .iter()
                .map(|p| match p {
                    crate::types::ContentPart::Text { text } => serde_json::json!({"text": text}),
                    crate::types::ContentPart::ImageUrl { image_url } => serde_json::json!({
                        "inlineData": {"mimeType": "image/jpeg", "data": image_url.url}
                    }),
                })
                .collect();
            serde_json::Value::Array(encoded)
        }
    };

    Ok(serde_json::json!({"role": role, "parts": parts}))
}

// ── Decoding (Gemini → canonical) ───────────────────────────────────────────

pub fn decode(body: serde_json::Value) -> Result<ChatResponse> {
    let candidates = body["candidates"].as_array().cloned().unwrap_or_default();
    let usage_meta = &body["usageMetadata"];

    let prompt_tokens = usage_meta["promptTokenCount"].as_u64().unwrap_or(0) as u32;
    let output_tokens = usage_meta["candidatesTokenCount"].as_u64().unwrap_or(0) as u32;

    let choices = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| decode_candidate(i as u32, c))
        .collect::<Result<Vec<_>>>()?;

    Ok(ChatResponse {
        id: format!("gemini-{}", chrono::Utc::now().timestamp_millis()),
        model: body["modelVersion"].as_str().unwrap_or("gemini").to_string(),
        choices,
        usage: TokenUsage {
            prompt_tokens,
            completion_tokens: output_tokens,
            total_tokens: prompt_tokens + output_tokens,
        },
    })
}

fn decode_candidate(index: u32, candidate: &serde_json::Value) -> Result<ChatChoice> {
    let parts = candidate["content"]["parts"].as_array().cloned().unwrap_or_default();

    let mut text_parts: Vec<String> = vec![];
    let mut tool_calls: Vec<serde_json::Value> = vec![];

    for part in &parts {
        if let Some(text) = part["text"].as_str() {
            text_parts.push(text.to_string());
        }
        if !part["functionCall"].is_null() {
            tool_calls.push(serde_json::json!({
                "id": format!("call_{}", uuid::Uuid::new_v4()),
                "type": "function",
                "function": {
                    "name": part["functionCall"]["name"],
                    "arguments": serde_json::to_string(&part["functionCall"]["args"]).unwrap_or_default(),
                }
            }));
        }
    }

    let finish_reason = candidate["finishReason"].as_str().map(|r| match r {
        "STOP" => "stop".to_string(),
        "MAX_TOKENS" => "length".to_string(),
        "SAFETY" => "content_filter".to_string(),
        other => other.to_lowercase(),
    });

    let msg_json = if tool_calls.is_empty() {
        serde_json::json!({"role": "assistant", "content": text_parts.join("")})
    } else {
        serde_json::json!({"role": "assistant", "content": serde_json::Value::Null, "tool_calls": tool_calls})
    };

    let message: ChatMessage =
        serde_json::from_value(msg_json).map_err(|e| GatewayError::Translation(e.to_string()))?;

    Ok(ChatChoice { index, message, finish_reason })
}

pub fn decode_chunk(value: serde_json::Value) -> Result<Option<ChatStreamChunk>> {
    // Gemini streaming returns full candidate objects per chunk
    let candidates = value["candidates"].as_array().cloned().unwrap_or_default();
    if candidates.is_empty() {
        return Ok(None);
    }

    let usage_meta = &value["usageMetadata"];
    let output_tokens = usage_meta["candidatesTokenCount"].as_u64().map(|t| TokenUsage {
        prompt_tokens: usage_meta["promptTokenCount"].as_u64().unwrap_or(0) as u32,
        completion_tokens: t as u32,
        total_tokens: (usage_meta["totalTokenCount"].as_u64().unwrap_or(0)) as u32,
    });

    let choices = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let text = c["content"]["parts"][0]["text"].as_str().unwrap_or("").to_string();
            let finish_reason = c["finishReason"].as_str().map(|r| match r {
                "STOP" => "stop".to_string(),
                "MAX_TOKENS" => "length".to_string(),
                other => other.to_lowercase(),
            });
            StreamChoice {
                index: i as u32,
                delta: ChatDelta { content: Some(text), ..Default::default() },
                finish_reason,
            }
        })
        .collect();

    Ok(Some(ChatStreamChunk {
        id: format!("gemini-{}", chrono::Utc::now().timestamp_millis()),
        model: value["modelVersion"].as_str().unwrap_or("gemini").to_string(),
        choices,
        usage: output_tokens,
    }))
}
