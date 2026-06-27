//! Anthropic Messages API ↔ canonical (OpenAI) schema.
//! Ref: https://docs.anthropic.com/en/api/messages

use crate::{
    error::{GatewayError, Result},
    types::{
        ChatChoice, ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatStreamChunk,
        MessageContent, MessageRole, StreamChoice, TokenUsage,
    },
};

// ── Encoding (canonical → Anthropic) ────────────────────────────────────────

pub fn encode(req: &ChatRequest) -> Result<serde_json::Value> {
    // Anthropic requires system message as a top-level field, not in messages[].
    let system: Option<String> = req.messages.iter().find_map(|m| {
        if m.role == MessageRole::System {
            Some(m.content.text().to_string())
        } else {
            None
        }
    });

    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(encode_message)
        .collect::<Result<_>>()?;

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "max_tokens": req.max_tokens.unwrap_or(4096),
    });

    if let Some(sys) = system {
        body["system"] = serde_json::Value::String(sys);
    }
    if let Some(t) = req.temperature {
        body["temperature"] = serde_json::Value::from(t);
    }
    if let Some(tp) = req.top_p {
        body["top_p"] = serde_json::Value::from(tp);
    }
    if let Some(tools) = &req.tools {
        let anthro_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                })
            })
            .collect();
        body["tools"] = serde_json::Value::Array(anthro_tools);
    }
    if req.stream == Some(true) {
        body["stream"] = serde_json::Value::Bool(true);
    }

    Ok(body)
}

fn encode_message(msg: &ChatMessage) -> Result<serde_json::Value> {
    let role = match msg.role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "user", // tool results go as user messages in Anthropic
        MessageRole::System => unreachable!("system filtered above"),
    };

    let content = match &msg.content {
        MessageContent::Text(t) => {
            if msg.role == MessageRole::Tool {
                // tool_result block
                serde_json::json!([{
                    "type": "tool_result",
                    "tool_use_id": msg.tool_call_id,
                    "content": t,
                }])
            } else {
                serde_json::json!([{"type": "text", "text": t}])
            }
        }
        MessageContent::Parts(parts) => {
            let encoded: Vec<serde_json::Value> = parts
                .iter()
                .map(|p| match p {
                    crate::types::ContentPart::Text { text } => {
                        serde_json::json!({"type": "text", "text": text})
                    }
                    crate::types::ContentPart::ImageUrl { image_url } => {
                        // Anthropic expects base64 or a URL; pass URL through as-is for now.
                        serde_json::json!({
                            "type": "image",
                            "source": {"type": "url", "url": image_url.url},
                        })
                    }
                })
                .collect();
            serde_json::Value::Array(encoded)
        }
    };

    Ok(serde_json::json!({"role": role, "content": content}))
}

// ── Decoding (Anthropic → canonical) ────────────────────────────────────────

pub fn decode(body: serde_json::Value) -> Result<ChatResponse> {
    let id = body["id"].as_str().unwrap_or("").to_string();
    let model = body["model"].as_str().unwrap_or("").to_string();

    let input_tokens = body["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
    let output_tokens = body["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

    let content_blocks = body["content"].as_array().cloned().unwrap_or_default();

    // Collect text blocks and tool_use blocks
    let mut text_parts: Vec<String> = vec![];
    let mut tool_calls: Vec<serde_json::Value> = vec![];

    for block in &content_blocks {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(t) = block["text"].as_str() {
                    text_parts.push(t.to_string());
                }
            }
            Some("tool_use") => {
                // Normalize to OpenAI tool_calls format
                tool_calls.push(serde_json::json!({
                    "id": block["id"],
                    "type": "function",
                    "function": {
                        "name": block["name"],
                        "arguments": serde_json::to_string(&block["input"]).unwrap_or_default(),
                    }
                }));
            }
            _ => {}
        }
    }

    let content = text_parts.join("");
    let finish_reason = body["stop_reason"].as_str().map(|r| match r {
        "end_turn" => "stop".to_string(),
        "tool_use" => "tool_calls".to_string(),
        "max_tokens" => "length".to_string(),
        other => other.to_string(),
    });

    let mut message = ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Text(content),
        tool_call_id: None,
        name: None,
        tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) },
    };

    // If there are tool calls, encode them in extra via serde flatten — we store them
    // as raw JSON in a wrapper struct for the response. For now embed as a serde_json extra.
    let choice_msg = if tool_calls.is_empty() {
        serde_json::json!({
            "role": "assistant",
            "content": text_parts.join(""),
        })
    } else {
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::Value::Null,
            "tool_calls": tool_calls,
        })
    };

    // Re-deserialize through canonical type
    let canonical_message: ChatMessage =
        serde_json::from_value(choice_msg).map_err(|e| GatewayError::Translation(e.to_string()))?;

    Ok(ChatResponse {
        id,
        model,
        choices: vec![ChatChoice {
            index: 0,
            message: canonical_message,
            finish_reason,
        }],
        usage: TokenUsage {
            prompt_tokens: input_tokens,
            completion_tokens: output_tokens,
            total_tokens: input_tokens + output_tokens,
        },
    })
}

pub fn decode_chunk(value: serde_json::Value) -> Result<Option<ChatStreamChunk>> {
    let event_type = value["type"].as_str().unwrap_or("");

    match event_type {
        "message_start" => {
            let id = value["message"]["id"].as_str().unwrap_or("").to_string();
            let model = value["message"]["model"].as_str().unwrap_or("").to_string();
            Ok(Some(ChatStreamChunk {
                id,
                model,
                choices: vec![StreamChoice {
                    index: 0,
                    delta: ChatDelta { role: Some("assistant".into()), ..Default::default() },
                    finish_reason: None,
                }],
                usage: None,
            }))
        }
        "content_block_delta" => {
            let text = value["delta"]["text"].as_str().unwrap_or("").to_string();
            Ok(Some(ChatStreamChunk {
                id: String::new(),
                model: String::new(),
                choices: vec![StreamChoice {
                    index: 0,
                    delta: ChatDelta { content: Some(text), ..Default::default() },
                    finish_reason: None,
                }],
                usage: None,
            }))
        }
        "message_delta" => {
            let finish_reason = value["delta"]["stop_reason"].as_str().map(|r| match r {
                "end_turn" => "stop".to_string(),
                "tool_use" => "tool_calls".to_string(),
                "max_tokens" => "length".to_string(),
                other => other.to_string(),
            });
            let output_tokens = value["usage"]["output_tokens"].as_u64().map(|t| TokenUsage {
                prompt_tokens: 0,
                completion_tokens: t as u32,
                total_tokens: t as u32,
            });
            Ok(Some(ChatStreamChunk {
                id: String::new(),
                model: String::new(),
                choices: vec![StreamChoice {
                    index: 0,
                    delta: ChatDelta::default(),
                    finish_reason,
                }],
                usage: output_tokens,
            }))
        }
        "message_stop" | "ping" | "content_block_start" | "content_block_stop" => Ok(None),
        _ => Ok(None),
    }
}
