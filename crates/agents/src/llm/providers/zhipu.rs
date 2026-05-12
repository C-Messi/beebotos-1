//! Zhipu AI (智谱 AI) LLM Provider
//!
//! Implementation for Zhipu AI API (GLM-4, GLM-4V, GLM-3-Turbo, etc.)
//! API Documentation: https://open.bigmodel.cn/dev/api

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace};

use crate::llm::http_client::{LLMHttpClient, ProviderConfig};
use crate::llm::traits::*;
use crate::llm::types::*;

/// Zhipu AI API configuration
#[derive(Debug, Clone)]
pub struct ZhipuConfig {
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub timeout: std::time::Duration,
    pub retry_policy: RetryPolicy,
}

impl Default for ZhipuConfig {
    fn default() -> Self {
        Self {
            base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            api_key: String::new(),
            default_model: zhipu_models::GLM_4.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl ZhipuConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("ZHIPU_API_KEY")
            .or_else(|_| env::var("CHATGLM_API_KEY"))
            .map_err(|_| "ZHIPU_API_KEY not set".to_string())?;

        let base_url = env::var("ZHIPU_BASE_URL")
            .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4".to_string());

        let default_model =
            env::var("ZHIPU_DEFAULT_MODEL").unwrap_or_else(|_| zhipu_models::GLM_4.to_string());

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        })
    }
}

impl ProviderConfig for ZhipuConfig {
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

pub struct ZhipuProvider {
    config: ZhipuConfig,
    http_client: LLMHttpClient,
    capabilities: ProviderCapabilities,
}

impl ZhipuProvider {
    pub fn new(config: ZhipuConfig) -> Result<Self, LLMError> {
        if config.api_key.is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = LLMHttpClient::new(config.timeout)?;

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
            "Zhipu provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            capabilities,
        })
    }

    pub fn from_env() -> Result<Self, LLMError> {
        let config = ZhipuConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    fn name(&self) -> &str {
        "zhipu"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Zhipu");

        let mut req = request;
        if req.config.model.is_empty() {
            req.config.model = self.config.default_model.clone();
        }

        let body =
            serde_json::to_value(&req).map_err(|e| LLMError::Serialization(e.to_string()))?;
        let response = self
            .http_client
            .execute_with_retry(&self.config, "/chat/completions", body)
            .await?;

        let zhipu_resp: ZhipuResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let choice = zhipu_resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LLMError::Api {
                code: 500,
                message: "No choices in response".to_string(),
            })?;

        debug!(
            "Received response from Zhipu: {} tokens used",
            zhipu_resp.usage.total_tokens
        );

        let tool_calls = choice.message.tool_calls.map(|tcs| {
            tcs.into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    r#type: tc.r#type,
                    function: FunctionCall {
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                    },
                })
                .collect()
        });

        Ok(LLMResponse {
            id: zhipu_resp.id,
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: zhipu_resp.model,
            choices: vec![Choice {
                index: 0,
                message: if let Some(calls) = tool_calls {
                    Message::assistant(choice.message.content).with_tool_calls(calls)
                } else {
                    Message::assistant(choice.message.content)
                },
                finish_reason: choice.finish_reason,
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens: zhipu_resp.usage.prompt_tokens,
                completion_tokens: zhipu_resp.usage.completion_tokens,
                total_tokens: zhipu_resp.usage.total_tokens,
            }),
        })
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(100);

        let mut req = request;
        req.config.stream = Some(true);
        if req.config.model.is_empty() {
            req.config.model = self.config.default_model.clone();
        }

        let body =
            serde_json::to_value(&req).map_err(|e| LLMError::Serialization(e.to_string()))?;
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

                                match serde_json::from_str::<ZhipuStreamChunk>(data) {
                                    Ok(chunk) => {
                                        let content = chunk
                                            .choices
                                            .first()
                                            .and_then(|c| c.delta.content.clone())
                                            .unwrap_or_default();

                                        let finish_reason = chunk
                                            .choices
                                            .first()
                                            .and_then(|c| c.finish_reason.clone());

                                        let stream_chunk = StreamChunk {
                                            id: chunk.id,
                                            object: "chat.completion.chunk".to_string(),
                                            created: chrono::Utc::now().timestamp() as u64,
                                            model: String::new(),
                                            choices: vec![StreamChoice {
                                                index: 0,
                                                delta: Delta {
                                                    role: None,
                                                    content: Some(content),
                                                    tool_calls: None,
                                                },
                                                finish_reason: finish_reason.clone(),
                                            }],
                                        };

                                        if tx.send(stream_chunk).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        trace!("Failed to parse Zhipu stream chunk: {}", e);
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
                id: zhipu_models::GLM_4.to_string(),
                name: "GLM-4".to_string(),
                description: Some("Zhipu AI's most powerful model".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0001, 0.0001)),
            },
            ModelInfo {
                id: zhipu_models::GLM_4V.to_string(),
                name: "GLM-4V".to_string(),
                description: Some("Zhipu AI's vision model".to_string()),
                context_window: 8_192,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0001, 0.0001)),
            },
            ModelInfo {
                id: zhipu_models::GLM_3_TURBO.to_string(),
                name: "GLM-3 Turbo".to_string(),
                description: Some("Zhipu AI's fast and efficient model".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.000005, 0.000005)),
            },
        ])
    }
}

use futures::StreamExt;

#[derive(Debug, Deserialize)]
struct ZhipuResponse {
    id: String,
    model: String,
    choices: Vec<ZhipuChoice>,
    usage: ZhipuUsage,
}

#[derive(Debug, Deserialize)]
struct ZhipuChoice {
    message: ZhipuMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZhipuMessage {
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<ZhipuToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ZhipuUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ZhipuToolCall {
    id: String,
    r#type: String,
    function: ZhipuFunctionCall,
}

#[derive(Debug, Deserialize)]
struct ZhipuFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ZhipuStreamChunk {
    id: String,
    choices: Vec<ZhipuStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct ZhipuStreamChoice {
    delta: ZhipuDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZhipuDelta {
    content: Option<String>,
}

/// Zhipu model names
pub mod zhipu_models {
    pub const GLM_4: &str = "glm-4";
    pub const GLM_4V: &str = "glm-4v";
    pub const GLM_3_TURBO: &str = "glm-3-turbo";
}
