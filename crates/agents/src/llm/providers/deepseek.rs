//! DeepSeek LLM Provider
//!
//! Implementation for DeepSeek API (DeepSeek-V3, DeepSeek-R1, etc.)
//! DeepSeek provides high-performance models at competitive pricing.

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace};

use crate::llm::http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig};
use crate::llm::traits::*;
use crate::llm::types::*;

/// DeepSeek API configuration
#[derive(Debug, Clone)]
pub struct DeepSeekConfig {
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
}

impl Default for DeepSeekConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
            default_model: deepseek_models::DEEPSEEK_CHAT.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl DeepSeekConfig {
    /// Create from environment variables
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key =
            env::var("DEEPSEEK_API_KEY").map_err(|_| "DEEPSEEK_API_KEY not set".to_string())?;

        let base_url = env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".to_string());

        let default_model = env::var("DEEPSEEK_DEFAULT_MODEL")
            .unwrap_or_else(|_| deepseek_models::DEEPSEEK_CHAT.to_string());

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        })
    }
}

impl ProviderConfig for DeepSeekConfig {
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
}

/// DeepSeek LLM Provider
pub struct DeepSeekProvider {
    config: DeepSeekConfig,
    http_client: LLMHttpClient,
    request_builder: OpenAIRequestBuilder,
    capabilities: ProviderCapabilities,
}

impl DeepSeekProvider {
    /// Create new DeepSeek provider
    pub fn new(config: DeepSeekConfig) -> Result<Self, LLMError> {
        if config.api_key.is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = LLMHttpClient::new(config.timeout)?;
        let request_builder = OpenAIRequestBuilder::new(config.default_model.clone());

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: false,
            json_mode: true,
            system_messages: true,
            max_context_length: 64_000,
            max_output_tokens: 8_192,
        };

        info!(
            "DeepSeek provider initialized with model: {}",
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
        let config = DeepSeekConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }
}

#[async_trait]
impl LLMProvider for DeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to DeepSeek");

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
            "Received response from DeepSeek: {} tokens used",
            llm_response
                .usage
                .as_ref()
                .map(|u| u.total_tokens)
                .unwrap_or(0)
        );

        Ok(llm_response)
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        debug!("Sending streaming request to DeepSeek");

        let (tx, rx) = mpsc::channel(100);

        let mut request = request;
        request.config.stream = Some(true);

        let body = self.request_builder.build_body(request);
        let response = self
            .http_client
            .stream_with_retry(&self.config, "/chat/completions", body)
            .await?;

        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);

                        for line in text.lines() {
                            if line.starts_with("data: ") {
                                let data = &line[6..];

                                if data == "[DONE]" {
                                    return;
                                }

                                match serde_json::from_str::<StreamChunk>(data) {
                                    Ok(chunk) => {
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
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn health_check(&self) -> LLMResult<()> {
        let test_request = LLMRequest {
            messages: vec![Message::user("Hi")],
            config: RequestConfig {
                model: self.config.default_model.clone(),
                max_tokens: Some(1),
                ..Default::default()
            },
        };

        match self.complete(test_request).await {
            Ok(_) => Ok(()),
            Err(e) => Err(LLMError::Provider(format!("Health check failed: {}", e))),
        }
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: deepseek_models::DEEPSEEK_CHAT.to_string(),
                name: "DeepSeek-V3".to_string(),
                description: Some("General purpose chat model, 671B MoE".to_string()),
                context_window: 64_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.00027, 0.0011)),
            },
            ModelInfo {
                id: deepseek_models::DEEPSEEK_REASONER.to_string(),
                name: "DeepSeek-R1".to_string(),
                description: Some("Reasoning model with CoT, excels at math/coding".to_string()),
                context_window: 64_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: false,
                    json_mode: true,
                },
                pricing: Some((0.00055, 0.00219)),
            },
        ])
    }
}

use futures::StreamExt;

/// DeepSeek model names
pub mod deepseek_models {
    pub const DEEPSEEK_CHAT: &str = "deepseek-chat";
    pub const DEEPSEEK_REASONER: &str = "deepseek-reasoner";
    pub const DEEPSEEK_V3: &str = "deepseek-chat";
    pub const DEEPSEEK_R1: &str = "deepseek-reasoner";
}
