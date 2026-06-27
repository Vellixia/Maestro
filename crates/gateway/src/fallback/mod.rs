//! Account fallback + rate-limit cooldown logic.
//! When a provider call fails, this module decides whether to mark the
//! connection as unavailable and for how long, then signals the caller
//! to try the next connection in the ordered list.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

/// Per-connection in-memory availability state.
/// SurrealDB is the persistent source of truth; this is the hot-path cache.
#[derive(Debug, Default)]
pub struct AvailabilityCache {
    inner: Arc<Mutex<HashMap<String, ConnectionState>>>,
}

#[derive(Debug, Clone)]
struct ConnectionState {
    cooldown_until: Option<DateTime<Utc>>,
    consecutive_errors: u32,
}

impl AvailabilityCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the connection is currently available (no active cooldown).
    pub async fn is_available(&self, connection_id: &str) -> bool {
        let map = self.inner.lock().await;
        match map.get(connection_id) {
            None => true,
            Some(state) => match state.cooldown_until {
                None => true,
                Some(until) => Utc::now() > until,
            },
        }
    }

    /// Mark a connection as rate-limited with exponential backoff.
    pub async fn mark_rate_limited(&self, connection_id: &str) {
        let mut map = self.inner.lock().await;
        let state = map.entry(connection_id.to_string()).or_insert(ConnectionState {
            cooldown_until: None,
            consecutive_errors: 0,
        });
        state.consecutive_errors = (state.consecutive_errors + 1).min(10);
        let backoff_secs = 2u64.pow(state.consecutive_errors).min(300); // max 5 min
        let until = Utc::now() + chrono::Duration::seconds(backoff_secs as i64);
        state.cooldown_until = Some(until);
        warn!(
            connection_id,
            backoff_secs, "Connection rate-limited, cooldown set"
        );
    }

    /// Mark a connection as having an auth error — longer cooldown.
    pub async fn mark_auth_failed(&self, connection_id: &str) {
        let mut map = self.inner.lock().await;
        let until = Utc::now() + chrono::Duration::seconds(120);
        map.insert(
            connection_id.to_string(),
            ConnectionState { cooldown_until: Some(until), consecutive_errors: 5 },
        );
        warn!(connection_id, "Connection auth failed, 2-min cooldown set");
    }

    /// Mark a successful call — reset error counter.
    pub async fn mark_success(&self, connection_id: &str) {
        let mut map = self.inner.lock().await;
        if let Some(state) = map.get_mut(connection_id) {
            state.consecutive_errors = 0;
            state.cooldown_until = None;
        }
    }
}

/// Classify a provider HTTP error to decide fallback strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    RateLimit,
    AuthFailed,
    /// Transient (5xx, timeout) — retry with backoff on same or next connection.
    Transient,
    /// Unrecoverable for this request (bad request, content policy).
    Permanent,
}

pub fn classify_error(status: u16, body: &str) -> ErrorClass {
    match status {
        429 => ErrorClass::RateLimit,
        401 | 403 => ErrorClass::AuthFailed,
        400 | 404 | 422 => ErrorClass::Permanent,
        500 | 502 | 503 | 504 => ErrorClass::Transient,
        _ => {
            // Body heuristics for providers that don't use correct HTTP status codes
            let lower = body.to_lowercase();
            if lower.contains("rate limit") || lower.contains("too many requests") || lower.contains("quota exceeded") {
                ErrorClass::RateLimit
            } else if lower.contains("invalid api key") || lower.contains("unauthorized") {
                ErrorClass::AuthFailed
            } else {
                ErrorClass::Transient
            }
        }
    }
}
