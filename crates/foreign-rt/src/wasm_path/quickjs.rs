//! QuickJS WASM runtime executor
//!
//! Executes JavaScript/TypeScript code using QuickJS compiled to WASM
//! running inside wasmtime.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use crate::config::WasmPathConfig;
use crate::error::{ForeignRtError, Result};
use crate::script_task::{ForeignRuntime, ScriptResult, ScriptTask};
use crate::wasm_path::{WasmExecutorUtils, WasmRuntimeEngine, WasmRuntimeExecutor};

/// QuickJS WASM executor
pub struct QuickJsExecutor {
    /// WASM engine
    engine: Arc<WasmRuntimeEngine>,
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

        Ok(Self {
            engine,
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
            r#"
// BeeBotOS JavaScript Bridge (QuickJS)
const BEE_TASK_ID = {task_id:?};
const BEE_AGENT_ID = {agent_id:?};

const beebotos = {{
    _callHost(namespace, method, args) {{
        console.error(`[BRIDGE] ${{namespace}}.${{method}}(${{JSON.stringify(args)}})`);
        // In full implementation, this calls into the host function bridge
        return null;
    }},

    storage: {{
        get(key) {{
            return this._callHost("storage", "get", [key]);
        }},
        put(key, value) {{
            return this._callHost("storage", "put", [key, value]);
        }}
    }},

    a2a: {{
        sendMessage(targetAgent, messageType, payload) {{
            return this._callHost("ipc", "send_message", [targetAgent, messageType, payload]);
        }}
    }},

    llm: {{
        chat(model, prompt) {{
            return this._callHost("llm", "chat", [model, prompt]);
        }}
    }},

    log(level, message) {{
        console.error(`[${{level}}] ${{message}}`);
    }}
}};

// Shim common Node.js APIs
globalThis.process = {{
    env: {{
        BEE_TASK_ID,
        BEE_AGENT_ID,
        NODE_ENV: "production"
    }},
    exit(code) {{
        throw new Error(`Process exit with code ${{code}}`);
    }}
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
        if (result && typeof result.then === 'function') {{
            result.then(r => {{
                if (r !== undefined) console.log(JSON.stringify(r));
            }}).catch(e => {{
                console.error(e);
            }});
        }} else {{
            console.log(JSON.stringify(result));
        }}
    }}
}} else {{
    console.error(`Entrypoint '{entrypoint}' is not a function`);
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
        let start = Instant::now();

        if !self.is_available() {
            return Err(ForeignRtError::RuntimeNotAvailable(
                "QuickJS module not loaded".to_string(),
            ));
        }

        debug!(
            task_id = %task.task_id,
            "Executing JavaScript via QuickJS WASM"
        );

        let code = self.prepare_code(task);

        // TODO: Full wasmtime integration with QuickJS
        // In production, this would:
        // 1. Compile/load the QuickJS WASM module
        // 2. Set up WASI context
        // 3. Inject JS code into QuickJS runtime
        // 4. Execute and capture stdout/stderr
        // 5. Handle async/Promise resolution via host event loop bridge

        let logs = WasmExecutorUtils::parse_logs(&format!(
            "[info] QuickJS execution started for task {}\n[info] Code length: {} bytes",
            task.task_id,
            code.len()
        ));

        let output = serde_json::json!({
            "status": "completed",
            "note": "Full QuickJS wasmtime integration requires QuickJS WASM module",
            "task_id": task.task_id,
        });

        let execution_time = start.elapsed();
        let fuel_consumed = 500u64; // Placeholder (QuickJS is lighter than Pyodide)

        info!(
            task_id = %task.task_id,
            duration_ms = execution_time.as_millis(),
            "QuickJS execution completed (placeholder)"
        );

        Ok(WasmExecutorUtils::build_success(
            task,
            output,
            logs,
            execution_time,
            fuel_consumed,
        ))
    }

    fn runtime_type(&self) -> ForeignRuntime {
        ForeignRuntime::NodeJs
    }

    async fn prewarm(&self, count: usize) -> Result<()> {
        info!("Pre-warming {} QuickJS instances (placeholder)", count);
        // TODO: Pre-initialize wasmtime stores with QuickJS module
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        let engine = Arc::new(
            WasmRuntimeEngine::new(WasmPathConfig::default()).unwrap()
        );
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
