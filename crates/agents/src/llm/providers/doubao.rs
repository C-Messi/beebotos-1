//! Doubao (ByteDance) LLM Provider
//!
//! Implementation for ByteDance's Doubao API.
//! Uses OpenAI-compatible API format.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::llm::http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig};
use crate::llm::traits::*;
use crate::llm::types::*;

/// Doubao API configuration
#[derive(Debug, Clone)]
pub struct DoubaoConfig {
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub timeout: std::time::Duration,
    pub retry_policy: RetryPolicy,
}

impl Default for DoubaoConfig {
    fn default() -> Self {
        Self {
            base_url: "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            api_key: String::new(),
            default_model: doubao_models::DOUBAO_PRO.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl DoubaoConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("DOUBAO_API_KEY")
            .or_else(|_| env::var("ARK_API_KEY"))
            .map_err(|_| "DOUBAO_API_KEY not set".to_string())?;

        let base_url = env::var("DOUBAO_BASE_URL")
            .unwrap_or_else(|_| "https://ark.cn-beijing.volces.com/api/v3".to_string());

        let default_model = env::var("DOUBAO_DEFAULT_MODEL")
            .unwrap_or_else(|_| doubao_models::DOUBAO_PRO.to_string());

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        })
    }
}

impl ProviderConfig for DoubaoConfig {
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

pub struct DoubaoProvider {
    config: DoubaoConfig,
    http_client: LLMHttpClient,
    request_builder: OpenAIRequestBuilder,
    capabilities: ProviderCapabilities,
}

impl DoubaoProvider {
    pub fn new(config: DoubaoConfig) -> Result<Self, LLMError> {
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
            max_output_tokens: 4_096,
        };

        info!(
            "Doubao provider initialized with model: {}",
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
        let config = DoubaoConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }
}

#[async_trait]
impl LLMProvider for DoubaoProvider {
    fn name(&self) -> &str {
        "doubao"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Doubao");

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
        debug!("Sending streaming request to Doubao");

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
                id: doubao_models::DOUBAO_PRO.to_string(),
                name: "Doubao-Pro".to_string(),
                description: Some("Flagship model, best performance".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.008, 0.008)),
            },
            ModelInfo {
                id: doubao_models::DOUBAO_LITE.to_string(),
                name: "Doubao-Lite".to_string(),
                description: Some("Fast and cost-effective".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.003, 0.003)),
            },
            ModelInfo {
                id: doubao_models::DOUBAO_VISION.to_string(),
                name: "Doubao-Vision".to_string(),
                description: Some("Vision-capable model".to_string()),
                context_window: 32_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.015, 0.015)),
            },
        ])
    }
}

use futures::StreamExt;

pub mod doubao_models {
    pub const DOUBAO_PRO: &str = "doubao-pro-128k";
    pub const DOUBAO_LITE: &str = "doubao-lite-128k";
    pub const DOUBAO_VISION: &str = "doubao-vision-lite-32k";
    pub const DOUBAO_PRO_32K: &str = "doubao-pro-32k";
    pub const DOUBAO_PRO_4K: &str = "doubao-pro-4k";
}
