//! Claude (Anthropic) LLM Provider
//!
//! Implementation for Anthropic's Claude API
//! Claude is known for its strong reasoning capabilities.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::info;

use crate::llm::traits::*;
use crate::llm::types::*;

/// Claude API configuration
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    pub base_url: String,
    pub api_key: String,
    pub version: String, // API version
    pub default_model: String,
    pub timeout: std::time::Duration,
    pub retry_policy: RetryPolicy,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com/v1".to_string(),
            api_key: String::new(),
            version: "2023-06-01".to_string(),
            default_model: claude_models::CLAUDE_3_5_SONNET.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl ClaudeConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("ANTHROPIC_API_KEY")
            .or_else(|_| env::var("CLAUDE_API_KEY"))
            .map_err(|_| "ANTHROPIC_API_KEY not set".to_string())?;

        let base_url = env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string());

        let default_model = env::var("CLAUDE_DEFAULT_MODEL")
            .unwrap_or_else(|_| claude_models::CLAUDE_3_5_SONNET.to_string());

        Ok(Self {
            base_url,
            api_key,
            version: "2023-06-01".to_string(),
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        })
    }
}

pub struct ClaudeProvider {
    config: ClaudeConfig,
    http_client: reqwest::Client,
    capabilities: ProviderCapabilities,
}

impl ClaudeProvider {
    pub fn new(config: ClaudeConfig) -> Result<Self, LLMError> {
        if config.api_key.is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LLMError::Network(e.to_string()))?;

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: true,
            json_mode: true,
            system_messages: true,
            max_context_length: 200_000,
            max_output_tokens: 8_192,
        };

        info!(
            "Claude provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            capabilities,
        })
    }

    pub fn from_env() -> Result<Self, LLMError> {
        let config = ClaudeConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }

    fn build_headers(&self) -> Result<HeaderMap, LLMError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.config.api_key)
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&self.config.version)
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(headers)
    }

    fn convert_messages(&self, messages: Vec<Message>) -> (Option<String>, Vec<serde_json::Value>) {
        let mut system_prompt = None;
        let mut claude_messages = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_prompt = Some(msg.text_content());
                }
                Role::User => {
                    claude_messages.push(json!({
                        "role": "user",
                        "content": msg.text_content()
                    }));
                }
                Role::Assistant => {
                    claude_messages.push(json!({
                        "role": "assistant",
                        "content": msg.text_content()
                    }));
                }
                _ => {}
            }
        }

        (system_prompt, claude_messages)
    }

    fn build_request_body(&self, request: LLMRequest) -> serde_json::Value {
        let (system, messages) = self.convert_messages(request.messages);
        let model = if request.config.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.config.model
        };

        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.config.max_tokens.unwrap_or(4096),
        });

        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        if let Some(temp) = request.config.temperature {
            body["temperature"] = json!(temp);
        }

        if let Some(stream) = request.config.stream {
            body["stream"] = json!(stream);
        }

        body
    }

    async fn execute_with_retry(&self, request: LLMRequest) -> LLMResult<reqwest::Response> {
        let url = format!("{}/messages", self.config.base_url);
        let headers = self.build_headers()?;
        let body = self.build_request_body(request);

        let mut attempt = 0u32;
        loop {
            let response = self
                .http_client
                .post(&url)
                .headers(headers.clone())
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        LLMError::Timeout
                    } else {
                        LLMError::Network(e.to_string())
                    }
                })?;

            if response.status().is_success() {
                return Ok(response);
            }

            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown".to_string());
            let error = match status {
                reqwest::StatusCode::UNAUTHORIZED => LLMError::Auth("Invalid key".to_string()),
                reqwest::StatusCode::TOO_MANY_REQUESTS => LLMError::RateLimit { retry_after: None },
                reqwest::StatusCode::BAD_REQUEST => LLMError::InvalidRequest(error_body),
                _ => LLMError::Api {
                    code: status.as_u16(),
                    message: error_body,
                },
            };

            if !self.config.retry_policy.should_retry(&error, attempt) {
                return Err(error);
            }
            attempt += 1;
            tokio::time::sleep(self.config.retry_policy.delay_for_attempt(attempt)).await;
        }
    }
}

#[async_trait]
impl LLMProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        let response = self.execute_with_retry(request).await?;
        let claude_resp: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let content = claude_resp
            .content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        Ok(LLMResponse {
            id: claude_resp.id,
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: claude_resp.model,
            choices: vec![Choice {
                index: 0,
                message: Message::assistant(content),
                finish_reason: Some(
                    claude_resp
                        .stop_reason
                        .unwrap_or_else(|| "stop".to_string()),
                ),
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens: claude_resp.usage.input_tokens,
                completion_tokens: claude_resp.usage.output_tokens,
                total_tokens: claude_resp.usage.input_tokens + claude_resp.usage.output_tokens,
            }),
        })
    }

    async fn complete_stream(
        &self,
        _request: LLMRequest,
    ) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        Err(LLMError::NotImplemented(
            "Streaming not yet implemented for Claude".to_string(),
        ))
    }

    async fn health_check(&self) -> LLMResult<()> {
        let test = LLMRequest {
            messages: vec![Message::user("Hi")],
            config: RequestConfig {
                max_tokens: Some(1),
                ..Default::default()
            },
        };
        self.complete(test)
            .await
            .map(|_| ())
            .map_err(|e| LLMError::Provider(format!("Health check: {}", e)))
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: claude_models::CLAUDE_3_5_SONNET.to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: Some("Best balance of intelligence and speed".to_string()),
                context_window: 200_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.003, 0.015)),
            },
            ModelInfo {
                id: claude_models::CLAUDE_3_OPUS.to_string(),
                name: "Claude 3 Opus".to_string(),
                description: Some("Most powerful model for complex tasks".to_string()),
                context_window: 200_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.015, 0.075)),
            },
            ModelInfo {
                id: claude_models::CLAUDE_3_HAIKU.to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: Some("Fastest model for lightweight actions".to_string()),
                context_window: 200_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.00025, 0.00125)),
            },
            ModelInfo {
                id: claude_models::CLAUDE_3_5_HAIKU.to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                description: Some("Updated fast model".to_string()),
                context_window: 200_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0008, 0.004)),
            },
        ])
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    id: String,
    model: String,
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    usage: ClaudeUsage,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    content_type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u32,
    output_tokens: u32,
}

pub mod claude_models {
    pub const CLAUDE_3_5_SONNET: &str = "claude-3-5-sonnet-20241022";
    pub const CLAUDE_3_OPUS: &str = "claude-3-opus-20240229";
    pub const CLAUDE_3_SONNET: &str = "claude-3-sonnet-20240229";
    pub const CLAUDE_3_HAIKU: &str = "claude-3-haiku-20240307";
    pub const CLAUDE_3_5_HAIKU: &str = "claude-3-5-haiku-20241022";
}
