use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Wire-protocol format a provider speaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireFormat {
    OpenAi,
    Anthropic,
    Gemini,
}

/// Static configuration for a provider (not credentials — those are in storage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind_tag: String,
    pub wire_format: WireFormat,
    pub base_url: String,
    /// Header name for the API key (e.g., "Authorization" or "x-api-key").
    pub auth_header: String,
    /// Prefix before the key value (e.g., "Bearer " or "").
    pub auth_prefix: String,
    /// Additional static headers (e.g., API version headers).
    pub extra_headers: HashMap<String, String>,
    /// Default models available on this provider (for validation + listing).
    pub models: Vec<BuiltinModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinModel {
    pub id: String,
    pub display_name: String,
    pub context_window: u32,
    pub max_output: u32,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_json_mode: bool,
    pub cost_in_per_m: f64,
    pub cost_out_per_m: f64,
}

/// The built-in provider registry — seeded from static data.
/// User-added OpenAI-compat providers are loaded from storage and merged at runtime.
pub struct ProviderRegistry {
    configs: HashMap<String, ProviderConfig>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut configs = HashMap::new();

        configs.insert("openai".into(), openai_config());
        configs.insert("anthropic".into(), anthropic_config());
        configs.insert("gemini".into(), gemini_config());

        Self { configs }
    }

    pub fn get(&self, kind_tag: &str) -> Option<&ProviderConfig> {
        self.configs.get(kind_tag)
    }

    /// Register a user-defined OpenAI-compatible provider.
    pub fn register_compat(&mut self, tag: String, base_url: String, api_key_header: Option<String>) {
        let header = api_key_header.unwrap_or_else(|| "Authorization".into());
        let prefix = if header.to_lowercase() == "authorization" { "Bearer " } else { "" }.into();

        self.configs.insert(tag.clone(), ProviderConfig {
            kind_tag: tag,
            wire_format: WireFormat::OpenAi,
            base_url,
            auth_header: header,
            auth_prefix: prefix,
            extra_headers: HashMap::new(),
            models: vec![],
        });
    }

    pub fn all(&self) -> impl Iterator<Item = &ProviderConfig> {
        self.configs.values()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn openai_config() -> ProviderConfig {
    ProviderConfig {
        kind_tag: "openai".into(),
        wire_format: WireFormat::OpenAi,
        base_url: "https://api.openai.com/v1".into(),
        auth_header: "Authorization".into(),
        auth_prefix: "Bearer ".into(),
        extra_headers: HashMap::new(),
        models: vec![
            BuiltinModel {
                id: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                context_window: 128_000,
                max_output: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 2.50,
                cost_out_per_m: 10.00,
            },
            BuiltinModel {
                id: "gpt-4o-mini".into(),
                display_name: "GPT-4o Mini".into(),
                context_window: 128_000,
                max_output: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 0.15,
                cost_out_per_m: 0.60,
            },
            BuiltinModel {
                id: "o4-mini".into(),
                display_name: "o4-mini".into(),
                context_window: 200_000,
                max_output: 100_000,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 1.10,
                cost_out_per_m: 4.40,
            },
        ],
    }
}

fn anthropic_config() -> ProviderConfig {
    ProviderConfig {
        kind_tag: "anthropic".into(),
        wire_format: WireFormat::Anthropic,
        base_url: "https://api.anthropic.com/v1".into(),
        auth_header: "x-api-key".into(),
        auth_prefix: "".into(),
        extra_headers: {
            let mut h = HashMap::new();
            h.insert("anthropic-version".into(), "2023-06-01".into());
            h
        },
        models: vec![
            BuiltinModel {
                id: "claude-opus-4-8".into(),
                display_name: "Claude Opus 4.8".into(),
                context_window: 200_000,
                max_output: 32_000,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 15.00,
                cost_out_per_m: 75.00,
            },
            BuiltinModel {
                id: "claude-sonnet-4-6".into(),
                display_name: "Claude Sonnet 4.6".into(),
                context_window: 200_000,
                max_output: 64_000,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 3.00,
                cost_out_per_m: 15.00,
            },
            BuiltinModel {
                id: "claude-haiku-4-5-20251001".into(),
                display_name: "Claude Haiku 4.5".into(),
                context_window: 200_000,
                max_output: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 0.80,
                cost_out_per_m: 4.00,
            },
        ],
    }
}

fn gemini_config() -> ProviderConfig {
    ProviderConfig {
        kind_tag: "gemini".into(),
        wire_format: WireFormat::Gemini,
        base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
        auth_header: "Authorization".into(),
        auth_prefix: "Bearer ".into(),
        extra_headers: HashMap::new(),
        models: vec![
            BuiltinModel {
                id: "gemini-2.5-pro".into(),
                display_name: "Gemini 2.5 Pro".into(),
                context_window: 1_048_576,
                max_output: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 1.25,
                cost_out_per_m: 10.00,
            },
            BuiltinModel {
                id: "gemini-2.5-flash".into(),
                display_name: "Gemini 2.5 Flash".into(),
                context_window: 1_048_576,
                max_output: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_json_mode: true,
                cost_in_per_m: 0.30,
                cost_out_per_m: 2.50,
            },
            BuiltinModel {
                id: "gemini-2.5-flash-lite".into(),
                display_name: "Gemini 2.5 Flash Lite".into(),
                context_window: 1_048_576,
                max_output: 65_536,
                supports_tools: true,
                supports_vision: false,
                supports_json_mode: true,
                cost_in_per_m: 0.0,
                cost_out_per_m: 0.0,
            },
        ],
    }
}
