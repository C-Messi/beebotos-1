//! Pyodide WASM runtime executor
//!
//! Executes Python code using Pyodide (CPython compiled to WASM)
//! running inside wasmtime.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use crate::config::WasmPathConfig;
use crate::error::{ForeignRtError, Result};
use crate::script_task::{ForeignRuntime, ScriptResult, ScriptTask};
use crate::wasm_path::{WasmExecutorUtils, WasmRuntimeEngine, WasmRuntimeExecutor};

/// Pyodide WASM executor
pub struct PyodideExecutor {
    /// WASM engine
    engine: Arc<WasmRuntimeEngine>,
    /// Module bytes (loaded on init)
    module_bytes: Option<Vec<u8>>,
    /// Configuration
    config: WasmPathConfig,
}

impl PyodideExecutor {
    /// Create a new Pyodide executor
    pub fn new(engine: Arc<WasmRuntimeEngine>, config: WasmPathConfig) -> Result<Self> {
        // Load Pyodide module bytes if path is configured
        let module_bytes = if let Some(ref path) = config.pyodide_module_path {
            match std::fs::read(path) {
                Ok(bytes) => {
                    info!("Loaded Pyodide module: {} bytes", bytes.len());
                    Some(bytes)
                }
                Err(e) => {
                    warn!("Failed to load Pyodide module from {:?}: {}", path, e);
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

    /// Check if Pyodide is available
    pub fn is_available(&self) -> bool {
        self.module_bytes.is_some()
    }

    /// Prepare Python source code wrapper
    ///
    /// Wraps user code in a function and adds beebotos bridge imports
    fn prepare_code(&self, task: &ScriptTask) -> String {
        let user_code = match &task.source {
            crate::script_task::ScriptSource::Inline { code } => code.clone(),
            crate::script_task::ScriptSource::File { path } => {
                // In WASM path, file sources are read and embedded
                match std::fs::read_to_string(path) {
                    Ok(content) => content,
                    Err(e) => {
                        return format!(
                            r#"raise RuntimeError("Failed to read file {}: {}")"#,
                            path.display(),
                            e
                        );
                    }
                }
            }
            crate::script_task::ScriptSource::Prebuilt { module_id, .. } => {
                return format!(
                    r#"raise RuntimeError("Prebuilt modules not supported in Pyodide path: {}")"#,
                    module_id
                );
            }
        };

        // Wrap user code with bridge setup
        format!(
            r##"
# BeeBotOS Python Bridge (Pyodide)
import json
import sys

class BeeBotOSBridge:
    def __init__(self):
        self._task_id = {task_id:?}
        self._agent_id = {agent_id:?}

    def _call_host(self, namespace, method, args):
        # This will be bridged to HostFunctionDispatcher
        # In full implementation, this calls into JS bridge which calls host funcs
        print(f"[BRIDGE] {{namespace}}.{{method}}({{args}})", file=sys.stderr)
        return None

    def storage_get(self, key):
        return self._call_host("storage", "get", [key])

    def storage_put(self, key, value):
        return self._call_host("storage", "put", [key, value])

    def ipc_send(self, target_agent, message_type, payload):
        return self._call_host("ipc", "send_message", [target_agent, message_type, payload])

    def llm_chat(self, model, prompt):
        return self._call_host("llm", "chat", [model, prompt])

    def log(self, level, message):
        print(f"[{{level}}] {{message}}", file=sys.stderr)

beebotos = BeeBotOSBridge()

# User code
{user_code}

# Execute entrypoint if defined
if "{entrypoint}" in globals():
    result = {entrypoint}({input_json})
    if result is not None:
        print(json.dumps(result, default=str))
else:
    print(json.dumps({{"error": f"Entrypoint '{entrypoint}' not found"}}))
"##,
            task_id = task.task_id,
            agent_id = task.agent_id.as_deref().unwrap_or(""),
            user_code = user_code,
            entrypoint = task.entrypoint,
            input_json = serde_json::to_string(&task.input).unwrap_or_else(|_| "null".to_string()),
        )
    }
}

#[async_trait::async_trait]
impl WasmRuntimeExecutor for PyodideExecutor {
    async fn execute(&self, task: &ScriptTask) -> Result<ScriptResult> {
        let start = Instant::now();

        if !self.is_available() {
            return Err(ForeignRtError::RuntimeNotAvailable(
                "Pyodide module not loaded".to_string(),
            ));
        }

        debug!(
            task_id = %task.task_id,
            "Executing Python script via Pyodide WASM"
        );

        // Prepare wrapped code
        let code = self.prepare_code(task);

        // TODO: Full wasmtime integration with Pyodide
        // This is a placeholder implementation that simulates execution
        // In production, this would:
        // 1. Compile/load the Pyodide WASM module
        // 2. Set up WASI context with preopened directories
        // 3. Inject the Python code into Pyodide's filesystem
        // 4. Run Pyodide's eval_code or similar
        // 5. Capture stdout/stderr
        // 6. Extract fuel consumption from wasmtime Store

        // Simulate execution for now
        let logs = WasmExecutorUtils::parse_logs(&format!(
            "[info] Pyodide execution started for task {}\n[info] Code length: {} bytes",
            task.task_id,
            code.len()
        ));

        let output = serde_json::json!({
            "status": "completed",
            "note": "Full Pyodide wasmtime integration requires Pyodide WASM module and fs setup",
            "task_id": task.task_id,
        });

        let execution_time = start.elapsed();
        let fuel_consumed = 1000u64; // Placeholder

        info!(
            task_id = %task.task_id,
            duration_ms = execution_time.as_millis(),
            "Pyodide execution completed (placeholder)"
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
        ForeignRuntime::Python
    }

    async fn prewarm(&self, count: usize) -> Result<()> {
        info!("Pre-warming {} Pyodide instances (placeholder)", count);
        // TODO: Pre-initialize wasmtime stores with Pyodide module
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_task::ScriptSource;

    fn test_config() -> WasmPathConfig {
        WasmPathConfig {
            pyodide_module_path: None, // Skip loading for tests
            ..Default::default()
        }
    }

    #[test]
    fn test_prepare_code() {
        let engine = Arc::new(
            WasmRuntimeEngine::new(WasmPathConfig::default()).unwrap()
        );
        let executor = PyodideExecutor::new(engine, test_config()).unwrap();

        let task = ScriptTask {
            task_id: "test-1".to_string(),
            runtime: ForeignRuntime::Python,
            source: ScriptSource::Inline {
                code: "def main(input): return {'result': input['x'] + 1}".to_string(),
            },
            entrypoint: "main".to_string(),
            input: serde_json::json!({"x": 42}),
            sandbox: Default::default(),
            permissions: vec![],
            timeout: Duration::from_secs(30),
            agent_id: Some("agent-1".to_string()),
        };

        let code = executor.prepare_code(&task);
        assert!(code.contains("BeeBotOSBridge"));
        assert!(code.contains("def main(input): return {'result': input['x'] + 1}"));
        assert!(code.contains("main("));
    }
}
