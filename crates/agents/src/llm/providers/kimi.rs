//! Kimi LLM Provider
//!
//! Implementation for Moonshot AI's Kimi API.
//! Kimi uses an OpenAI-compatible API format.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

use crate::llm::http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig};
use crate::llm::traits::*;
// Re-export models for public access
pub use crate::llm::types::kimi_models;
use crate::llm::types::*;

/// 🔧 P1 FIX: Provider mode for multi-provider configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProviderMode {
    /// Merge mode: Allow multiple providers, system automatically selects
    #[default]
    #[serde(rename = "merge")]
    Merge,
    /// Replace mode: Only use this provider
    #[serde(rename = "replace")]
    Replace,
}

impl std::fmt::Display for ProviderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderMode::Merge => write!(f, "merge"),
            ProviderMode::Replace => write!(f, "replace"),
        }
    }
}

/// Kimi thinking mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    /// Enabled thinking mode
    Enabled,
    /// Disabled thinking mode (fast mode, default)
    Disabled,
}

impl Default for ThinkingMode {
    fn default() -> Self {
        ThinkingMode::Disabled
    }
}

impl std::fmt::Display for ThinkingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThinkingMode::Enabled => write!(f, "enabled"),
            ThinkingMode::Disabled => write!(f, "disabled"),
        }
    }
}

impl ThinkingMode {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "enabled" => Some(ThinkingMode::Enabled),
            "disabled" => Some(ThinkingMode::Disabled),
            _ => None,
        }
    }
}

/// Kimi API configuration
#[derive(Debug, Clone)]
pub struct KimiConfig {
    /// API base URL
    pub base_url: String,
    /// API key
    pub api_key: String,
    /// Default model
    pub default_model: String,
    /// Request timeout
    pub timeout: std::time::Duration,
    /// Retry policy
    pub retry_policy: RetryPolicy,
    /// 🔧 P1 FIX: Provider mode for multi-provider configuration
    pub mode: ProviderMode,
    /// 🆕 FIX: Kimi k2.6 thinking mode (default disabled for fast mode)
    pub thinking: ThinkingMode,
}

impl Default for KimiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key: String::new(),
            default_model: kimi_models::KIMI_LATEST.to_string(),
            timeout: std::time::Duration::from_secs(120),
            retry_policy: RetryPolicy::default(),
            mode: ProviderMode::default(),     // merge by default
            thinking: ThinkingMode::default(), // disabled by default
        }
    }
}

impl KimiConfig {
    /// Create from environment variables
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("KIMI_API_KEY")
            .or_else(|_| env::var("MOONSHOT_API_KEY"))
            .map_err(|_| "KIMI_API_KEY or MOONSHOT_API_KEY not set".to_string())?;

        let base_url =
            env::var("KIMI_BASE_URL").unwrap_or_else(|_| "https://api.moonshot.cn/v1".to_string());

        let default_model =
            env::var("KIMI_DEFAULT_MODEL").unwrap_or_else(|_| kimi_models::KIMI_LATEST.to_string());

        let mode = env::var("KIMI_MODE")
            .ok()
            .and_then(|m| match m.to_lowercase().as_str() {
                "merge" => Some(ProviderMode::Merge),
                "replace" => Some(ProviderMode::Replace),
                _ => None,
            })
            .unwrap_or_default();

        let thinking = env::var("KIMI_THINKING")
            .ok()
            .and_then(|t| ThinkingMode::from_str(&t))
            .unwrap_or_default();

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(120),
            retry_policy: RetryPolicy::default(),
            mode,
            thinking,
        })
    }

    /// 🔧 P1 FIX: Set provider mode
    pub fn with_mode(mut self, mode: ProviderMode) -> Self {
        self.mode = mode;
        self
    }

    /// 🔧 P1 FIX: Check if this provider should be used exclusively
    pub fn is_exclusive(&self) -> bool {
        self.mode == ProviderMode::Replace
    }

    /// 🆕 FIX: Set thinking mode
    pub fn with_thinking(mut self, thinking: ThinkingMode) -> Self {
        self.thinking = thinking;
        self
    }
}

impl ProviderConfig for KimiConfig {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn api_key(&self) -> &str {
        &self.api_key
    }

    fn timeout(&self) -> std::time::Duration {
        self.timeout
    }

    fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn build_headers(&self) -> Result<HeaderMap, LLMError> {
        let mut headers = HeaderMap::new();

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );

        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        Ok(headers)
    }
}

/// Kimi LLM Provider
pub struct KimiProvider {
    config: KimiConfig,
    http_client: LLMHttpClient,
    request_builder: OpenAIRequestBuilder,
    capabilities: ProviderCapabilities,
}

impl KimiProvider {
    /// Create new Kimi provider
    pub fn new(config: KimiConfig) -> Result<Self, LLMError> {
        if config.api_key.is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = LLMHttpClient::new(config.timeout)?;
        let request_builder = OpenAIRequestBuilder::new(config.default_model.clone());

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: true,
            json_mode: true,
            system_messages: true,
            max_context_length: 256_000, // Kimi supports very long context
            max_output_tokens: 8_192,
        };

        info!(
            "Kimi provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            request_builder,
            capabilities,
        })
    }

    /// Create from environment
    pub fn from_env() -> Result<Self, LLMError> {
        let config = KimiConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }

    /// 🆕 FIX: Check if web_search tool is present (incompatible with thinking
    /// mode)
    fn has_web_search_tool(tools: Option<&Vec<crate::llm::types::Tool>>) -> bool {
        tools
            .map(|tools| {
                tools
                    .iter()
                    .any(|t| t.function.name == "$web_search" || t.function.name == "web_search")
            })
            .unwrap_or(false)
    }
}

#[async_trait]
impl LLMProvider for KimiProvider {
    fn name(&self) -> &str {
        "kimi"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, mut request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Kimi");

        // 🆕 FIX: Kimi k2.6 only supports temperature=0.6
        if request.config.model.contains("k2.6") {
            request.config.temperature = Some(0.6);
        }

        // 🆕 FIX: Determine effective thinking mode
        // Constraint 3: $web_search tool is incompatible with thinking mode on
        // K2.6/K2.5
        let effective_thinking = if request.config.model.contains("k2.6")
            && Self::has_web_search_tool(request.config.tools.as_ref())
        {
            debug!("Web search tool detected, forcing thinking mode to disabled for Kimi k2.6");
            ThinkingMode::Disabled
        } else {
            self.config.thinking
        };

        // 🆕 FIX: Explicitly set thinking parameter (default disabled for fast mode)
        let thinking_json = serde_json::json!({"type": effective_thinking.to_string()});
        request
            .config
            .extra
            .insert("thinking".to_string(), thinking_json);

        // 🆕 FIX: Constraint 1 - tool_choice can only be "auto" or "none" for K2.6 with
        // thinking enabled
        if request.config.model.contains("k2.6") {
            if let Some(ref tool_choice) = request.config.tool_choice {
                let is_valid = matches!(tool_choice, ToolChoice::Auto(_) | ToolChoice::None(_));
                if !is_valid {
                    debug!("Invalid tool_choice for Kimi k2.6, resetting to 'auto'");
                    request.config.tool_choice = Some(ToolChoice::Auto("auto".to_string()));
                }
            }
        }

        let body = self.request_builder.build_body(request);
        let response = self
            .http_client
            .execute_with_retry(&self.config, "/chat/completions", body)
            .await?;

        let llm_response: LLMResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        debug!(
            "Received response from Kimi: {} tokens used",
            llm_response
                .usage
                .as_ref()
                .map(|u| u.total_tokens)
                .unwrap_or(0)
        );

        Ok(llm_response)
    }

    async fn complete_stream(
        &self,
        mut request: LLMRequest,
    ) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        debug!("Sending streaming request to Kimi");

        let (tx, rx) = mpsc::channel(100);

        // 🆕 FIX: Kimi k2.6 only supports temperature=0.6
        if request.config.model.contains("k2.6") {
            request.config.temperature = Some(0.6);
        }
        request.config.stream = Some(true);

        // 🆕 FIX: Determine effective thinking mode
        // Constraint 3: $web_search tool is incompatible with thinking mode on
        // K2.6/K2.5
        let effective_thinking = if request.config.model.contains("k2.6")
            && Self::has_web_search_tool(request.config.tools.as_ref())
        {
            debug!("Web search tool detected, forcing thinking mode to disabled for Kimi k2.6");
            ThinkingMode::Disabled
        } else {
            self.config.thinking
        };

        // 🆕 FIX: Explicitly set thinking parameter (default disabled for fast mode)
        let thinking_json = serde_json::json!({"type": effective_thinking.to_string()});
        request
            .config
            .extra
            .insert("thinking".to_string(), thinking_json);

        // 🆕 FIX: Constraint 1 - tool_choice can only be "auto" or "none" for K2.6 with
        // thinking enabled
        if request.config.model.contains("k2.6") {
            if let Some(ref tool_choice) = request.config.tool_choice {
                let is_valid = matches!(tool_choice, ToolChoice::Auto(_) | ToolChoice::None(_));
                if !is_valid {
                    debug!("Invalid tool_choice for Kimi k2.6, resetting to 'auto'");
                    request.config.tool_choice = Some(ToolChoice::Auto("auto".to_string()));
                }
            }
        }

        let body = self.request_builder.build_body(request);
        let response = self
            .http_client
            .stream_with_retry(&self.config, "/chat/completions", body)
            .await?;

        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let idle_timeout = std::time::Duration::from_secs(30);
            let mut buffer = String::new();
            loop {
                match tokio::time::timeout(idle_timeout, stream.next()).await {
                    Ok(Some(chunk_result)) => match chunk_result {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            let mut sent_any = false;
                            buffer.push_str(&text);

                            while let Some(newline) = buffer.find('\n') {
                                let line: String = buffer.drain(..=newline).collect();
                                let line = line.trim_end_matches(['\r', '\n']);
                                if let Some(data) = line.strip_prefix("data: ") {
                                    let data = data.trim();

                                    if data == "[DONE]" {
                                        return;
                                    }

                                    match serde_json::from_str::<StreamChunk>(data) {
                                        Ok(chunk) => {
                                            sent_any = true;
                                            if tx.send(chunk).await.is_err() {
                                                return;
                                            }
                                        }
                                        Err(e) => {
                                            trace!("Failed to parse chunk: {}", e);
                                        }
                                    }
                                }
                            }
                            if !sent_any && bytes.is_empty() {
                                trace!("Kimi stream received empty bytes chunk");
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            break;
                        }
                    },
                    Ok(None) => break,
                    Err(_) => {
                        warn!("Kimi stream idle timeout (no data for 30s), closing stream");
                        break;
                    }
                }
            }

            let remaining = buffer.trim();
            if let Some(data) = remaining.strip_prefix("data: ") {
                let data = data.trim();
                if data != "[DONE]" {
                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(chunk) => {
                            let _ = tx.send(chunk).await;
                        }
                        Err(e) => {
                            trace!("Failed to parse trailing stream chunk: {}", e);
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn health_check(&self) -> LLMResult<()> {
        let _response = self
            .http_client
            .get_with_retry(&self.config, "/models")
            .await?;
        Ok(())
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: kimi_models::KIMI_LATEST.to_string(),
                name: "Kimi Latest".to_string(),
                description: Some("Latest Kimi model with best performance".to_string()),
                context_window: 256_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.006, 0.012)), // USD per 1K tokens
            },
            ModelInfo {
                id: kimi_models::KIMI_FLASH.to_string(),
                name: "Kimi Flash".to_string(),
                description: Some("Fast and cost-effective".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.001, 0.002)),
            },
        ])
    }
}

use futures::StreamExt;
