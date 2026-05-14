use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AgentError;

pub const DEFAULT_REACT_MAX_TOOL_ROUNDS: u32 = 40;
pub const MAX_REACT_TOOL_ROUNDS_LIMIT: u32 = 50;
pub const REACT_TRACE_PREVIEW_CHARS: usize = 1000;

pub fn clamp_react_max_tool_rounds(rounds: u32) -> u32 {
    rounds.clamp(1, MAX_REACT_TOOL_ROUNDS_LIMIT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReActTracePhase {
    LoopStarted,
    AssistantToolCall,
    ToolStarted,
    ToolResult,
    ToolError,
    FinalAnswer,
    LoopLimitReached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActTraceEvent {
    pub run_id: String,
    pub session_id: String,
    pub agent_id: String,
    pub round: u32,
    pub max_rounds: u32,
    pub phase: ReActTracePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    pub is_error: bool,
    pub timestamp: DateTime<Utc>,
}

impl ReActTraceEvent {
    pub fn new(
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        agent_id: impl Into<String>,
        round: u32,
        max_rounds: u32,
        phase: ReActTracePhase,
    ) -> Self {
        let is_error = matches!(phase, ReActTracePhase::ToolError);
        Self {
            run_id: run_id.into(),
            session_id: session_id.into(),
            agent_id: agent_id.into(),
            round,
            max_rounds,
            phase,
            tool_name: None,
            tool_call_id: None,
            arguments_preview: None,
            content_preview: None,
            is_error,
            timestamp: Utc::now(),
        }
    }

    pub fn with_tool(
        mut self,
        tool_name: impl Into<String>,
        tool_call_id: impl Into<String>,
    ) -> Self {
        self.tool_name = Some(tool_name.into());
        self.tool_call_id = Some(tool_call_id.into());
        self
    }

    pub fn with_arguments_preview(mut self, arguments: impl AsRef<str>) -> Self {
        self.arguments_preview = Some(sanitize_preview(arguments.as_ref()));
        self
    }

    pub fn with_content_preview(mut self, content: impl AsRef<str>) -> Self {
        self.content_preview = Some(sanitize_preview(content.as_ref()));
        self
    }

    pub fn error(mut self) -> Self {
        self.is_error = true;
        self
    }
}

#[async_trait]
pub trait ReActTraceSink: Send + Sync {
    async fn emit(&self, event: ReActTraceEvent) -> Result<(), AgentError>;
}

pub fn sanitize_preview(input: &str) -> String {
    let mut value = if let Ok(json) = serde_json::from_str::<serde_json::Value>(input) {
        redact_json_value(json).to_string()
    } else {
        redact_sensitive_text(input)
    };

    if value.chars().count() > REACT_TRACE_PREVIEW_CHARS {
        value = value
            .chars()
            .take(REACT_TRACE_PREVIEW_CHARS)
            .collect::<String>();
        value.push_str("...[truncated]");
    }

    value
}

fn redact_json_value(mut value: serde_json::Value) -> serde_json::Value {
    match &mut value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *value = serde_json::Value::String("[redacted]".to_string());
                } else {
                    *value = redact_json_value(value.take());
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                *item = redact_json_value(item.take());
            }
        }
        _ => {}
    }
    value
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("credential")
}

fn redact_sensitive_text(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let key = lower
                .split_once('=')
                .or_else(|| lower.split_once(':'))
                .map(|(key, _)| key)
                .unwrap_or(&lower);
            if is_sensitive_key(key) {
                "[redacted]".to_string()
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn format_tool_error(tool_name: &str, reason: impl std::fmt::Display) -> String {
    format!(
        "Error: Tool '{}' failed: {}. The model can recover by adjusting arguments, choosing a \
         different tool, or explaining the limitation.",
        tool_name, reason
    )
}

pub fn format_unknown_tool_error(tool_name: &str, available_tools: &[String]) -> String {
    format!(
        "Error: Tool '{}' not found. Available tools: {:?}. The model should choose one of the \
         available tools or answer without a tool if possible.",
        tool_name, available_tools
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn react_round_defaults_and_clamp_are_stable() {
        assert_eq!(DEFAULT_REACT_MAX_TOOL_ROUNDS, 40);
        assert_eq!(clamp_react_max_tool_rounds(0), 1);
        assert_eq!(clamp_react_max_tool_rounds(37), 37);
        assert_eq!(
            clamp_react_max_tool_rounds(999),
            MAX_REACT_TOOL_ROUNDS_LIMIT
        );
    }

    #[test]
    fn sanitize_preview_redacts_sensitive_values() {
        let json = sanitize_preview(r#"{"api_key":"abc","nested":{"token":"secret"},"q":"ok"}"#);
        assert!(json.contains("[redacted]"));
        assert!(!json.contains("abc"));
        assert!(!json.contains("secret"));

        let text = sanitize_preview("password=hunter2 token:abc query ok");
        assert!(text.contains("[redacted]"));
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("abc"));
    }

    #[test]
    fn tool_errors_are_llm_visible_and_recoverable() {
        let err = format_tool_error("search", "network down");
        assert!(err.starts_with("Error:"));
        assert!(err.contains("recover"));

        let unknown = format_unknown_tool_error("missing", &["read_file".to_string()]);
        assert!(unknown.contains("read_file"));
    }
}
