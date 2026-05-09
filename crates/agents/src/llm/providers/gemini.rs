//! Gemini (Google) LLM Provider
//!
//! Implementation for Google AI's Gemini API

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::info;

use crate::llm::traits::*;
use crate::llm::types::*;

/// Gemini API configuration
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub timeout: std::time::Duration,
    pub retry_policy: RetryPolicy,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_key: String::new(),
            default_model: gemini_models::GEMINI_2_0_FLASH.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl GeminiConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("GEMINI_API_KEY")
            .or_else(|_| env::var("GOOGLE_API_KEY"))
            .map_err(|_| "GEMINI_API_KEY or GOOGLE_API_KEY not set".to_string())?;

        let base_url = env::var("GEMINI_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta".to_string());

        let default_model = env::var("GEMINI_DEFAULT_MODEL")
            .unwrap_or_else(|_| gemini_models::GEMINI_2_0_FLASH.to_string());

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
        })
    }
}

pub struct GeminiProvider {
    config: GeminiConfig,
    http_client: reqwest::Client,
    capabilities: ProviderCapabilities,
}

impl GeminiProvider {
    pub fn new(config: GeminiConfig) -> Result<Self, LLMError> {
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
            max_context_length: 1_000_000, // Gemini 1.5 Pro has 1M context
            max_output_tokens: 8_192,
        };

        info!(
            "Gemini provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            capabilities,
        })
    }

    pub fn from_env() -> Result<Self, LLMError> {
        let config = GeminiConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }

    fn convert_messages(&self, messages: Vec<Message>) -> (Option<String>, Vec<serde_json::Value>) {
        let mut system_prompt = None;
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_prompt = Some(msg.text_content());
                }
                Role::User => {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{"text": msg.text_content()}]
                    }));
                }
                Role::Assistant => {
                    contents.push(json!({
                        "role": "model",
                        "parts": [{"text": msg.text_content()}]
                    }));
                }
                _ => {}
            }
        }

        (system_prompt, contents)
    }

    async fn execute_with_retry(&self, request: LLMRequest) -> LLMResult<reqwest::Response> {
        let (system_prompt, contents) = self.convert_messages(request.messages);

        let model = if request.config.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.config.model
        };

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.config.base_url, model, self.config.api_key
        );

        let mut body = json!({
            "contents": contents,
        });

        if let Some(sys) = system_prompt {
            body["systemInstruction"] = json!({"parts": [{"text": sys}]});
        }

        if let Some(temp) = request.config.temperature {
            body["generationConfig"] = json!({"temperature": temp});
        }

        if let Some(max_tokens) = request.config.max_tokens {
            if body.get("generationConfig").is_none() {
                body["generationConfig"] = json!({});
            }
            if let Some(config) = body.get_mut("generationConfig") {
                config["maxOutputTokens"] = json!(max_tokens);
            }
        }

        let headers = {
            let mut h = HeaderMap::new();
            h.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            h
        };

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
impl LLMProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        let response = self.execute_with_retry(request).await?;
        let gemini_resp: GeminiResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        // Convert Gemini response to standard format
        let content = gemini_resp
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .unwrap_or_default();

        Ok(LLMResponse {
            id: "gemini-".to_string() + &uuid::Uuid::new_v4().to_string(),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: self.config.default_model.clone(),
            choices: vec![Choice {
                index: 0,
                message: Message::assistant(content),
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: gemini_resp.usageMetadata.map(|u| Usage {
                prompt_tokens: u.promptTokenCount,
                completion_tokens: u.candidatesTokenCount,
                total_tokens: u.totalTokenCount,
            }),
        })
    }

    async fn complete_stream(
        &self,
        _request: LLMRequest,
    ) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        // Gemini streaming implementation would go here
        Err(LLMError::NotImplemented(
            "Streaming not yet implemented for Gemini".to_string(),
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
                id: gemini_models::GEMINI_2_0_FLASH.to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                description: Some("Fast multimodal model".to_string()),
                context_window: 1_000_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0, 0.0)), // Currently free tier
            },
            ModelInfo {
                id: gemini_models::GEMINI_1_5_PRO.to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                description: Some("High performance, 1M context".to_string()),
                context_window: 2_000_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0035, 0.0105)),
            },
            ModelInfo {
                id: gemini_models::GEMINI_1_5_FLASH.to_string(),
                name: "Gemini 1.5 Flash".to_string(),
                description: Some("Fast and cost-effective".to_string()),
                context_window: 1_000_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.00035, 0.00105)),
            },
        ])
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
    usageMetadata: Option<UsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Debug, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
struct Part {
    text: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct UsageMetadata {
    promptTokenCount: u32,
    candidatesTokenCount: u32,
    totalTokenCount: u32,
}

pub mod gemini_models {
    pub const GEMINI_2_0_FLASH: &str = "gemini-2.0-flash";
    pub const GEMINI_2_0_PRO: &str = "gemini-2.0-pro-exp";
    pub const GEMINI_1_5_PRO: &str = "gemini-1.5-pro";
    pub const GEMINI_1_5_FLASH: &str = "gemini-1.5-flash";
    pub const GEMINI_1_5_PRO_LATEST: &str = "gemini-1.5-pro-latest";
    pub const GEMINI_1_5_FLASH_LATEST: &str = "gemini-1.5-flash-latest";
}
