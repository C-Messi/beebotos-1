//! LLM Module - Comprehensive LLM integration
//!
//! Provides a complete interface for interacting with Large Language Models,
//! with support for:
//! - Multiple providers (Kimi, OpenAI, etc.)
//! - Streaming responses
//! - Tool/function calling
//! - Multimodal inputs (text + images)
//! - Conversation management
//! - Retry and error handling
//!
//! # Quick Start - Simple Chat
//!
//! ```ignore
//! use beebotos_agents::llm::{create_kimi_client, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create Kimi client from environment
//!     let client = create_kimi_client().await?;
//!     
//!     // Simple chat
//!     let response = client.chat("Hello, how are you?").await?;
//!     println!("{}", response);
//!     
//!     Ok(())
//! }
//! ```
//!
//! # Multimodal (Vision)
//!
//! ```rust
//! use beebotos_agents::llm::{create_kimi_client, Content, ImageUrlContent};
//!
//! async fn analyze_image(image_url: &str) -> Result<String, Box<dyn std::error::Error>> {
//!     let client = create_kimi_client().await?;
//!
//!     // Create multimodal message
//!     let contents = vec![
//!         Content::Text {
//!             text: "What's in this image?".to_string(),
//!         },
//!         Content::ImageUrl {
//!             image_url: ImageUrlContent {
//!                 url: image_url.to_string(),
//!                 detail: Some("high".to_string()),
//!             },
//!         },
//!     ];
//!
//!     let response = client.chat_multimodal(contents).await?;
//!     Ok(response)
//! }
//! ```
//!
//! # Streaming Response
//!
//! ```rust
//! use beebotos_agents::llm::create_kimi_client;
//!
//! async fn stream_chat() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = create_kimi_client().await?;
//!
//!     let mut stream = client.chat_stream("Tell me a story").await?;
//!
//!     while let Some(chunk) = stream.recv().await {
//!         print!("{}", chunk);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Tool Calling
//!
//! ```rust
//! use beebotos_agents::llm::{create_kimi_client, LLMClient, ToolHandler, Tool, FunctionDefinition};
//! use async_trait::async_trait;
//!
//! struct CalculatorTool;
//!
//! #[async_trait]
//! impl ToolHandler for CalculatorTool {
//!     fn definition(&self) -> Tool {
//!         Tool {
//!             r#type: "function".to_string(),
//!             function: FunctionDefinition {
//!                 name: "calculate".to_string(),
//!                 description: Some("Perform calculation".to_string()),
//!                 parameters: serde_json::json!({
//!                     "type": "object",
//!                     "properties": {
//!                         "expression": { "type": "string" }
//!                     },
//!                     "required": ["expression"]
//!                 }),
//!             },
//!         }
//!     }
//!     
//!     async fn execute(&self, arguments: &str) -> Result<String, String> {
//!         // Parse and execute...
//!         Ok("42".to_string())
//!     }
//! }
//! ```

pub mod adapter;
pub mod client;
pub mod failover;
pub mod http_client;
pub mod providers;
pub mod skill_tool_handler;
pub mod traits;
pub mod types;

// Re-export HTTP client types
// Re-export main types
pub use adapter::{LLMClientAdapter, LegacyLLMClientBuilder};
pub use client::{ClientMetrics, LLMClient, LLMClientBuilder, ToolHandler};
// ARCHITECTURE FIX: Re-export failover types
pub use failover::{FailoverConfig, FailoverProvider, FailoverProviderBuilder};
pub use http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig, ProviderInitParams};
// 🔧 P1 FIX: Re-export ProviderMode for multi-provider configuration
pub use providers::ProviderMode;
// Re-export model name modules
pub use providers::{
    anthropic_models, chatglm_models, claude_models, deepseek_models, doubao_models, gemini_models,
    kimi_models, ollama_models, openai_models, qwen_models, zhipu_models,
};
// Re-export all providers
pub use providers::{
    AnthropicConfig, AnthropicProvider, ChatGLMConfig, ChatGLMProvider, ClaudeConfig,
    ClaudeProvider, DeepSeekConfig, DeepSeekProvider, DoubaoConfig, DoubaoProvider, GeminiConfig,
    GeminiProvider, KimiConfig, KimiProvider, OllamaConfig, OllamaProvider, OpenAIConfig,
    OpenAIProvider, ProviderFactory, QwenConfig, QwenProvider, ZhipuConfig, ZhipuProvider,
};
pub use skill_tool_handler::SkillToolHandler;
pub use traits::{
    ContextManager, LLMProvider, MetricsCollector, ModelCapabilities, ModelInfo,
    ProviderCapabilities, RetryPolicy, ToolExecutor,
};
pub use types::{
    Choice, Content, Delta, FunctionCall, FunctionChoice, FunctionDefinition, ImageUrlContent,
    LLMError, LLMRequest, LLMResponse, LLMResult, Message, RequestConfig, ResponseFormat, Role,
    StreamChoice, StreamChunk, Tool, ToolCall, ToolChoice, ToolResult, Usage,
};

/// Create a Kimi client from environment variables
///
/// Expects KIMI_API_KEY or MOONSHOT_API_KEY to be set
pub async fn create_kimi_client() -> LLMResult<LLMClient> {
    let provider = KimiProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a Kimi client with custom config
pub async fn create_kimi_client_with_config(config: KimiConfig) -> LLMResult<LLMClient> {
    let provider = KimiProvider::new(config)?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create an OpenAI client from environment variables
///
/// Expects OPENAI_API_KEY to be set
pub async fn create_openai_client() -> LLMResult<LLMClient> {
    let provider = OpenAIProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a DeepSeek client from environment variables
///
/// Expects DEEPSEEK_API_KEY to be set
pub async fn create_deepseek_client() -> LLMResult<LLMClient> {
    let provider = DeepSeekProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a ChatGLM client from environment variables
///
/// Expects CHATGLM_API_KEY or ZHIPU_API_KEY to be set
pub async fn create_chatglm_client() -> LLMResult<LLMClient> {
    let provider = ChatGLMProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a Doubao client from environment variables
///
/// Expects DOUBAO_API_KEY to be set
pub async fn create_doubao_client() -> LLMResult<LLMClient> {
    let provider = DoubaoProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a Qwen client from environment variables
///
/// Expects QWEN_API_KEY or DASHSCOPE_API_KEY to be set
pub async fn create_qwen_client() -> LLMResult<LLMClient> {
    let provider = QwenProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a Gemini client from environment variables
///
/// Expects GEMINI_API_KEY or GOOGLE_API_KEY to be set
pub async fn create_gemini_client() -> LLMResult<LLMClient> {
    let provider = GeminiProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a Claude client from environment variables
///
/// Expects ANTHROPIC_API_KEY to be set
pub async fn create_claude_client() -> LLMResult<LLMClient> {
    let provider = ClaudeProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create an Anthropic client from environment variables (alias for
/// create_claude_client)
///
/// Expects ANTHROPIC_API_KEY to be set
pub async fn create_anthropic_client() -> LLMResult<LLMClient> {
    let provider = AnthropicProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create an Ollama client from environment variables
///
/// Expects OLLAMA_BASE_URL (optional, defaults to http://localhost:11434)
pub async fn create_ollama_client() -> LLMResult<LLMClient> {
    let provider = OllamaProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create a Zhipu client from environment variables
///
/// Expects ZHIPU_API_KEY to be set
pub async fn create_zhipu_client() -> LLMResult<LLMClient> {
    let provider = ZhipuProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Version of the LLM module
pub const VERSION: &str = "1.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text_content(), "Hello");
    }

    #[test]
    fn test_message_with_image() {
        let msg =
            Message::user("What's in this image?").with_image("https://example.com/image.png");

        assert_eq!(msg.content.len(), 2);
    }

    #[test]
    fn test_retry_policy() {
        let policy = RetryPolicy::default();

        // Should retry network errors
        let error = LLMError::Network("timeout".to_string());
        assert!(policy.should_retry(&error, 0));

        // Should not retry after max attempts
        assert!(!policy.should_retry(&error, 3));
    }
}
