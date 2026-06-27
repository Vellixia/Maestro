//! GatewayClient — the single entry point for calling any model on any provider.
//!
//! Responsibilities:
//!  - Resolve connection credentials from storage
//!  - Translate the canonical ChatRequest to the provider's wire format
//!  - Execute the HTTP call with retry/fallback across connections
//!  - Translate the response back to canonical form
//!  - Record usage in storage
//!  - Update the availability cache on success/failure

use std::sync::Arc;

use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    error::{GatewayError, Result},
    fallback::{classify_error, AvailabilityCache, ErrorClass},
    providers::{registry::ProviderRegistry, registry::WireFormat},
    stream::pipe_sse_stream,
    translation::{decode_response, encode_request},
    types::{ChatRequest, GatewayResponse},
};
use storage::{
    repos::{ConnectionRepo, UsageRepo},
    Db,
};

/// Configuration for the gateway client.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Maximum retries per request across all connections.
    pub max_retries: u32,
    /// Request timeout in seconds.
    pub request_timeout_secs: u64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self { max_retries: 3, request_timeout_secs: 120 }
    }
}

pub struct GatewayClient {
    http: Client,
    registry: Arc<ProviderRegistry>,
    availability: Arc<AvailabilityCache>,
    connections: ConnectionRepo,
    usage: UsageRepo,
    #[allow(dead_code)]
    config: GatewayConfig,
}

impl GatewayClient {
    pub fn new(db: Db, registry: Arc<ProviderRegistry>, config: GatewayConfig) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(config.request_timeout_secs))
            .build()
            .expect("failed to build reqwest client");

        Self {
            http,
            registry,
            availability: Arc::new(AvailabilityCache::new()),
            connections: ConnectionRepo::new(db.clone()),
            usage: UsageRepo::new(db),
            config,
        }
    }

    /// Execute a chat completion request.
    /// Automatically falls back across available connections for the provider.
    /// Expose the connection repo so API routes can manage connections.
    pub fn connection_repo(&self) -> &ConnectionRepo {
        &self.connections
    }

    /// Expose the provider registry for model listing.
    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    /// List all active connections (for model listing in the API).
    pub async fn list_connections(
        &self,
    ) -> std::result::Result<Vec<storage::repos::connection::StoredConnection>, storage::StorageError> {
        self.connections.list_active().await
    }

    /// Execute a chat request on a specific pre-chosen connection.
    /// Used by the auto router after capability matching — skips multi-connection fallback.
    pub async fn chat_on_connection(
        &self,
        req: ChatRequest,
        connection_id: &str,
        model_id: &str,
    ) -> Result<GatewayResponse> {
        let conn = self
            .connections
            .get(&core_types::ConnectionId(connection_id.to_string()))
            .await?;

        let provider_config = self
            .registry
            .get(&conn.provider_tag)
            .ok_or_else(|| GatewayError::Config(format!("Unknown provider: {}", conn.provider_tag)))?;

        let mut per_model_req = req;
        per_model_req.model = model_id.to_string();

        self.dispatch_single_connection(&conn, &per_model_req, provider_config, model_id).await
    }

    pub async fn chat(&self, req: ChatRequest) -> Result<GatewayResponse> {
        // Resolve which provider+connection to use.
        // `req.model` can be:
        //   "provider/model-id"  e.g. "anthropic/claude-sonnet-4-6"
        //   "model-id"           e.g. "gpt-4o"  (resolved via registry)
        let (provider_tag, model_id) = parse_model_string(&req.model);

        // Build a request per-attempt with the resolved model id
        let mut per_model_req = req.clone();
        per_model_req.model = model_id.clone();

        let provider_config = self
            .registry
            .get(&provider_tag)
            .ok_or_else(|| GatewayError::Config(format!("Unknown provider: {provider_tag}")))?;

        // Get all active connections for this provider, sorted by priority.
        let connections = self.connections.list_by_provider_kind(&provider_tag).await?;

        if connections.is_empty() {
            return Err(GatewayError::NoAvailableConnections);
        }

        let is_streaming = req.stream.unwrap_or(false);
        let mut last_error: Option<GatewayError> = None;

        for conn in &connections {
            if !self.availability.is_available(&conn.connection_id).await {
                debug!(connection_id = %conn.connection_id, "Skipping cooled-down connection");
                continue;
            }

            let api_key = extract_api_key(&conn.credentials)
                .ok_or_else(|| GatewayError::Config("No API key in credentials".into()))?;

            let body = encode_request(&per_model_req, &provider_config.wire_format)?;

            let url = build_url(&provider_config.base_url, &provider_config.wire_format, &model_id);

            debug!(
                provider = %provider_tag,
                model = %model_id,
                connection_id = %conn.connection_id,
                "Sending request"
            );

            let mut req_builder = self
                .http
                .post(&url)
                .header(&provider_config.auth_header, format!("{}{api_key}", provider_config.auth_prefix))
                .header("Content-Type", "application/json");

            for (k, v) in &provider_config.extra_headers {
                req_builder = req_builder.header(k, v);
            }

            let start = std::time::Instant::now();
            let result = req_builder.json(&body).send().await;

            match result {
                Err(e) => {
                    warn!(connection_id = %conn.connection_id, error = %e, "HTTP error");
                    self.availability.mark_rate_limited(&conn.connection_id).await;
                    last_error = Some(GatewayError::Network(e));
                    continue;
                }
                Ok(resp) => {
                    let status = resp.status().as_u16();

                    if !resp.status().is_success() {
                        let body_text = resp.text().await.unwrap_or_default();
                        let class = classify_error(status, &body_text);

                        warn!(
                            connection_id = %conn.connection_id,
                            status,
                            body = %body_text,
                            ?class,
                            "Provider error"
                        );

                        match class {
                            ErrorClass::RateLimit => {
                                self.availability.mark_rate_limited(&conn.connection_id).await;
                            }
                            ErrorClass::AuthFailed => {
                                self.availability.mark_auth_failed(&conn.connection_id).await;
                            }
                            ErrorClass::Transient | ErrorClass::Permanent => {}
                        }

                        last_error = Some(GatewayError::ProviderError { status, body: body_text });

                        if class == ErrorClass::Permanent {
                            break; // Don't try other connections for a bad request
                        }
                        continue;
                    }

                    self.availability.mark_success(&conn.connection_id).await;
                    let _latency_ms = start.elapsed().as_millis() as u64;

                    if is_streaming {
                        let fmt = provider_config.wire_format.clone();
                        let stream = resp.bytes_stream();
                        let rx = pipe_sse_stream(stream, fmt);
                        return Ok(GatewayResponse::Stream(rx));
                    } else {
                        let body_bytes = resp.bytes().await.map_err(GatewayError::Network)?;
                        let body_value: Value = serde_json::from_slice(&body_bytes)
                            .map_err(|e| GatewayError::Translation(e.to_string()))?;

                        let response = decode_response(body_value, &provider_config.wire_format)?;

                        // Record usage asynchronously — don't block the response path.
                        let usage_repo = self.usage.clone_with_db();
                        let usage_record = storage::repos::usage::StoredUsage {
                            id: None,
                            usage_id: Uuid::new_v4().to_string(),
                            connection_id: conn.connection_id.clone(),
                            model_id: model_id.clone(),
                            run_id: None,
                            subtask_id: None,
                            endpoint: "/v1/chat/completions".into(),
                            prompt_tokens: response.usage.prompt_tokens as i64,
                            completion_tokens: response.usage.completion_tokens as i64,
                            cost_usd: 0.0, // TODO: compute from pricing
                            status: "ok".into(),
                            ts: Utc::now(),
                        };
                        tokio::spawn(async move {
                            if let Err(e) = usage_repo.record(&usage_record).await {
                                warn!("Failed to record usage: {e}");
                            }
                        });

                        return Ok(GatewayResponse::Complete(response));
                    }
                }
            }
        }

        Err(last_error.unwrap_or(GatewayError::NoAvailableConnections))
    }

    /// Internal: run one HTTP call against a single pre-chosen connection.
    async fn dispatch_single_connection(
        &self,
        conn: &storage::repos::connection::StoredConnection,
        req: &ChatRequest,
        provider_config: &crate::providers::registry::ProviderConfig,
        model_id: &str,
    ) -> Result<GatewayResponse> {
        if !self.availability.is_available(&conn.connection_id).await {
            return Err(GatewayError::RateLimited { retry_after_secs: 30 });
        }

        let api_key = extract_api_key(&conn.credentials)
            .ok_or_else(|| GatewayError::Config("No API key in credentials".into()))?;

        let body = encode_request(req, &provider_config.wire_format)?;
        let url = build_url(&provider_config.base_url, &provider_config.wire_format, model_id);

        let mut req_builder = self
            .http
            .post(&url)
            .header(&provider_config.auth_header, format!("{}{api_key}", provider_config.auth_prefix))
            .header("Content-Type", "application/json");

        for (k, v) in &provider_config.extra_headers {
            req_builder = req_builder.header(k, v);
        }

        let resp = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                let avail = self.availability.clone();
                let cid = conn.connection_id.clone();
                tokio::spawn(async move { avail.mark_rate_limited(&cid).await });
                GatewayError::Network(e)
            })?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let class = classify_error(status, &body_text);
            let avail = self.availability.clone();
            let cid = conn.connection_id.clone();
            match class {
                ErrorClass::RateLimit => { tokio::spawn(async move { avail.mark_rate_limited(&cid).await }); }
                ErrorClass::AuthFailed => { tokio::spawn(async move { avail.mark_auth_failed(&cid).await }); }
                _ => {}
            }
            return Err(GatewayError::ProviderError { status, body: body_text });
        }

        self.availability.mark_success(&conn.connection_id).await;

        if req.stream.unwrap_or(false) {
            let fmt = provider_config.wire_format.clone();
            let rx = pipe_sse_stream(resp.bytes_stream(), fmt);
            return Ok(GatewayResponse::Stream(rx));
        }

        let body_bytes = resp.bytes().await.map_err(GatewayError::Network)?;
        let body_value: Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| GatewayError::Translation(e.to_string()))?;
        let response = decode_response(body_value, &provider_config.wire_format)?;

        let usage_repo = self.usage.clone_with_db();
        let usage_record = storage::repos::usage::StoredUsage {
            id: None,
            usage_id: Uuid::new_v4().to_string(),
            connection_id: conn.connection_id.clone(),
            model_id: model_id.to_string(),
            run_id: None,
            subtask_id: None,
            endpoint: "/v1/chat/completions".into(),
            prompt_tokens: response.usage.prompt_tokens as i64,
            completion_tokens: response.usage.completion_tokens as i64,
            cost_usd: 0.0,
            status: "ok".into(),
            ts: Utc::now(),
        };
        tokio::spawn(async move {
            if let Err(e) = usage_repo.record(&usage_record).await {
                warn!("Failed to record usage: {e}");
            }
        });

        Ok(GatewayResponse::Complete(response))
    }
}

/// Parse "provider/model" or "model" → (provider_tag, model_id).
fn parse_model_string(model: &str) -> (String, String) {
    if let Some((provider, model_id)) = model.split_once('/') {
        (provider.to_string(), model_id.to_string())
    } else {
        // Heuristic: well-known model prefixes → provider
        let provider = if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
            "openai"
        } else if model.starts_with("claude-") {
            "anthropic"
        } else if model.starts_with("gemini-") {
            "gemini"
        } else {
            "openai" // default to openai-compat
        };
        (provider.to_string(), model.to_string())
    }
}

fn extract_api_key(credentials: &serde_json::Value) -> Option<String> {
    credentials["api_key"]
        .as_str()
        .or_else(|| credentials["access_token"].as_str())
        .map(|s| s.to_string())
}

fn build_url(base_url: &str, format: &WireFormat, model_id: &str) -> String {
    match format {
        WireFormat::OpenAi => format!("{base_url}/chat/completions"),
        WireFormat::Anthropic => format!("{base_url}/messages"),
        WireFormat::Gemini => {
            format!("{base_url}/models/{model_id}:generateContent")
        }
    }
}
