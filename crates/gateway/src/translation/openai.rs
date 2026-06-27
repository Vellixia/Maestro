//! OpenAI ↔ canonical (identity transform — OAI is the canonical schema).

use crate::{
    error::{GatewayError, Result},
    types::{ChatRequest, ChatResponse, ChatStreamChunk},
};

pub fn encode(req: &ChatRequest) -> Result<serde_json::Value> {
    serde_json::to_value(req).map_err(|e| GatewayError::Serialization(e.to_string()))
}

pub fn decode(body: serde_json::Value) -> Result<ChatResponse> {
    serde_json::from_value(body).map_err(|e| GatewayError::Translation(e.to_string()))
}

pub fn decode_chunk(value: serde_json::Value) -> Result<Option<ChatStreamChunk>> {
    let chunk: ChatStreamChunk =
        serde_json::from_value(value).map_err(|e| GatewayError::Translation(e.to_string()))?;
    Ok(Some(chunk))
}
