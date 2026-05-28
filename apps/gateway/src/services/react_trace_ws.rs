use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use beebotos_agents::{ReActTraceEvent, ReActTracePhase};
use gateway::websocket::WebSocketManager;
use serde_json::{json, Value};

pub struct WebSocketReActTraceSink {
    ws_manager: Arc<WebSocketManager>,
    channel: String,
    tool_call_store: Option<Arc<ToolCallTraceStore>>,
}

impl WebSocketReActTraceSink {
    pub fn new(ws_manager: Arc<WebSocketManager>) -> Self {
        Self {
            ws_manager,
            channel: "webchat".to_string(),
            tool_call_store: None,
        }
    }

    pub fn with_tool_call_store(mut self, store: Arc<ToolCallTraceStore>) -> Self {
        self.tool_call_store = Some(store);
        self
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

#[derive(Default)]
pub struct ToolCallTraceStore {
    calls: Mutex<HashMap<String, Vec<Value>>>,
    active_sessions: Mutex<HashMap<String, usize>>,
}

impl ToolCallTraceStore {
    pub fn start_session(&self, session_id: &str) {
        if let Ok(mut sessions) = self.active_sessions.lock() {
            *sessions.entry(session_id.to_string()).or_default() += 1;
        }
    }

    pub fn end_session(&self, session_id: &str) {
        if let Ok(mut sessions) = self.active_sessions.lock() {
            if let Some(count) = sessions.get_mut(session_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    sessions.remove(session_id);
                }
            }
        }
    }

    pub fn collect(&self, event: &ReActTraceEvent) {
        let Some(tool_call) = WebSocketReActTraceSink::compatibility_tool_call(event) else {
            return;
        };
        let is_active = self
            .active_sessions
            .lock()
            .ok()
            .is_some_and(|sessions| sessions.contains_key(&event.session_id));
        if !is_active {
            return;
        }

        if let Ok(mut calls) = self.calls.lock() {
            calls
                .entry(event.session_id.clone())
                .or_default()
                .push(tool_call);
        }
    }

    pub fn drain(&self, session_id: &str) -> Vec<Value> {
        self.calls
            .lock()
            .ok()
            .and_then(|mut calls| calls.remove(session_id))
            .unwrap_or_default()
    }

    pub fn finish_session(&self, session_id: &str) -> Vec<Value> {
        let calls = self.drain(session_id);
        self.end_session(session_id);
        calls
    }
}

#[async_trait]
impl beebotos_agents::ReActTraceSink for WebSocketReActTraceSink {
    async fn emit(&self, event: ReActTraceEvent) -> Result<(), beebotos_agents::AgentError> {
        if let Some(store) = &self.tool_call_store {
            store.collect(&event);
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_started_trace_is_collected_for_session() {
        let store = ToolCallTraceStore::default();
        let event = ReActTraceEvent::new(
            "run-1",
            "session-1",
            "agent-1",
            1,
            40,
            ReActTracePhase::ToolStarted,
        )
        .with_tool("list_directory", "call-1")
        .with_arguments_preview(r#"{"path":"."}"#)
        .with_reasoning("check files");

        store.start_session("session-1");
        store.collect(&event);
        let calls = store.finish_session("session-1");

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["tool_name"], "list_directory");
        assert_eq!(calls[0]["arguments"]["path"], ".");
        assert!(store.drain("session-1").is_empty());
    }

    #[test]
    fn ignores_tool_started_trace_for_inactive_session() {
        let store = ToolCallTraceStore::default();
        let event = ReActTraceEvent::new(
            "run-1",
            "session-1",
            "agent-1",
            1,
            40,
            ReActTracePhase::ToolStarted,
        )
        .with_tool("list_directory", "call-1");

        store.collect(&event);

        assert!(store.drain("session-1").is_empty());
    }
}
