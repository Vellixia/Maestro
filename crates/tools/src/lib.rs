use chrono::Utc;
use gateway::types::{FunctionDef, ToolDefinition};
use tracing::{debug, warn};

/// Executor for built-in agentic tools.
pub struct ToolExecutor {
    http: reqwest::Client,
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent("maestro-tools/0.1")
                .build()
                .unwrap_or_default(),
        }
    }

    /// OpenAI-compatible tool definitions for all built-in tools.
    pub fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                kind: "function".into(),
                function: FunctionDef {
                    name: "web_search".into(),
                    description: Some(
                        "Search the web for information. Returns a brief summary of relevant results."
                        .into(),
                    ),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "The search query"
                            }
                        },
                        "required": ["query"]
                    }),
                },
            },
            ToolDefinition {
                kind: "function".into(),
                function: FunctionDef {
                    name: "calculator".into(),
                    description: Some(
                        "Evaluate a mathematical expression. Supports +, -, *, /, ^, sqrt(), sin(), cos(), etc."
                        .into(),
                    ),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "expression": {
                                "type": "string",
                                "description": "A mathematical expression, e.g. '2^10 + sqrt(144)'"
                            }
                        },
                        "required": ["expression"]
                    }),
                },
            },
            ToolDefinition {
                kind: "function".into(),
                function: FunctionDef {
                    name: "current_datetime".into(),
                    description: Some("Return the current UTC date and time.".into()),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {}
                    }),
                },
            },
        ]
    }

    /// Execute a named tool with JSON arguments. Always returns a String result.
    pub async fn execute(&self, name: &str, args: serde_json::Value) -> String {
        debug!(tool = name, "executing tool");
        match name {
            "web_search" => self.web_search(args).await,
            "calculator" => self.calculator(args),
            "current_datetime" => Utc::now().to_rfc3339(),
            other => format!("Unknown tool: {other}"),
        }
    }

    // ── web_search ────────────────────────────────────────────────────────────

    async fn web_search(&self, args: serde_json::Value) -> String {
        let query = match args["query"].as_str() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => return "Error: query parameter required".into(),
        };

        match self.ddg_instant_answers(&query).await {
            Ok(r) if !r.is_empty() => r,
            Ok(_) => format!("No results found for: {query}"),
            Err(e) => {
                warn!("web_search failed: {e}");
                format!("Search unavailable: {e}")
            }
        }
    }

    async fn ddg_instant_answers(&self, query: &str) -> anyhow::Result<String> {
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding(query)
        );

        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        let mut parts: Vec<String> = Vec::new();

        if let Some(answer) = resp["Answer"].as_str() {
            if !answer.is_empty() {
                parts.push(format!("Answer: {answer}"));
            }
        }
        if let Some(text) = resp["AbstractText"].as_str() {
            if !text.is_empty() {
                parts.push(format!("Summary: {text}"));
            }
        }
        if let Some(topics) = resp["RelatedTopics"].as_array() {
            for t in topics.iter().take(4) {
                if let Some(text) = t["Text"].as_str() {
                    if !text.is_empty() {
                        parts.push(format!("• {text}"));
                    }
                }
            }
        }

        Ok(parts.join("\n"))
    }

    // ── calculator ────────────────────────────────────────────────────────────

    fn calculator(&self, args: serde_json::Value) -> String {
        let expr = match args["expression"].as_str() {
            Some(e) if !e.is_empty() => e,
            _ => return "Error: expression parameter required".into(),
        };

        match evalexpr::eval(expr) {
            Ok(val) => val.to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }
}

/// Minimal percent-encoding for URL query params (avoids pulling in `percent-encoding` crate).
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            ' ' => out.push('+'),
            other => {
                for byte in other.to_string().as_bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}
