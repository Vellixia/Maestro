//! Format translation layer.
//! OpenAI Chat Completions is the canonical internal schema.
//! We translate TO the provider's wire format on the way out,
//! and FROM it on the way back.

pub mod anthropic;
pub mod gemini;
pub mod openai;

use crate::{
    error::{GatewayError, Result},
    providers::registry::WireFormat,
    types::{ChatRequest, ChatResponse, ChatStreamChunk},
};

/// Translate a canonical ChatRequest → provider-specific JSON body.
pub fn encode_request(req: &ChatRequest, fmt: &WireFormat) -> Result<serde_json::Value> {
    match fmt {
        WireFormat::OpenAi => openai::encode(req),
        WireFormat::Anthropic => anthropic::encode(req),
        WireFormat::Gemini => gemini::encode(req),
    }
}

/// Translate a provider-specific response JSON → canonical ChatResponse.
pub fn decode_response(body: serde_json::Value, fmt: &WireFormat) -> Result<ChatResponse> {
    match fmt {
        WireFormat::OpenAi => openai::decode(body),
        WireFormat::Anthropic => anthropic::decode(body),
        WireFormat::Gemini => gemini::decode(body),
    }
}

/// Translate a provider-specific SSE data line → canonical ChatStreamChunk.
/// Returns `None` for `[DONE]` or empty/comment lines.
pub fn decode_stream_chunk(
    data: &str,
    fmt: &WireFormat,
) -> Result<Option<ChatStreamChunk>> {
    if data.trim() == "[DONE]" || data.trim().is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| GatewayError::Translation(format!("bad SSE JSON: {e}")))?;
    match fmt {
        WireFormat::OpenAi => openai::decode_chunk(value),
        WireFormat::Anthropic => anthropic::decode_chunk(value),
        WireFormat::Gemini => gemini::decode_chunk(value),
    }
}
