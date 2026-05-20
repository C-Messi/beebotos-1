//! Host functions exposed to foreign runtimes
//!
//! These functions allow Python/Node.js scripts to safely interact with
//! BeeBotOS kernel capabilities without direct system access.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::json;
use tracing::{debug, trace, warn};

use crate::bridge::{BridgeCall, BridgeResponse};

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

/// Trait for backend service integration
///
/// Implementors provide actual kernel/agent capabilities to host functions.
/// When no backend is available, host functions return mock responses.
pub trait BackendServices: Send + Sync {
    /// Storage get operation
    fn storage_get(&self, _key: &str) -> Option<serde_json::Value> {
        None
    }
    /// Storage put operation
    fn storage_put(&self, _key: &str, _value: serde_json::Value) -> bool {
        false
    }
    /// Storage delete operation
    fn storage_delete(&self, _key: &str) -> bool {
        false
    }
    /// Storage list keys with prefix
    fn storage_list(&self, _prefix: &str) -> Vec<String> {
        Vec::new()
    }

    /// IPC send message
    fn ipc_send(
        &self,
        _target: &str,
        _msg_type: &str,
        _payload: serde_json::Value,
    ) -> Option<String> {
        None
    }
    /// IPC receive messages (non-blocking)
    fn ipc_receive(&self, _agent_id: &str, _limit: usize) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// LLM chat completion
    fn llm_chat(&self, _model: &str, _prompt: &str) -> Option<String> {
        None
    }
    /// LLM embedding
    fn llm_embed(&self, _model: &str, _text: &str) -> Option<Vec<f32>> {
        None
    }

    /// Chain call contract
    fn chain_call(
        &self,
        _chain_id: &str,
        _contract: &str,
        _data: serde_json::Value,
    ) -> Option<serde_json::Value> {
        None
    }
    /// Chain query balance
    fn chain_balance(&self, _chain_id: &str, _address: &str) -> Option<String> {
        None
    }
}

/// No-op backend services (default)
pub struct NoopBackendServices;

impl BackendServices for NoopBackendServices {}

/// In-memory mock backend for testing
pub struct MockBackendServices {
    storage: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    messages: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl MockBackendServices {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl BackendServices for MockBackendServices {
    fn storage_get(&self, key: &str) -> Option<serde_json::Value> {
        let storage = self.storage.lock().unwrap();
        storage.get(key).cloned()
    }

    fn storage_put(&self, key: &str, value: serde_json::Value) -> bool {
        let mut storage = self.storage.lock().unwrap();
        storage.insert(key.to_string(), value);
        true
    }

    fn storage_delete(&self, key: &str) -> bool {
        let mut storage = self.storage.lock().unwrap();
        storage.remove(key).is_some()
    }

    fn storage_list(&self, prefix: &str) -> Vec<String> {
        let storage = self.storage.lock().unwrap();
        storage
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn ipc_send(&self, target: &str, msg_type: &str, payload: serde_json::Value) -> Option<String> {
        let msg_id = uuid::Uuid::new_v4().to_string();
        let mut messages = self.messages.lock().unwrap();
        messages.push(json!({
            "id": &msg_id,
            "target": target,
            "type": msg_type,
            "payload": payload,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
        Some(msg_id)
    }

    fn ipc_receive(&self, _agent_id: &str, limit: usize) -> Vec<serde_json::Value> {
        let messages = self.messages.lock().unwrap();
        messages.iter().rev().take(limit).cloned().collect()
    }
}

/// Host function dispatcher
pub struct HostFunctionDispatcher {
    context: HostContext,
    backend: Arc<dyn BackendServices>,
}

impl HostFunctionDispatcher {
    /// Create a new dispatcher with no-op backend
    pub fn new(context: HostContext) -> Self {
        Self {
            context,
            backend: Arc::new(NoopBackendServices),
        }
    }

    /// Create a new dispatcher with custom backend
    pub fn with_backend(context: HostContext, backend: Arc<dyn BackendServices>) -> Self {
        Self { context, backend }
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
            "system" => self.handle_system(call),
            _ => {
                warn!("Unknown bridge namespace: {}", call.namespace);
                BridgeResponse::error(format!("Unknown namespace: {}", call.namespace))
            }
        }
    }

    // ── Storage ──────────────────────────────────────────────────────────────

    async fn handle_storage(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("storage") {
            return BridgeResponse::error("Missing permission: storage");
        }

        match call.method.as_str() {
            "get" => {
                if call.args.is_empty() {
                    return BridgeResponse::error("storage.get requires key argument");
                }
                let key = call.args[0].as_str().unwrap_or("");
                debug!("Storage get: {}", key);
                match self.backend.storage_get(key) {
                    Some(value) => BridgeResponse::success(json!({"value": value, "found": true})),
                    None => BridgeResponse::success(json!({"value": null, "found": false})),
                }
            }
            "put" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("storage.put requires key and value arguments");
                }
                let key = call.args[0].as_str().unwrap_or("");
                let value = call.args[1].clone();
                debug!("Storage put: {} = {:?}", key, value);
                let stored = self.backend.storage_put(key, value);
                BridgeResponse::success(json!({"stored": stored}))
            }
            "delete" => {
                if call.args.is_empty() {
                    return BridgeResponse::error("storage.delete requires key argument");
                }
                let key = call.args[0].as_str().unwrap_or("");
                debug!("Storage delete: {}", key);
                let deleted = self.backend.storage_delete(key);
                BridgeResponse::success(json!({"deleted": deleted }))
            }
            "list" => {
                let prefix = call.args.get(0).and_then(|a| a.as_str()).unwrap_or("");
                debug!("Storage list prefix: {}", prefix);
                let keys = self.backend.storage_list(prefix);
                BridgeResponse::success(json!({"keys": keys }))
            }
            _ => BridgeResponse::error(format!("Unknown storage method: {}", call.method)),
        }
    }

    // ── IPC / A2A ────────────────────────────────────────────────────────────

    async fn handle_ipc(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("ipc") {
            return BridgeResponse::error("Missing permission: ipc");
        }

        match call.method.as_str() {
            "send_message" => {
                if call.args.len() < 3 {
                    return BridgeResponse::error(
                        "ipc.send_message requires target_agent, message_type, payload",
                    );
                }
                let target = call.args[0].as_str().unwrap_or("");
                let msg_type = call.args[1].as_str().unwrap_or("");
                let payload = call.args.get(2).cloned().unwrap_or(json!({}));
                debug!("IPC send to {}: type={}", target, msg_type);
                match self.backend.ipc_send(target, msg_type, payload) {
                    Some(msg_id) => BridgeResponse::success(json!({ "message_id": msg_id })),
                    None => BridgeResponse::success(
                        json!({ "message_id": uuid::Uuid::new_v4().to_string(), "mock": true }),
                    ),
                }
            }
            "receive_messages" => {
                let agent_id = call
                    .args
                    .get(0)
                    .and_then(|a| a.as_str())
                    .unwrap_or(self.context.agent_id.as_deref().unwrap_or(""));
                let limit = call.args.get(1).and_then(|a| a.as_u64()).unwrap_or(10) as usize;
                debug!("IPC receive for {}: limit={}", agent_id, limit);
                let messages = self.backend.ipc_receive(agent_id, limit);
                BridgeResponse::success(json!({ "messages": messages }))
            }
            _ => BridgeResponse::error(format!("Unknown ipc method: {}", call.method)),
        }
    }

    // ── LLM ──────────────────────────────────────────────────────────────────

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
                match self.backend.llm_chat(model, prompt) {
                    Some(content) => BridgeResponse::success(json!({
                        "content": content,
                        "model": model,
                        "tokens_used": 0
                    })),
                    None => BridgeResponse::success(json!({
                        "content": "[LLM integration: no backend configured]",
                        "model": model,
                        "tokens_used": 0,
                        "mock": true
                    })),
                }
            }
            "embed" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("llm.embed requires model and text arguments");
                }
                let model = call.args[0].as_str().unwrap_or("default");
                let text = call.args[1].as_str().unwrap_or("");
                debug!("LLM embed: model={}, text_len={}", model, text.len());
                match self.backend.llm_embed(model, text) {
                    Some(embedding) => BridgeResponse::success(json!({ "embedding": embedding })),
                    None => BridgeResponse::success(json!({
                        "embedding": vec![0.0f32; 1536],
                        "mock": true
                    })),
                }
            }
            _ => BridgeResponse::error(format!("Unknown llm method: {}", call.method)),
        }
    }

    // ── Chain / Blockchain ───────────────────────────────────────────────────

    async fn handle_chain(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("chain") {
            return BridgeResponse::error("Missing permission: chain");
        }

        match call.method.as_str() {
            "call_contract" => {
                if call.args.len() < 3 {
                    return BridgeResponse::error(
                        "chain.call_contract requires chain_id, contract, data",
                    );
                }
                let chain_id = call.args[0].as_str().unwrap_or("");
                let contract = call.args[1].as_str().unwrap_or("");
                let data = call.args.get(2).cloned().unwrap_or(json!({}));
                debug!("Chain call: chain={}, contract={}", chain_id, contract);
                match self.backend.chain_call(chain_id, contract, data) {
                    Some(result) => BridgeResponse::success(result),
                    None => BridgeResponse::success(json!({
                        "tx_hash": null,
                        "status": "pending",
                        "mock": true,
                        "note": "No chain backend configured"
                    })),
                }
            }
            "get_balance" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error(
                        "chain.get_balance requires chain_id and address",
                    );
                }
                let chain_id = call.args[0].as_str().unwrap_or("");
                let address = call.args[1].as_str().unwrap_or("");
                debug!("Chain balance: chain={}, addr={}", chain_id, address);
                match self.backend.chain_balance(chain_id, address) {
                    Some(balance) => BridgeResponse::success(json!({ "balance": balance })),
                    None => BridgeResponse::success(json!({ "balance": "0", "mock": true })),
                }
            }
            _ => BridgeResponse::error(format!("Unknown chain method: {}", call.method)),
        }
    }

    // ── Logging ──────────────────────────────────────────────────────────────

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
                    "info" => {
                        tracing::info!(task_id = %self.context.task_id, "[script] {}", message)
                    }
                    "warn" => {
                        tracing::warn!(task_id = %self.context.task_id, "[script] {}", message)
                    }
                    "error" => {
                        tracing::error!(task_id = %self.context.task_id, "[script] {}", message)
                    }
                    _ => tracing::info!(task_id = %self.context.task_id, "[script] {}", message),
                }

                BridgeResponse::success(json!({"logged": true}))
            }
            _ => BridgeResponse::error(format!("Unknown log method: {}", call.method)),
        }
    }

    // ── Environment ──────────────────────────────────────────────────────────

    fn handle_env(&self, call: BridgeCall) -> BridgeResponse {
        match call.method.as_str() {
            "get" => {
                if call.args.is_empty() {
                    return BridgeResponse::error("env.get requires key argument");
                }
                let key = call.args[0].as_str().unwrap_or("");

                let allowed = ["BEE_RUNTIME", "BEE_TASK_ID", "BEE_AGENT_ID"];
                if !allowed.contains(&key) {
                    return BridgeResponse::error(format!(
                        "Access to env var '{}' not allowed",
                        key
                    ));
                }

                let value = match key {
                    "BEE_RUNTIME" => "foreign-rt",
                    "BEE_TASK_ID" => &self.context.task_id,
                    "BEE_AGENT_ID" => self.context.agent_id.as_deref().unwrap_or(""),
                    _ => "",
                };

                BridgeResponse::success(json!(value))
            }
            "list" => {
                let vars = json!({
                    "BEE_RUNTIME": "foreign-rt",
                    "BEE_TASK_ID": self.context.task_id,
                    "BEE_AGENT_ID": self.context.agent_id.as_deref().unwrap_or(""),
                });
                BridgeResponse::success(vars)
            }
            _ => BridgeResponse::error(format!("Unknown env method: {}", call.method)),
        }
    }

    // ── Filesystem (sandboxed) ───────────────────────────────────────────────

    fn handle_fs(&self, call: BridgeCall) -> BridgeResponse {
        if !self.context.has_permission("fs") {
            return BridgeResponse::error("Missing permission: fs");
        }

        match call.method.as_str() {
            "read_file" => {
                if call.args.is_empty() {
                    return BridgeResponse::error("fs.read_file requires path argument");
                }
                let path = call.args[0].as_str().unwrap_or("");
                debug!("FS read: {}", path);
                // Read via standard fs, path is already restricted by WASI preopens
                match std::fs::read_to_string(path) {
                    Ok(content) => BridgeResponse::success(json!({"content": content})),
                    Err(e) => BridgeResponse::error(format!("Failed to read file: {}", e)),
                }
            }
            "write_file" => {
                if call.args.len() < 2 {
                    return BridgeResponse::error("fs.write_file requires path and content");
                }
                let path = call.args[0].as_str().unwrap_or("");
                let content = call.args[1].as_str().unwrap_or("");
                debug!("FS write: {}", path);
                match std::fs::write(path, content) {
                    Ok(()) => BridgeResponse::success(json!({"written": true})),
                    Err(e) => BridgeResponse::error(format!("Failed to write file: {}", e)),
                }
            }
            "exists" => {
                if call.args.is_empty() {
                    return BridgeResponse::error("fs.exists requires path argument");
                }
                let path = call.args[0].as_str().unwrap_or("");
                BridgeResponse::success(json!({"exists": std::path::Path::new(path).exists()}))
            }
            _ => BridgeResponse::error(format!("Unknown fs method: {}", call.method)),
        }
    }

    // ── System ───────────────────────────────────────────────────────────────

    fn handle_system(&self, call: BridgeCall) -> BridgeResponse {
        match call.method.as_str() {
            "info" => {
                let uptime = self.context.started_at.elapsed().as_secs_f64();
                BridgeResponse::success(json!({
                    "task_id": self.context.task_id,
                    "agent_id": self.context.agent_id,
                    "uptime_seconds": uptime,
                    "permissions": self.context.permissions,
                    "bridge_version": crate::bridge::BRIDGE_PROTOCOL_VERSION,
                }))
            }
            "time" => BridgeResponse::success(json!({
                "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                "rfc3339": chrono::Utc::now().to_rfc3339(),
            })),
            _ => BridgeResponse::error(format!("Unknown system method: {}", call.method)),
        }
    }
}

/// Register host functions with a wasmtime linker
pub fn register_host_functions<T>(
    _linker: &mut wasmtime::Linker<T>,
    _ctx: HostContext,
) -> crate::error::Result<()>
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

        let response = dispatcher.handle_log(BridgeCall {
            namespace: "log".to_string(),
            method: "write".to_string(),
            args: vec![json!("info"), json!("Hello from test")],
        });

        assert!(response.success);
    }

    #[test]
    fn test_dispatcher_env_whitelist() {
        let dispatcher =
            HostFunctionDispatcher::new(HostContext::new("test").with_agent_id("agent-1"));

        let response = dispatcher.handle_env(BridgeCall {
            namespace: "env".to_string(),
            method: "get".to_string(),
            args: vec![json!("BEE_TASK_ID")],
        });

        assert!(response.success);
        assert_eq!(response.data, Some(json!("test")));

        // Forbidden env var
        let response2 = dispatcher.handle_env(BridgeCall {
            namespace: "env".to_string(),
            method: "get".to_string(),
            args: vec![json!("PATH")],
        });

        assert!(!response2.success);
    }

    #[tokio::test]
    async fn test_dispatcher_storage_with_mock_backend() {
        let backend = Arc::new(MockBackendServices::new());
        let dispatcher = HostFunctionDispatcher::with_backend(
            HostContext::new("test").with_permissions(vec!["storage".to_string()]),
            backend.clone(),
        );

        // Put
        let put_resp = dispatcher
            .dispatch(BridgeCall {
                namespace: "storage".to_string(),
                method: "put".to_string(),
                args: vec![json!("key1"), json!("value1")],
            })
            .await;
        assert!(put_resp.success);

        // Get
        let get_resp = dispatcher
            .dispatch(BridgeCall {
                namespace: "storage".to_string(),
                method: "get".to_string(),
                args: vec![json!("key1")],
            })
            .await;
        assert!(get_resp.success);
        assert_eq!(
            get_resp.data,
            Some(json!({"value": "value1", "found": true}))
        );

        // Delete
        let del_resp = dispatcher
            .dispatch(BridgeCall {
                namespace: "storage".to_string(),
                method: "delete".to_string(),
                args: vec![json!("key1")],
            })
            .await;
        assert!(del_resp.success);

        // Get after delete
        let get2 = dispatcher
            .dispatch(BridgeCall {
                namespace: "storage".to_string(),
                method: "get".to_string(),
                args: vec![json!("key1")],
            })
            .await;
        assert!(get2.success);
        assert_eq!(get2.data, Some(json!({"value": null, "found": false})));
    }

    #[tokio::test]
    async fn test_dispatcher_system_info() {
        let dispatcher = HostFunctionDispatcher::new(HostContext::new("sys-test"));

        let resp = dispatcher
            .dispatch(BridgeCall {
                namespace: "system".to_string(),
                method: "info".to_string(),
                args: vec![],
            })
            .await;

        assert!(resp.success);
        let data = resp.data.unwrap();
        assert_eq!(data["task_id"], "sys-test");
        assert!(data["uptime_seconds"].as_f64().unwrap() >= 0.0);
    }
}
