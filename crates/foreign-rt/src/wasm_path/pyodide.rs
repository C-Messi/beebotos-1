//! Pyodide WASM runtime executor
//!
//! Executes Python code using Pyodide (CPython compiled to WASM)
//! running inside wasmtime with full WASI support.

use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::config::WasmPathConfig;
use crate::error::{ForeignRtError, Result};
use crate::script_task::{ForeignRuntime, ScriptResult, ScriptTask};
use crate::wasm_path::executor::WasmScriptExecutor;
use crate::wasm_path::{WasmRuntimeEngine, WasmRuntimeExecutor};

/// Pyodide WASM executor
pub struct PyodideExecutor {
    /// WASM script executor
    executor: WasmScriptExecutor,
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

        let executor = WasmScriptExecutor::new(engine.engine().clone());

        // Precompile module if available
        if let Some(ref bytes) = module_bytes {
            if let Err(e) = executor.get_or_compile_module(bytes, ForeignRuntime::Python) {
                warn!("Failed to precompile Pyodide module: {}", e);
            }
        }

        Ok(Self {
            executor,
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
                match std::fs::read_to_string(path) {
                    Ok(content) => content,
                    Err(e) => {
                        return format!(
                            r#"import sys; print("Failed to read file {}: {}", file=sys.stderr); raise SystemExit(1)"#,
                            path.display(),
                            e
                        );
                    }
                }
            }
            crate::script_task::ScriptSource::Prebuilt { module_id, .. } => {
                return format!(
                    r#"import sys; print("Prebuilt modules not supported in Pyodide path: {}", file=sys.stderr); raise SystemExit(1)"#,
                    module_id
                );
            }
        };

        // Wrap user code with bridge setup
        format!(
            r##"# BeeBotOS Python Bridge (Pyodide)
import json
import sys

class BeeBotOSBridge:
    def __init__(self):
        self._task_id = {task_id:?}
        self._agent_id = {agent_id:?}

    def log(self, level, message):
        print(f"[{{level}}] {{message}}", file=sys.stderr)

    def storage_get(self, key):
        self.log("debug", f"storage.get({{key}})")
        return None

    def storage_put(self, key, value):
        self.log("debug", f"storage.put({{key}}, {{value}})")
        return True

    def ipc_send(self, target_agent, message_type, payload):
        self.log("debug", f"ipc.send({{target_agent}}, {{message_type}})")
        return None

    def llm_chat(self, model, prompt):
        self.log("debug", f"llm.chat({{model}})")
        return "[LLM integration pending]"

beebotos = BeeBotOSBridge()

# User code
{user_code}

# Execute entrypoint if defined
if "{entrypoint}" in globals():
    _result = {entrypoint}({input_json})
    if _result is not None:
        print(json.dumps(_result, default=str))
else:
    print(json.dumps({{"error": f"Entrypoint '{entrypoint}' not found"}}), file=sys.stderr)
    raise SystemExit(1)
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
        let _start = Instant::now();

        if !self.is_available() {
            return Err(ForeignRtError::RuntimeNotAvailable(
                "Pyodide module not loaded".to_string(),
            ));
        }

        debug!(
            task_id = %task.task_id,
            "Executing Python script via Pyodide WASM"
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
                    "Pyodide execution completed"
                );
                Ok(script_result)
            }
            Err(e) => {
                warn!(task_id = %task.task_id, error = %e, "Pyodide execution failed");
                Err(e)
            }
        }
    }

    fn runtime_type(&self) -> ForeignRuntime {
        ForeignRuntime::Python
    }

    async fn prewarm(&self, count: usize) -> Result<()> {
        info!("Pre-warming {} Pyodide instances", count);
        // Module is already compiled and cached by WasmScriptExecutor.
        // In a full implementation, we would pre-initialize wasmtime Stores
        // with the Pyodide module loaded and core packages imported.
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
            pyodide_module_path: None, // Skip loading for tests
            ..Default::default()
        }
    }

    #[test]
    fn test_prepare_code() {
        let engine = Arc::new(WasmRuntimeEngine::new(WasmPathConfig::default()).unwrap());
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
            timeout: Duration::from_secs(180),
            agent_id: Some("agent-1".to_string()),
        };

        let code = executor.prepare_code(&task);
        assert!(code.contains("BeeBotOSBridge"));
        assert!(code.contains("def main(input): return {'result': input['x'] + 1}"));
        assert!(code.contains("main("));
    }
}
