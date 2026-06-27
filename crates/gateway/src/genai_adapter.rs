use std::sync::Arc;

use genai::chat::{ChatMessage as GenaiChatMessage, ChatRole, ChatRequest, ChatStreamEvent};
use genai::resolver::AuthData;
use genai::Client as GenaiClient;
use genai::ModelIden;
use tokio::sync::mpsc;

use crate::error::{GatewayError, Result};
use crate::types::{
    self as our, ChatChoice, ChatDelta, ChatResponse, GatewayResponse,
    MessageContent, MessageRole, StreamChoice, TokenUsage,
};

/// Map our canonical messages to genai's message format.
fn adapt_messages(msgs: &[our::ChatMessage]) -> Vec<GenaiChatMessage> {
    msgs.iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::System => ChatRole::System,
                MessageRole::User => ChatRole::User,
                MessageRole::Assistant => ChatRole::Assistant,
                MessageRole::Tool => ChatRole::Tool,
            };
            let content = m.content.text().to_string();
            GenaiChatMessage::new(role, content)
        })
        .collect()
}

/// Map genai's response back to our canonical form.
fn adapt_response(resp: genai::chat::ChatResponse) -> our::ChatResponse {
    let texts: Vec<String> = resp.content.into_texts();
    let content = texts.join("\n\n");

    let choices = vec![ChatChoice {
        index: 0,
        message: our::ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
        finish_reason: resp.stop_reason.map(|r| r.to_string()),
    }];

    let usage = TokenUsage {
        prompt_tokens: resp.usage.prompt_tokens.unwrap_or(0) as u32,
        completion_tokens: resp.usage.completion_tokens.unwrap_or(0) as u32,
        total_tokens: resp.usage.total_tokens.unwrap_or(0) as u32,
    };

    ChatResponse {
        id: resp.response_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        model: resp.provider_model_iden.model_name.to_string(),
        choices,
        usage,
    }
}

/// Map a genai stream event to our stream chunk.
fn adapt_stream_event(event: ChatStreamEvent) -> Option<our::ChatStreamChunk> {
    match event {
        ChatStreamEvent::Chunk(chunk) => Some(our::ChatStreamChunk {
            id: uuid::Uuid::new_v4().to_string(),
            model: String::new(),
            choices: vec![StreamChoice {
                index: 0,
                delta: ChatDelta {
                    role: None,
                    content: Some(chunk.content),
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        }),
        ChatStreamEvent::End(end) => {
            let usage = end.captured_usage.map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens.unwrap_or(0) as u32,
                completion_tokens: u.completion_tokens.unwrap_or(0) as u32,
                total_tokens: u.total_tokens.unwrap_or(0) as u32,
            });
            let finish_reason = end.captured_stop_reason.map(|r| r.to_string());
            Some(our::ChatStreamChunk {
                id: uuid::Uuid::new_v4().to_string(),
                model: String::new(),
                choices: vec![StreamChoice {
                    index: 0,
                    delta: ChatDelta::default(),
                    finish_reason,
                }],
                usage,
            })
        }
        _ => None,
    }
}

/// Wraps a genai Client so callers can dispatch chat requests via genai
/// instead of the hand-rolled HTTP translation.
pub struct GenaiAdapter {
    client: GenaiClient,
    #[allow(dead_code)]
    connections: Option<Arc<dyn Connector>>,
}

/// Abstraction for looking up a credential by provider/model context.
pub trait Connector: Send + Sync {
    fn auth_for(&self, provider: &str, model: &str) -> Option<String>;
}

impl GenaiAdapter {
    /// Create an adapter whose resolver delegates to the given Connector.
    /// The resolver receives genai's ModelIden (adapter_kind + model_name)
    /// so it can look up the right credential.
    pub fn with_connector(connections: Arc<dyn Connector>) -> Self {
        let conn = connections.clone();
        let client = GenaiClient::builder()
            .with_auth_resolver_fn(move |model_iden: ModelIden| {
                let provider = model_iden.adapter_kind.as_lower_str();
                let model = model_iden.model_name.to_string();
                Ok(conn.auth_for(provider, &model).map(AuthData::from_single))
            })
            .build();
        Self {
            client,
            connections: Some(connections),
        }
    }

    /// Create an adapter with a bare client (no resolver).
    /// Call `chat_with_key` to pass credentials per-request.
    pub fn new() -> Self {
        Self {
            client: GenaiClient::builder().build(),
            connections: None,
        }
    }

    /// Execute a chat request using the given API key.
    /// A temporary client is created for the call so the key
    /// is not stored in the adapter.
    pub async fn chat_with_key(
        &self,
        model: &str,
        api_key: &str,
        req: our::ChatRequest,
    ) -> Result<GatewayResponse> {
        let key = api_key.to_string();
        let client = GenaiClient::builder()
            .with_auth_resolver_fn(move |_model_iden: ModelIden| {
                Ok(Some(AuthData::from_single(key.clone())))
            })
            .build();

        let is_streaming = req.stream.unwrap_or(false);
        let messages = adapt_messages(&req.messages);
        let genai_req = ChatRequest::new(messages);

        if is_streaming {
            let stream_resp = client
                .exec_chat_stream(model, genai_req, None)
                .await
                .map_err(|e| GatewayError::ProviderError {
                    status: 0,
                    body: e.to_string(),
                })?;

            let mut stream = stream_resp.stream;
            let (tx, rx) = mpsc::channel(32);
            tokio::spawn(async move {
                use futures_util::StreamExt;
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(event) => {
                            if let Some(chunk) = adapt_stream_event(event) {
                                let _ = tx.send(Ok(chunk)).await;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string())).await;
                            break;
                        }
                    }
                }
            });
            Ok(GatewayResponse::Stream(rx))
        } else {
            let resp = client
                .exec_chat(model, genai_req, None)
                .await
                .map_err(|e| GatewayError::ProviderError {
                    status: 0,
                    body: e.to_string(),
                })?;
            Ok(GatewayResponse::Complete(adapt_response(resp)))
        }
    }

    /// Resolver-based chat — see `with_connector`.
    pub async fn chat(&self, req: our::ChatRequest) -> Result<GatewayResponse> {
        let is_streaming = req.stream.unwrap_or(false);
        let messages = adapt_messages(&req.messages);
        let genai_req = ChatRequest::new(messages);

        if is_streaming {
            let stream_resp = self
                .client
                .exec_chat_stream(&req.model, genai_req, None)
                .await
                .map_err(|e| GatewayError::ProviderError {
                    status: 0,
                    body: e.to_string(),
                })?;

            let mut stream = stream_resp.stream;
            let (tx, rx) = mpsc::channel(32);
            tokio::spawn(async move {
                use futures_util::StreamExt;
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(event) => {
                            if let Some(chunk) = adapt_stream_event(event) {
                                let _ = tx.send(Ok(chunk)).await;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string())).await;
                            break;
                        }
                    }
                }
            });
            Ok(GatewayResponse::Stream(rx))
        } else {
            let resp = self
                .client
                .exec_chat(&req.model, genai_req, None)
                .await
                .map_err(|e| GatewayError::ProviderError {
                    status: 0,
                    body: e.to_string(),
                })?;
            Ok(GatewayResponse::Complete(adapt_response(resp)))
        }
    }
}

impl Default for GenaiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Provider tags that genai supports natively.
/// Custom / OpenAI-compat providers fall through to the hand-rolled path.
pub fn is_supported(tag: &str) -> bool {
    matches!(tag, "openai" | "anthropic" | "gemini")
}
