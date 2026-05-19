use std::sync::Arc;

use async_trait::async_trait;
use beebotos_agents::{ReActTraceEvent, ReActTracePhase};
use gateway::websocket::WebSocketManager;
use serde_json::{json, Value};

pub struct WebSocketReActTraceSink {
    ws_manager: Arc<WebSocketManager>,
    channel: String,
}

impl WebSocketReActTraceSink {
    pub fn new(ws_manager: Arc<WebSocketManager>) -> Self {
        Self {
            ws_manager,
            channel: "webchat".to_string(),
        }
    }

    fn compatibility_tool_call(event: &ReActTraceEvent) -> Option<Value> {
        if event.phase != ReActTracePhase::ToolStarted {
            return None;
        }

        let arguments = event
            .arguments_preview
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .unwrap_or_else(|| {
                event
                    .arguments_preview
                    .as_ref()
                    .map(|value| Value::String(value.clone()))
                    .unwrap_or(Value::Null)
            });

        Some(json!({
            "round": event.round,
            "tool_name": event.tool_name.as_deref().unwrap_or("unknown"),
            "reasoning": event.reasoning.as_deref().unwrap_or(""),
            "arguments": arguments,
            "status": "started",
        }))
    }
}

#[async_trait]
impl beebotos_agents::ReActTraceSink for WebSocketReActTraceSink {
    async fn emit(&self, event: ReActTraceEvent) -> Result<(), beebotos_agents::AgentError> {
        let trace_payload = json!({
            "type": "react_trace",
            "session_id": event.session_id,
            "event": event.clone(),
        });

        self.ws_manager
            .broadcast_to_channel(&self.channel, trace_payload)
            .await
            .map_err(|e| {
                beebotos_agents::AgentError::Execution(format!(
                    "Failed to broadcast ReAct trace: {}",
                    e
                ))
            })?;

        if let Some(tool_event) = Self::compatibility_tool_call(&event) {
            let tool_payload = json!({
                "type": "chat_tool_call",
                "session_id": event.session_id,
                "event": tool_event,
            });

            self.ws_manager
                .broadcast_to_channel(&self.channel, tool_payload)
                .await
                .map_err(|e| {
                    beebotos_agents::AgentError::Execution(format!(
                        "Failed to broadcast WebChat tool call: {}",
                        e
                    ))
                })?;
        }

        Ok(())
    }
}
