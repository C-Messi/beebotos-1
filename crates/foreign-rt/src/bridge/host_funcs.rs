//! Host functions exposed to foreign runtimes
//!
//! These functions allow Python/Node.js scripts to safely interact with
//! BeeBotOS kernel capabilities without direct system access.

use std::sync::Arc;

use serde_json::json;
use tracing::{debug, trace, warn};

use crate::bridge::{BridgeCall, BridgeResponse};
use crate::error::{ForeignRtError, Result};

/// Host function context passed to WASM instances
#[derive(Debug, Clone)]
pub struct HostContext {
    /// Agent ID that owns this execution
    pub agent_id: Option<String>,
    /// Task ID
    pub task_id: String,
    /// Allowed permissions
    pub permissions: Vec<String>,
    /// Execution start time
    pub started_at: std::time::Instant,
}

impl HostContext {
    /// Create a new host context
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            agent_id: None,
            task_id: task_id.into(),
            permissions: Vec::new(),
            started_at: std::time::Instant::now(),
        }
    }

    /// With agent ID
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// With permissions
    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    /// Check if a permission is granted
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.contains(&"*".to_string()) || self.permissions.contains(&perm.to_string())
    }
}

/// Host function dispatcher
pub struct HostFunctionDispatcher {
    context: HostContext,
}

impl HostFunctionDispatcher {
    /// Create a new dispatcher
    pub fn new(context: HostContext) -> Self {
        Self { context }
    }

    /// Dispatch a bridge call
    pub async fn dispatch(&self, call: BridgeCall) -> BridgeResponse {
        trace!(
            task_id = %self.context.task_id,
            namespace = %call.namespace,
            method = %call.method,
            "Dispatching host function call"
        );

        match call.namespace.as_str() {
            "storage" => self.handle_storage(call).await,
            "ipc" => self.handle_ipc(call).await,
            "llm" => self.handle_llm(call).await,
            "chain" => self.handle_chain(call).await,
            "log" => self.handle_log(call),
            "env" => self.handle_env(call),
            "fs" => self.handle_fs(call),
            _ => {
                warn!("Unknown bridge namespace: {}", call.namespace);
                BridgeResponse::error(format!("Unknown namespace: {}", call.namespace))
            }
        }
    }

    /// Handle storage operations
    async fn handle_storage(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("storage") {
            return BridgeResponse::error("Missing permission: storage");
        }

        match call.method.as_str() {
            "get" => {
                if call.args.len() < 1 {
                    return BridgeResponse::error("storage.get requires key argument");
                }
                let key = call.args[0].as_str().unwrap_or("");
                debug!("Storage get: {}", key);
                // TODO: Integrate with kernel::storage
                BridgeResponse::success(json!({"value": null, "found": false}))
            }
            "put" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("storage.put requires key and value arguments");
                }
                let key = call.args[0].as_str().unwrap_or("");
                let value = &call.args[1];
                debug!("Storage put: {} = {:?}", key, value);
                // TODO: Integrate with kernel::storage
                BridgeResponse::success(json!({"stored": true}))
            }
            _ => BridgeResponse::error(format!("Unknown storage method: {}", call.method)),
        }
    }

    /// Handle IPC/A2A operations
    async fn handle_ipc(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("ipc") {
            return BridgeResponse::error("Missing permission: ipc");
        }

        match call.method.as_str() {
            "send_message" => {
                if call.args.len() < 3 {
                    return BridgeResponse::error("ipc.send_message requires target_agent, message_type, payload");
                }
                let target = call.args[0].as_str().unwrap_or("");
                let msg_type = call.args[1].as_str().unwrap_or("");
                debug!("IPC send to {}: type={}", target, msg_type);
                // TODO: Integrate with kernel::ipc or agents::a2a
                BridgeResponse::success(json!({"message_id": uuid::Uuid::new_v4().to_string()}))
            }
            _ => BridgeResponse::error(format!("Unknown ipc method: {}", call.method)),
        }
    }

    /// Handle LLM operations
    async fn handle_llm(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("llm") {
            return BridgeResponse::error("Missing permission: llm");
        }

        match call.method.as_str() {
            "chat" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("llm.chat requires model and prompt arguments");
                }
                let model = call.args[0].as_str().unwrap_or("default");
                let prompt = call.args[1].as_str().unwrap_or("");
                debug!("LLM chat: model={}, prompt_len={}", model, prompt.len());
                // TODO: Integrate with agents::llm
                BridgeResponse::success(json!({
                    "content": "[LLM integration pending]",
                    "model": model,
                    "tokens_used": 0
                }))
            }
            _ => BridgeResponse::error(format!("Unknown llm method: {}", call.method)),
        }
    }

    /// Handle blockchain operations
    async fn handle_chain(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("chain") {
            return BridgeResponse::error("Missing permission: chain");
        }

        match call.method.as_str() {
            "call_contract" => {
                if call.args.len() < 3 {
                    return BridgeResponse::error("chain.call_contract requires chain_id, contract, data");
                }
                let chain_id = call.args[0].as_str().unwrap_or("");
                let contract = call.args[1].as_str().unwrap_or("");
                debug!("Chain call: chain={}, contract={}", chain_id, contract);
                // TODO: Integrate with beebotos-chain
                BridgeResponse::success(json!({"tx_hash": null, "status": "pending"}))
            }
            _ => BridgeResponse::error(format!("Unknown chain method: {}", call.method)),
        }
    }

    /// Handle logging
    fn handle_log(&self, call: BridgeCall) -> BridgeResponse {
        match call.method.as_str() {
            "write" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("log.write requires level and message");
                }
                let level = call.args[0].as_str().unwrap_or("info");
                let message = call.args[1].as_str().unwrap_or("");

                match level {
                    "debug" => debug!(task_id = %self.context.task_id, "[script] {}", message),
                    "info" => tracing::info!(task_id = %self.context.task_id, "[script] {}", message),
                    "warn" => warn!(task_id = %self.context.task_id, "[script] {}", message),
                    "error" => tracing::error!(task_id = %self.context.task_id, "[script] {}", message),
                    _ => tracing::info!(task_id = %self.context.task_id, "[script] {}", message),
                }

                BridgeResponse::success(json!({"logged": true}))
            }
            _ => BridgeResponse::error(format!("Unknown log method: {}", call.method)),
        }
    }

    /// Handle environment access
    fn handle_env(&self, call: BridgeCall) -> BridgeResponse {
        match call.method.as_str() {
            "get" => {
                if call.args.len() < 1 {
                    return BridgeResponse::error("env.get requires key argument");
                }
                let key = call.args[0].as_str().unwrap_or("");

                // Only allow access to whitelisted env vars
                let allowed = ["BEE_RUNTIME", "BEE_TASK_ID", "BEE_AGENT_ID"];
                if !allowed.contains(&key) {
                    return BridgeResponse::error(format!("Access to env var '{}' not allowed", key));
                }

                let value = match key {
                    "BEE_RUNTIME" => "foreign-rt",
                    "BEE_TASK_ID" => &self.context.task_id,
                    "BEE_AGENT_ID" => self.context.agent_id.as_deref().unwrap_or(""),
                    _ => "",
                };

                BridgeResponse::success(json!(value))
            }
            _ => BridgeResponse::error(format!("Unknown env method: {}", call.method)),
        }
    }

    /// Handle filesystem operations (sandboxed)
    fn handle_fs(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("fs") {
            return BridgeResponse::error("Missing permission: fs");
        }

        match call.method.as_str() {
            "read_file" => {
                if call.args.len() < 1 {
                    return BridgeResponse::error("fs.read_file requires path argument");
                }
                let path = call.args[0].as_str().unwrap_or("");
                debug!("FS read: {}", path);
                // TODO: Implement via WASI preopens or restricted fs access
                BridgeResponse::error("FS operations not yet implemented")
            }
            "write_file" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("fs.write_file requires path and content");
                }
                let path = call.args[0].as_str().unwrap_or("");
                debug!("FS write: {}", path);
                BridgeResponse::error("FS operations not yet implemented")
            }
            _ => BridgeResponse::error(format!("Unknown fs method: {}", call.method)),
        }
    }
}

/// Register host functions with a wasmtime linker
#[cfg(feature = "wasmtime")]
pub fn register_host_functions<T>(
    linker: &mut wasmtime::Linker<T>,
    _ctx: HostContext,
) -> Result<()>
where
    T: Send,
{
    // This will be expanded when integrating with wasmtime::Linker
    // For now, host functions are called via a different mechanism
    // through the bridge protocol
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_host_context_permissions() {
        let ctx = HostContext::new("test-task")
            .with_permissions(vec!["storage".to_string(), "log".to_string()]);

        assert!(ctx.has_permission("storage"));
        assert!(ctx.has_permission("log"));
        assert!(!ctx.has_permission("ipc"));
    }

    #[test]
    fn test_dispatcher_log() {
        let dispatcher = HostFunctionDispatcher::new(HostContext::new("test"));

        let response = dispatcher
            .handle_log(BridgeCall {
                namespace: "log".to_string(),
                method: "write".to_string(),
                args: vec![json!("info"), json!("Hello from test")],
            });

        assert!(response.success);
    }

    #[test]
    fn test_dispatcher_env_whitelist() {
        let dispatcher = HostFunctionDispatcher::new(
            HostContext::new("test").with_agent_id("agent-1"),
        );

        let response = dispatcher
            .handle_env(BridgeCall {
                namespace: "env".to_string(),
                method: "get".to_string(),
                args: vec![json!("BEE_TASK_ID")],
            });

        assert!(response.success);
        assert_eq!(response.data, Some(json!("test")));

        // Forbidden env var
        let response2 = dispatcher
            .handle_env(BridgeCall {
                namespace: "env".to_string(),
                method: "get".to_string(),
                args: vec![json!("PATH")],
            });

        assert!(!response2.success);
    }
}
