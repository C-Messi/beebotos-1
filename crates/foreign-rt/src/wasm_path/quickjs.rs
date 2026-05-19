//! QuickJS WASM runtime executor
//!
//! Executes JavaScript/TypeScript code using QuickJS compiled to WASM
//! running inside wasmtime with full WASI support.

use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::config::WasmPathConfig;
use crate::error::{ForeignRtError, Result};
use crate::script_task::{ForeignRuntime, ScriptResult, ScriptTask};
use crate::wasm_path::executor::WasmScriptExecutor;
use crate::wasm_path::{WasmRuntimeEngine, WasmRuntimeExecutor};

/// QuickJS WASM executor
pub struct QuickJsExecutor {
    /// WASM script executor
    executor: WasmScriptExecutor,
    /// Module bytes (loaded on init)
    module_bytes: Option<Vec<u8>>,
    /// Configuration
    config: WasmPathConfig,
}

impl QuickJsExecutor {
    /// Create a new QuickJS executor
    pub fn new(engine: Arc<WasmRuntimeEngine>, config: WasmPathConfig) -> Result<Self> {
        // Load QuickJS module bytes if path is configured
        let module_bytes = if let Some(ref path) = config.quickjs_module_path {
            match std::fs::read(path) {
                Ok(bytes) => {
                    info!("Loaded QuickJS module: {} bytes", bytes.len());
                    Some(bytes)
                }
                Err(e) => {
                    warn!("Failed to load QuickJS module from {:?}: {}", path, e);
                    None
                }
            }
        } else {
            None
        };

        let executor = WasmScriptExecutor::new(engine.engine().clone());

        // Precompile module if available
        if let Some(ref bytes) = module_bytes {
            if let Err(e) = executor.get_or_compile_module(bytes, ForeignRuntime::NodeJs) {
                warn!("Failed to precompile QuickJS module: {}", e);
            }
        }

        Ok(Self {
            executor,
            module_bytes,
            config,
        })
    }

    /// Check if QuickJS is available
    pub fn is_available(&self) -> bool {
        self.module_bytes.is_some()
    }

    /// Prepare JavaScript source code wrapper
    fn prepare_code(&self, task: &ScriptTask) -> String {
        let user_code = match &task.source {
            crate::script_task::ScriptSource::Inline { code } => code.clone(),
            crate::script_task::ScriptSource::File { path } => {
                match std::fs::read_to_string(path) {
                    Ok(content) => content,
                    Err(e) => {
                        return format!(
                            r#"console.error("Failed to read file {}: {}"); throw new Error("File read failed");"#,
                            path.display(),
                            e
                        );
                    }
                }
            }
            crate::script_task::ScriptSource::Prebuilt { module_id, .. } => {
                return format!(
                    r#"console.error("Prebuilt modules not supported in QuickJS path: {}"); throw new Error("Prebuilt not supported");"#,
                    module_id
                );
            }
        };

        // Wrap user code with BeeBotOS bridge
        format!(
            r#"// BeeBotOS JavaScript Bridge (QuickJS)
const BEE_TASK_ID = {task_id:?};
const BEE_AGENT_ID = {agent_id:?};

const beebotos = {{
    log(level, message) {{
        console.error(`[${{level}}] ${{message}}`);
    }},
    storage: {{
        get(key) {{ this.log("debug", `storage.get(${{key}})`); return null; }},
        put(key, value) {{ this.log("debug", `storage.put(${{key}})`); return true; }}
    }},
    a2a: {{
        sendMessage(targetAgent, messageType, payload) {{
            this.log("debug", `ipc.send(${{targetAgent}}, ${{messageType}})`);
            return null;
        }}
    }},
    llm: {{
        chat(model, prompt) {{
            this.log("debug", `llm.chat(${{model}})`);
            return "[LLM integration pending]";
        }}
    }}
}};

// Shim common Node.js APIs
globalThis.process = {{
    env: {{ BEE_TASK_ID, BEE_AGENT_ID, NODE_ENV: "production" }},
    exit(code) {{ throw new Error(`Process exit with code ${{code}}`); }}
}};

globalThis.Buffer = class Buffer {{
    constructor(data) {{ this._data = data; }}
    static from(data) {{ return new Buffer(data); }}
    toString(encoding = 'utf8') {{ return String(this._data); }}
}};

// User code
{user_code}

// Execute entrypoint if it's a function
const input = {input_json};
if (typeof {entrypoint} === 'function') {{
    const result = {entrypoint}(input);
    if (result !== undefined) {{
        console.log(JSON.stringify(result));
    }}
}} else {{
    console.error(`Entrypoint '{entrypoint}' is not a function`);
    throw new Error("Invalid entrypoint");
}}
"#,
            task_id = task.task_id,
            agent_id = task.agent_id.as_deref().unwrap_or(""),
            user_code = user_code,
            entrypoint = task.entrypoint,
            input_json = serde_json::to_string(&task.input).unwrap_or_else(|_| "null".to_string()),
        )
    }
}

#[async_trait::async_trait]
impl WasmRuntimeExecutor for QuickJsExecutor {
    async fn execute(&self, task: &ScriptTask) -> Result<ScriptResult> {
        let _start = Instant::now();

        if !self.is_available() {
            return Err(ForeignRtError::RuntimeNotAvailable(
                "QuickJS module not loaded".to_string(),
            ));
        }

        debug!(
            task_id = %task.task_id,
            "Executing JavaScript via QuickJS WASM"
        );

        let module_bytes = self.module_bytes.as_ref().unwrap();
        let code = self.prepare_code(task);

        // Create a modified task with the wrapped code
        let mut wrapped_task = task.clone();
        wrapped_task.source = crate::script_task::ScriptSource::Inline { code };

        // Execute via the generic WASM executor
        let result = self
            .executor
            .execute(module_bytes, &wrapped_task, "_start")
            .await;

        match result {
            Ok(script_result) => {
                info!(
                    task_id = %task.task_id,
                    success = script_result.success,
                    duration_ms = script_result.execution_time.as_millis(),
                    "QuickJS execution completed"
                );
                Ok(script_result)
            }
            Err(e) => {
                warn!(task_id = %task.task_id, error = %e, "QuickJS execution failed");
                Err(e)
            }
        }
    }

    fn runtime_type(&self) -> ForeignRuntime {
        ForeignRuntime::NodeJs
    }

    async fn prewarm(&self, count: usize) -> Result<()> {
        info!("Pre-warming {} QuickJS instances", count);
        // Module is already compiled and cached by WasmScriptExecutor.
        // In a full implementation, we would pre-initialize wasmtime Stores.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::script_task::ScriptSource;

    fn test_config() -> WasmPathConfig {
        WasmPathConfig {
            quickjs_module_path: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_prepare_code() {
        let engine = Arc::new(WasmRuntimeEngine::new(WasmPathConfig::default()).unwrap());
        let executor = QuickJsExecutor::new(engine, test_config()).unwrap();

        let task = ScriptTask {
            task_id: "test-1".to_string(),
            runtime: ForeignRuntime::NodeJs,
            source: ScriptSource::Inline {
                code: "function main(input) { return { result: input.x + 1 }; }".to_string(),
            },
            entrypoint: "main".to_string(),
            input: serde_json::json!({"x": 42}),
            sandbox: Default::default(),
            permissions: vec![],
            timeout: Duration::from_secs(30),
            agent_id: Some("agent-1".to_string()),
        };

        let code = executor.prepare_code(&task);
        assert!(code.contains("beebotos"));
        assert!(code.contains("function main(input) { return { result: input.x + 1 }; }"));
        assert!(code.contains("globalThis.process"));
    }
}
