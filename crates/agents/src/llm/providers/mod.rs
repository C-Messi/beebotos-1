//! LLM Providers
//!
//! Implementations of the LLMProvider trait for various LLM services.

pub mod anthropic;
pub mod claude;
pub mod deepseek;
pub mod doubao;
pub mod gemini;
pub mod kimi;
pub mod ollama;
pub mod openai;
pub mod qwen;
pub mod zhipu;

// Re-export providers
pub use anthropic::{AnthropicConfig, AnthropicProvider, anthropic_models};
pub use claude::{ClaudeConfig, ClaudeProvider, claude_models};

// ChatGLM is now an alias for Zhipu (same provider)
pub use zhipu::{ZhipuConfig as ChatGLMConfig, ZhipuProvider as ChatGLMProvider, zhipu_models as chatglm_models};
pub use deepseek::{DeepSeekConfig, DeepSeekProvider, deepseek_models};
pub use doubao::{DoubaoConfig, DoubaoProvider, doubao_models};
pub use gemini::{GeminiConfig, GeminiProvider, gemini_models};
pub use kimi::{KimiConfig, KimiProvider, ProviderMode, ThinkingMode, kimi_models};
pub use ollama::{OllamaConfig, OllamaProvider, ollama_models};
pub use openai::{OpenAIConfig, OpenAIProvider, openai_models};
pub use qwen::{QwenConfig, QwenProvider, qwen_models};
pub use zhipu::{ZhipuConfig, ZhipuProvider, zhipu_models};

/// Provider factory - creates providers by name
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider by name from environment
    pub fn from_env(name: &str) -> Result<Box<dyn super::traits::LLMProvider>, String> {
        match name.to_lowercase().as_str() {
            "kimi" | "moonshot" => {
                let provider = KimiProvider::from_env()
                    .map_err(|e| format!("Failed to create Kimi provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "openai" | "chatgpt" => {
                let provider = OpenAIProvider::from_env()
                    .map_err(|e| format!("Failed to create OpenAI provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "deepseek" => {
                let provider = DeepSeekProvider::from_env()
                    .map_err(|e| format!("Failed to create DeepSeek provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "chatglm" | "zhipu" | "glm" => {
                let provider = ZhipuProvider::from_env()
                    .map_err(|e| format!("Failed to create Zhipu provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "doubao" | "bytedance" | "ark" => {
                let provider = DoubaoProvider::from_env()
                    .map_err(|e| format!("Failed to create Doubao provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "qwen" | "alibaba" | "dashscope" => {
                let provider = QwenProvider::from_env()
                    .map_err(|e| format!("Failed to create Qwen provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "gemini" | "google" => {
                let provider = GeminiProvider::from_env()
                    .map_err(|e| format!("Failed to create Gemini provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "claude" | "anthropic" => {
                let provider = AnthropicProvider::from_env()
                    .map_err(|e| format!("Failed to create Anthropic provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "ollama" | "local" => {
                let provider = OllamaProvider::from_env()
                    .map_err(|e| format!("Failed to create Ollama provider: {}", e))?;
                Ok(Box::new(provider))
            }
            _ => Err(format!("Unknown provider: {}", name)),
        }
    }

    /// List all available provider names
    pub fn available_providers() -> Vec<&'static str> {
        vec![
            "kimi",
            "openai",
            "deepseek",
            "zhipu",
            "doubao",
            "qwen",
            "gemini",
            "claude",
            "ollama",
        ]
    }
}
