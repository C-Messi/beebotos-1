//! Generic WASM script execution engine
//!
//! Provides real wasmtime execution for foreign runtimes with:
//! - WASI preview1 context creation with stdout/stderr capture
//! - Fuel metering and memory limits
//! - Directory preopening for sandboxed filesystem access
//! - Module compilation cache

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use tracing::{debug, info, warn};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::p2::WasiCtxBuilder;
use wasmtime_wasi::{DirPerms, FilePerms};

use crate::error::{ForeignRtError, Result};
use crate::metering::ForeignGasReport;
use crate::script_task::{ForeignRuntime, LogEntry, LogLevel, ScriptResult, ScriptTask};

/// State for foreign runtime WASM execution using WASI preview1
pub struct ForeignPreview1State {
    /// WASI preview1 context
    wasi: wasmtime_wasi::preview1::WasiP1Ctx,
    /// Captured stdout
    stdout_pipe: MemoryOutputPipe,
    /// Captured stderr
    stderr_pipe: MemoryOutputPipe,
    /// Task ID for logging
    task_id: String,
}

impl ForeignPreview1State {
    /// Create a new WASI preview1 state for script execution
    pub fn new(
        task_id: impl Into<String>,
        code_input: Option<String>,
        args: Vec<String>,
        env_vars: HashMap<String, String>,
        preopen_dirs: Vec<(PathBuf, String, DirPerms, FilePerms)>,
        stdout_capacity: usize,
        stderr_capacity: usize,
    ) -> Result<Self> {
        let task_id = task_id.into();

        // Build stdout/stderr capture pipes
        let stdout_pipe = MemoryOutputPipe::new(stdout_capacity);
        let stderr_pipe = MemoryOutputPipe::new(stderr_capacity);

        let mut builder = WasiCtxBuilder::new();

        // Set stdout/stderr to capture pipes
        builder.stdout(stdout_pipe.clone());
        builder.stderr(stderr_pipe.clone());

        // Set stdin if code input provided
        if let Some(input) = code_input {
            let stdin = MemoryInputPipe::new(input.into_bytes());
            builder.stdin(stdin);
        }

        // Set command line arguments
        if !args.is_empty() {
            builder.args(&args);
        }

        // Set environment variables
        for (key, value) in env_vars {
            builder.env(&key, &value);
        }

        // Preopen directories
        for (host_path, guest_path, dir_perms, file_perms) in preopen_dirs {
            if !host_path.exists() {
                if let Err(e) = std::fs::create_dir_all(&host_path) {
                    warn!(
                        task_id = %task_id,
                        path = ?host_path,
                        "Failed to create directory: {}",
                        e
                    );
                    continue;
                }
            }
            if let Err(e) = builder.preopened_dir(&host_path, &guest_path, dir_perms, file_perms) {
                warn!(
                    task_id = %task_id,
                    path = ?host_path,
                    "Failed to preopen directory: {}",
                    e
                );
            } else {
                debug!(
                    task_id = %task_id,
                    host = ?host_path,
                    guest = %guest_path,
                    "Preopened directory"
                );
            }
        }

        let wasi = builder.build_p1();

        Ok(Self {
            wasi,
            stdout_pipe,
            stderr_pipe,
            task_id,
        })
    }

    /// Get captured stdout contents
    pub fn stdout_contents(&self) -> String {
        String::from_utf8_lossy(&self.stdout_pipe.contents()).to_string()
    }

    /// Get captured stderr contents
    pub fn stderr_contents(&self) -> String {
        String::from_utf8_lossy(&self.stderr_pipe.contents()).to_string()
    }
}

/// Generic WASM script executor
pub struct WasmScriptExecutor {
    /// wasmtime engine
    engine: Engine,
    /// Module cache: (runtime_type, module_hash) -> Module
    module_cache: parking_lot::Mutex<HashMap<String, Module>>,
}

impl WasmScriptExecutor {
    /// Create a new WASM script executor from an engine
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            module_cache: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Execute a script by compiling/instantiating a WASM module
    ///
    /// # Arguments
    /// * `module_bytes` - The WASM module bytes
    /// * `task` - The script task containing code, input, sandbox config
    /// * `entrypoint` - The function to call (e.g., "_start" or "main").
    ///   If "_start", uses WASI command convention.
    pub async fn execute(
        &self,
        module_bytes: &[u8],
        task: &ScriptTask,
        entrypoint: &str,
    ) -> Result<ScriptResult> {
        let start = Instant::now();
        let task_id = task.task_id.clone();

        debug!(task_id = %task_id, "Starting WASM script execution");

        // 1. Compile or get cached module
        let module = self.get_or_compile_module(module_bytes, task.runtime)?;

        // 2. Prepare code input and arguments
        let code = match &task.source {
            crate::script_task::ScriptSource::Inline { code } => code.clone(),
            crate::script_task::ScriptSource::File { path } => {
                std::fs::read_to_string(path)
                    .map_err(|e| ForeignRtError::Io(format!("Failed to read script file: {}", e)))?
            }
            crate::script_task::ScriptSource::Prebuilt { module_id, .. } => {
                return Err(ForeignRtError::InvalidConfig(format!(
                    "Prebuilt modules not supported in WASM path: {}",
                    module_id
                )));
            }
        };

        // 3. Prepare environment
        let mut env_vars = HashMap::new();
        env_vars.insert("BEE_TASK_ID".to_string(), task.task_id.clone());
        if let Some(agent_id) = &task.agent_id {
            env_vars.insert("BEE_AGENT_ID".to_string(), agent_id.clone());
        }
        env_vars.insert("BEE_RUNTIME".to_string(), task.runtime.name().to_string());

        // 4. Prepare preopen directories
        let mut preopen_dirs = Vec::new();

        // /tmp - writable
        let tmp_dir = std::env::temp_dir().join(format!("beebotos-wasm-{}", task_id));
        preopen_dirs.push((
            tmp_dir.clone(),
            "/tmp".to_string(),
            DirPerms::all(),
            FilePerms::all(),
        ));

        // /workspace - read-only if filesystem paths specified
        for mapping in &task.sandbox.filesystem_paths {
            preopen_dirs.push((
                mapping.host_path.clone(),
                mapping.guest_path.to_string_lossy().to_string(),
                if mapping.read_only {
                    DirPerms::READ
                } else {
                    DirPerms::all()
                },
                if mapping.read_only {
                    FilePerms::READ
                } else {
                    FilePerms::all()
                },
            ));
        }

        // 5. Write code to /tmp/script.{ext} for modules that read from filesystem
        let ext = task.runtime.extension();
        let script_path = tmp_dir.join(format!("script.{}", ext));
        if let Err(e) = tokio::fs::create_dir_all(&tmp_dir).await {
            warn!("Failed to create tmp dir: {}", e);
        }
        if let Err(e) = tokio::fs::write(&script_path, &code).await {
            warn!("Failed to write script to tmp: {}", e);
        }

        // 6. Create WASI preview1 state with captured stdio
        let stdout_cap = task.sandbox.max_memory_mb * 1024 * 1024;
        let stderr_cap = 1024 * 1024; // 1MB for stderr

        let state = ForeignPreview1State::new(
            &task_id,
            Some(code.clone()),
            vec![
                "beebotos-script".to_string(),
                format!("/tmp/script.{}", ext),
            ],
            env_vars,
            preopen_dirs,
            stdout_cap,
            stderr_cap,
        )?;

        // 7. Create store with fuel metering
        let mut store = Store::new(&self.engine, state);

        // Configure fuel if enabled
        let fuel_enabled = store.set_fuel(10_000_000).is_ok();

        // 8. Create linker and add WASI preview1
        let mut linker = Linker::<ForeignPreview1State>::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state| &mut state.wasi)
            .map_err(|e| {
                ForeignRtError::WasmRuntime(format!("Failed to add WASI to linker: {}", e))
            })?;

        // 9. Instantiate module
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| ForeignRtError::WasmRuntime(format!("Failed to instantiate module: {}", e)))?;

        // 10. Call entrypoint
        let call_result = if entrypoint == "_start" {
            // WASI command entrypoint
            if let Some(func) = instance.get_export(&mut store, "_start").and_then(|e| e.into_func()) {
                func.call(&mut store, &[], &mut [])
                    .map_err(|e| ForeignRtError::ExecutionFailed(format!("WASM _start failed: {}", e)))
            } else {
                Err(ForeignRtError::ExecutionFailed(
                    "WASM module does not export _start".to_string(),
                ))
            }
        } else {
            // Custom entrypoint
            call_custom_entrypoint(&mut store, &instance, entrypoint).await
        };

        // 11. Extract output
        let stdout = store.data().stdout_contents();
        let stderr = store.data().stderr_contents();

        // Clean up temp directory
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

        // 12. Parse output
        let output = if call_result.is_ok() && !stdout.is_empty() {
            parse_json_output(&stdout)?
        } else {
            serde_json::Value::Null
        };

        let logs = parse_logs(&stderr);

        let execution_time = start.elapsed();

        // 13. Calculate fuel consumed
        let fuel_consumed = if fuel_enabled {
            10_000_000u64.saturating_sub(store.get_fuel().unwrap_or(0))
        } else {
            0
        };

        let mut gas_report = ForeignGasReport::new();
        gas_report.add_compute(fuel_consumed);
        gas_report.add_memory((task.sandbox.max_memory_mb * 1024 * 1024) as u64);

        info!(
            task_id = %task_id,
            success = call_result.is_ok(),
            duration_ms = execution_time.as_millis(),
            fuel_consumed,
            "WASM execution completed"
        );

        if call_result.is_ok() {
            Ok(ScriptResult::success(&task_id, output, execution_time)
                .with_logs(logs)
                .with_gas_report(gas_report))
        } else {
            let error_msg = format!(
                "WASM execution failed: {}. stderr: {}",
                call_result.unwrap_err(),
                stderr.trim()
            );
            Ok(ScriptResult::failure(&task_id, error_msg, execution_time)
                .with_logs(logs)
                .with_gas_report(gas_report))
        }
    }

    /// Get or compile a WASM module
    pub(crate) fn get_or_compile_module(&self, bytes: &[u8], runtime: ForeignRuntime) -> Result<Module> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        let cache_key = format!("{}:{:x}", runtime.name(), hash);

        {
            let cache = self.module_cache.lock();
            if let Some(module) = cache.get(&cache_key) {
                debug!("Using cached WASM module: {}", cache_key);
                return Ok(module.clone());
            }
        }

        debug!("Compiling WASM module: {} ({} bytes)", cache_key, bytes.len());
        let module = Module::new(&self.engine, bytes).map_err(|e| {
            ForeignRtError::CompilationFailed(format!("WASM module compilation failed: {}", e))
        })?;

        self.module_cache.lock().insert(cache_key, module.clone());
        Ok(module)
    }

    /// Clear module cache
    pub fn clear_cache(&self) {
        self.module_cache.lock().clear();
    }
}

/// Call a custom WASM entrypoint function
async fn call_custom_entrypoint(
    store: &mut Store<ForeignPreview1State>,
    instance: &wasmtime::Instance,
    name: &str,
) -> Result<()> {
    let func = instance
        .get_export(&mut *store, name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| ForeignRtError::ExecutionFailed(format!("Function '{}' not found", name)))?;

    func.call(&mut *store, &[], &mut [])
        .map_err(|e| ForeignRtError::ExecutionFailed(format!("Function call failed: {}", e)))?;

    Ok(())
}

/// Parse JSON output from stdout
///
/// Attempts to find and parse a JSON object or array from the output,
/// handling cases where the output contains trailing/leading text.
/// Uses proper brace counting that respects string literals.
fn parse_json_output(stdout: &str) -> Result<serde_json::Value> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }

    // Try to find JSON object/array using proper brace matching
    let bytes = trimmed.as_bytes();
    for i in 0..bytes.len() {
        let c = bytes[i] as char;
        if c == '{' || c == '[' {
            if let Some(end) = find_json_end(trimmed, i) {
                let json_part = &trimmed[i..=end];
                if let Ok(val) = serde_json::from_str(json_part) {
                    return Ok(val);
                }
            }
        }
    }

    // Fallback: return as string
    Ok(serde_json::Value::String(trimmed.to_string()))
}

/// Find the end position of a JSON object/array starting at `start`
/// using brace counting that respects string literals.
fn find_json_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if start >= bytes.len() {
        return None;
    }

    let open = bytes[start] as char;
    let close = match open {
        '{' => '}',
        '[' => ']',
        _ => return None,
    };

    let mut depth = 1;
    let mut in_string = false;
    let mut escape_next = false;

    for i in (start + 1)..bytes.len() {
        let c = bytes[i] as char;

        if escape_next {
            escape_next = false;
            continue;
        }

        if c == '\\' && in_string {
            escape_next = true;
            continue;
        }

        if c == '"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }

    None
}

/// Parse stderr into log entries
fn parse_logs(stderr: &str) -> Vec<LogEntry> {
    stderr
        .lines()
        .map(|line| {
            let level = if line.to_lowercase().contains("error") {
                LogLevel::Error
            } else if line.to_lowercase().contains("warn") {
                LogLevel::Warn
            } else if line.to_lowercase().contains("debug") {
                LogLevel::Debug
            } else {
                LogLevel::Info
            };
            LogEntry {
                level,
                message: line.to_string(),
                timestamp: Some(chrono::Utc::now()),
                source: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_output() {
        let out = "some prefix\n{\"result\": 42}\n trailing";
        let val = parse_json_output(out).unwrap();
        assert_eq!(val["result"], 42);

        let out2 = "plain text";
        let val2 = parse_json_output(out2).unwrap();
        assert_eq!(val2, serde_json::Value::String("plain text".to_string()));
    }

    #[test]
    fn test_parse_json_with_braces_in_string() {
        let out = r#"{"key": "}", "nested": {"a": 1}}"#;
        let val = parse_json_output(out).unwrap();
        assert_eq!(val["key"], "}");
        assert_eq!(val["nested"]["a"], 1);
    }

    #[test]
    fn test_parse_json_array() {
        let out = "prefix [1, 2, 3] suffix";
        let val = parse_json_output(out).unwrap();
        assert_eq!(val, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_parse_json_nested() {
        let out = r#"{"outer": {"inner": [1, 2, 3]}}"#;
        let val = parse_json_output(out).unwrap();
        assert_eq!(val["outer"]["inner"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_parse_json_empty() {
        let out = "";
        let val = parse_json_output(out).unwrap();
        assert_eq!(val, serde_json::Value::Null);
    }

    #[test]
    fn test_parse_logs() {
        let stderr = "[info] starting\n[error] failed\n[warn] caution";
        let logs = parse_logs(stderr);
        assert_eq!(logs.len(), 3);
        assert!(matches!(logs[1].level, LogLevel::Error));
    }
}
