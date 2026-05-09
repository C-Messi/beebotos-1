//! Qwen (Alibaba) LLM Provider
//!
//! Implementation for Alibaba Cloud's Qwen API.
//! Uses OpenAI-compatible API format.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::llm::http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig};
use crate::llm::traits::*;
use crate::llm::types::*;

/// Qwen API configuration
#[derive(Debug, Clone)]
pub struct QwenConfig {
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub timeout: std::time::Duration,
    pub retry_policy: RetryPolicy,
}

impl Default for QwenConfig {
    fn default() -> Self {
        Self {
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key: String::new(),
            default_model: qwen_models::QWEN_MAX.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl QwenConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("QWEN_API_KEY")
            .or_else(|_| env::var("DASHSCOPE_API_KEY"))
            .map_err(|_| "QWEN_API_KEY or DASHSCOPE_API_KEY not set".to_string())?;

        let base_url = env::var("QWEN_BASE_URL")
            .unwrap_or_else(|_| "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string());

        let default_model =
            env::var("QWEN_DEFAULT_MODEL").unwrap_or_else(|_| qwen_models::QWEN_MAX.to_string());

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        })
    }
}

impl ProviderConfig for QwenConfig {
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

pub struct QwenProvider {
    config: QwenConfig,
    http_client: LLMHttpClient,
    request_builder: OpenAIRequestBuilder,
    capabilities: ProviderCapabilities,
}

impl QwenProvider {
    pub fn new(config: QwenConfig) -> Result<Self, LLMError> {
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
            max_context_length: 128_000,
            max_output_tokens: 8_192,
        };

        info!(
            "Qwen provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            request_builder,
            capabilities,
        })
    }

    pub fn from_env() -> Result<Self, LLMError> {
        let config = QwenConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }
}

#[async_trait]
impl LLMProvider for QwenProvider {
    fn name(&self) -> &str {
        "qwen"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Qwen");

        let body = self.request_builder.build_body(request);
        let response = self
            .http_client
            .execute_with_retry(&self.config, "/chat/completions", body)
            .await?;

        let llm_response: LLMResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        Ok(llm_response)
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        debug!("Sending streaming request to Qwen");

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
                        for line in String::from_utf8_lossy(&bytes).lines() {
                            if line.starts_with("data: ") {
                                let data = &line[6..];
                                if data == "[DONE]" {
                                    break;
                                }
                                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                                    if tx.send(chunk).await.is_err() {
                                        return;
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
        let _response = self
            .http_client
            .get_with_retry(&self.config, "/models")
            .await?;
        Ok(())
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: qwen_models::QWEN_MAX.to_string(),
                name: "Qwen-Max".to_string(),
                description: Some("Best performance, 128K context".to_string()),
                context_window: 128_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.02, 0.06)),
            },
            ModelInfo {
                id: qwen_models::QWEN_PLUS.to_string(),
                name: "Qwen-Plus".to_string(),
                description: Some("Balanced performance and cost".to_string()),
                context_window: 128_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.008, 0.02)),
            },
            ModelInfo {
                id: qwen_models::QWEN_TURBO.to_string(),
                name: "Qwen-Turbo".to_string(),
                description: Some("Fast and cost-effective".to_string()),
                context_window: 128_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.003, 0.006)),
            },
            ModelInfo {
                id: qwen_models::QWEN_VL_MAX.to_string(),
                name: "Qwen-VL-Max".to_string(),
                description: Some("Vision-language model".to_string()),
                context_window: 32_000,
                max_tokens: 2_048,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.02, 0.02)),
            },
            ModelInfo {
                id: qwen_models::QWEN_CODER_PLUS.to_string(),
                name: "Qwen-Coder-Plus".to_string(),
                description: Some("Specialized for coding".to_string()),
                context_window: 128_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.008, 0.02)),
            },
        ])
    }
}

use futures::StreamExt;

pub mod qwen_models {
    pub const QWEN_MAX: &str = "qwen-max";
    pub const QWEN_PLUS: &str = "qwen-plus";
    pub const QWEN_TURBO: &str = "qwen-turbo";
    pub const QWEN_VL_MAX: &str = "qwen-vl-max";
    pub const QWEN_CODER_PLUS: &str = "qwen-coder-plus";
    pub const QWEN_2_5_72B: &str = "qwen2.5-72b-instruct";
    pub const QWEN_2_5_14B: &str = "qwen2.5-14b-instruct";
    pub const QWEN_2_5_7B: &str = "qwen2.5-7b-instruct";
}
