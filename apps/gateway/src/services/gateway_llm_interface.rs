//! Gateway LLM bridge for the beebotos_agents runtime.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;

/// Gateway wrapper that implements the agents crate's LLMCallInterface
/// using the gateway's own LlmService.
pub struct GatewayLLMInterface {
    llm_service: Arc<crate::services::llm_service::LlmService>,
    react_trace_sink: Option<Arc<dyn beebotos_agents::ReActTraceSink>>,
}

impl GatewayLLMInterface {
    pub fn new(llm_service: Arc<crate::services::llm_service::LlmService>) -> Self {
        Self {
            llm_service,
            react_trace_sink: None,
        }
    }

    pub fn with_react_trace_sink(mut self, sink: Arc<dyn beebotos_agents::ReActTraceSink>) -> Self {
        self.react_trace_sink = Some(sink);
        self
    }

    fn convert_messages(
        messages: &[beebotos_agents::communication::Message],
    ) -> Vec<beebotos_agents::llm::Message> {
        use beebotos_agents::llm::{Content, Message as LLMMessage, Role};

        let mut system_parts = Vec::new();
        let mut llm_messages = Vec::new();

        for (idx, msg) in messages.iter().enumerate() {
            let content = msg.content.trim();
            let role = msg.metadata.get("role").map(|s| s.as_str());

            match role {
                Some("system") => system_parts.push(content.to_string()),
                Some("assistant") => {
                    let mut out = LLMMessage::assistant(content.to_string());
                    if let Some(tool_calls_json) = msg.metadata.get("tool_calls_json") {
                        if let Ok(tool_calls) = serde_json::from_str(tool_calls_json) {
                            out = out.with_tool_calls(tool_calls);
                        }
                    }
                    if let Some(reasoning_content) = msg.metadata.get("reasoning_content") {
                        out.reasoning_content = Some(reasoning_content.clone());
                    }
                    llm_messages.push(out);
                }
                Some("tool") => {
                    llm_messages.push(LLMMessage {
                        role: Role::Tool,
                        content: vec![Content::Text {
                            text: msg.content.clone(),
                        }],
                        name: None,
                        tool_calls: None,
                        tool_call_id: msg.metadata.get("tool_call_id").cloned(),
                        reasoning_content: None,
                    });
                }
                Some("user") => llm_messages.push(LLMMessage::user(content.to_string())),
                _ if idx == 0 => system_parts.push(content.to_string()),
                _ if content.starts_with("[系统提示")
                    || content.starts_with("以下是与当前对话相关的历史记忆") =>
                {
                    system_parts.push(content.to_string())
                }
                _ if content.starts_with("用户:") => {
                    if let Some(rest) = content.strip_prefix("用户:") {
                        llm_messages.push(LLMMessage::user(rest.trim().to_string()));
                    }
                }
                _ if content.starts_with("助手:") => {
                    if let Some(rest) = content.strip_prefix("助手:") {
                        let trimmed = rest.trim();
                        if !trimmed.is_empty() {
                            llm_messages.push(LLMMessage::assistant(trimmed.to_string()));
                        }
                    }
                }
                _ if content.starts_with("系统:") => {
                    if let Some(rest) = content.strip_prefix("系统:") {
                        system_parts.push(rest.trim().to_string());
                    }
                }
                _ => llm_messages.push(LLMMessage::user(content.to_string())),
            }
        }

        if !system_parts.is_empty() {
            let system_text = system_parts.join("\n\n");
            let mut final_messages = vec![LLMMessage::system(system_text)];
            final_messages.extend(llm_messages);
            final_messages
        } else {
            llm_messages
        }
    }

    fn max_tokens_from_context(context: &Option<HashMap<String, String>>) -> Option<u32> {
        context
            .as_ref()
            .and_then(|c| c.get("max_tokens"))
            .and_then(|t| t.parse::<u32>().ok())
    }

    fn model_from_context(context: &Option<HashMap<String, String>>) -> Option<String> {
        context.as_ref().and_then(|c| c.get("model")).cloned()
    }

    fn tool_choice_from_context(context: &Option<HashMap<String, String>>) -> Option<String> {
        context.as_ref().and_then(|c| c.get("tool_choice")).cloned()
    }

    fn tools_from_context(
        context: &Option<HashMap<String, String>>,
    ) -> Option<Vec<beebotos_agents::llm::Tool>> {
        context
            .as_ref()
            .and_then(|c| c.get("tools_json"))
            .and_then(|json_str| {
                serde_json::from_str::<Vec<beebotos_agents::communication::ToolDefinition>>(
                    json_str,
                )
                .ok()
            })
            .map(Self::convert_tools)
    }

    async fn emit_tool_call_messages(
        &self,
        context: &Option<HashMap<String, String>>,
        tool_calls: &[beebotos_agents::llm::ToolCall],
    ) {
        let Some(sink) = &self.react_trace_sink else {
            return;
        };
        let Some(context) = context.as_ref() else {
            return;
        };
        let Some(session_id) = context
            .get("session_id")
            .or_else(|| context.get("channel_id"))
            .cloned()
        else {
            return;
        };
        let agent_id = context
            .get("agent_id")
            .cloned()
            .unwrap_or_else(|| "gateway".to_string());
        let run_id = context
            .get("react_run_id")
            .cloned()
            .unwrap_or_else(|| format!("gateway-{}", uuid::Uuid::new_v4()));
        let round = context
            .get("react_round")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);
        let max_rounds = context
            .get("max_tool_rounds")
            .and_then(|v| v.parse::<u32>().ok())
            .map(beebotos_agents::clamp_react_max_tool_rounds)
            .unwrap_or(beebotos_agents::DEFAULT_REACT_MAX_TOOL_ROUNDS);

        for tool_call in tool_calls {
            let event = beebotos_agents::ReActTraceEvent::new(
                run_id.clone(),
                session_id.clone(),
                agent_id.clone(),
                round,
                max_rounds,
                beebotos_agents::ReActTracePhase::AssistantToolCall,
            )
            .with_tool(tool_call.function.name.clone(), tool_call.id.clone())
            .with_arguments_preview(&tool_call.function.arguments);
            if let Err(e) = sink.emit(event).await {
                warn!("Failed to emit LLM tool_call message: {}", e);
            }
        }
    }

    fn convert_tools(
        defs: Vec<beebotos_agents::communication::ToolDefinition>,
    ) -> Vec<beebotos_agents::llm::Tool> {
        defs.into_iter()
            .map(|d| beebotos_agents::llm::Tool {
                r#type: "function".to_string(),
                function: beebotos_agents::llm::FunctionDefinition {
                    name: d.name,
                    description: Some(d.description),
                    parameters: d.parameters,
                },
            })
            .collect()
    }
}

#[async_trait]
impl beebotos_agents::communication::LLMCallInterface for GatewayLLMInterface {
    fn supports_native_tools(&self) -> bool {
        true
    }

    async fn call_llm(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        _context: Option<std::collections::HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<String> {
        let final_messages = Self::convert_messages(&messages);

        let max_tokens_override = Self::max_tokens_from_context(&_context);
        let model_override = Self::model_from_context(&_context);
        let tools = Self::tools_from_context(&_context);
        let tool_choice = Self::tool_choice_from_context(&_context);

        self.llm_service
            .chat(
                final_messages,
                max_tokens_override,
                tools,
                tool_choice,
                model_override,
            )
            .await
            .map_err(|e| {
                beebotos_agents::error::AgentError::Execution(format!("LLM call failed: {}", e))
            })
    }

    async fn call_llm_tool_turn(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        tools: Vec<beebotos_agents::communication::ToolDefinition>,
        context: Option<HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<beebotos_agents::communication::ToolAwareResponse> {
        let final_messages = Self::convert_messages(&messages);
        let llm_tools = Self::convert_tools(tools);
        let max_tokens_override = Self::max_tokens_from_context(&context);
        let model_override = Self::model_from_context(&context);
        let tool_choice = Self::tool_choice_from_context(&context);

        let turn = self
            .llm_service
            .chat_turn(
                final_messages,
                max_tokens_override,
                Some(llm_tools),
                tool_choice,
                model_override,
            )
            .await
            .map_err(|e| {
                beebotos_agents::error::AgentError::Execution(format!("LLM call failed: {}", e))
            })?;

        if !turn.tool_calls.is_empty() {
            self.emit_tool_call_messages(&context, &turn.tool_calls)
                .await;
        }

        Ok(beebotos_agents::communication::ToolAwareResponse {
            content: turn.content,
            tool_calls: turn.tool_calls,
            reasoning_content: turn.reasoning_content,
        })
    }

    async fn call_llm_stream(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        _context: Option<std::collections::HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<tokio::sync::mpsc::Receiver<String>> {
        let final_messages = Self::convert_messages(&messages);

        let max_tokens_override = Self::max_tokens_from_context(&_context);
        let model_override = Self::model_from_context(&_context);

        self.llm_service
            .chat_stream(final_messages, max_tokens_override, model_override)
            .await
            .map_err(|e| {
                beebotos_agents::error::AgentError::Execution(format!("LLM stream failed: {}", e))
            })
    }
}
