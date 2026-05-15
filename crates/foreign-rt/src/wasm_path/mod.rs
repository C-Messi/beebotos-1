//! WASM execution path for foreign runtimes
//!
//! This module handles execution of Python (via Pyodide) and Node.js
//! (via QuickJS) within the wasmtime sandbox.

pub mod executor;
pub mod pyodide;
pub mod quickjs;

use std::sync::Arc;
use std::time::Duration;

use tracing::info;


use crate::config::WasmPathConfig;
use crate::error::{ForeignRtError, Result};
use crate::metering::{ForeignGasReport, GasOracle, StandardGasOracle};
use crate::script_task::{ForeignRuntime, LogEntry, LogLevel, ScriptResult, ScriptTask};

/// WASM runtime engine wrapper
pub struct WasmRuntimeEngine {
    /// wasmtime engine
    engine: wasmtime::Engine,
    /// Engine configuration
    config: WasmPathConfig,
    /// Gas oracle
    gas_oracle: Arc<dyn GasOracle>,
}

impl WasmRuntimeEngine {
    /// Create a new WASM runtime engine
    pub fn new(config: WasmPathConfig) -> Result<Self> {
        let mut engine_config = wasmtime::Config::new();
        engine_config.consume_fuel(config.fuel_metering);
        engine_config.memory_reservation(config.max_memory_mb as u64 * 1024 * 1024);
        engine_config.memory_guard_size(65536);
        engine_config.parallel_compilation(true);
        engine_config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        engine_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Environment);

        let engine = wasmtime::Engine::new(&engine_config)
            .map_err(|e| ForeignRtError::WasmRuntime(format!("Failed to create engine: {}", e)))?;

        info!("WASM runtime engine initialized");

        Ok(Self {
            engine,
            config,
            gas_oracle: Arc::new(StandardGasOracle::new()),
        })
    }

    /// Get engine reference
    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }

    /// Create a new store with host context
    pub fn create_store<T: Send>(&self, data: T) -> wasmtime::Store<T> {
        wasmtime::Store::new(&self.engine, data)
    }

    /// Compile a WASM module
    pub fn compile(&self, bytes: &[u8]) -> Result<wasmtime::Module> {
        wasmtime::Module::new(&self.engine, bytes)
            .map_err(|e| ForeignRtError::CompilationFailed(format!("WASM compilation failed: {}", e)))
    }

    /// Get gas oracle
    pub fn gas_oracle(&self) -> &dyn GasOracle {
        self.gas_oracle.as_ref()
    }
}

/// Base trait for WASM-based foreign runtime executors
#[async_trait::async_trait]
pub trait WasmRuntimeExecutor: Send + Sync {
    /// Execute a script task in the WASM runtime
    async fn execute(&self, task: &ScriptTask) -> Result<ScriptResult>;

    /// Get the runtime type this executor handles
    fn runtime_type(&self) -> ForeignRuntime;

    /// Pre-warm the runtime with instances
    async fn prewarm(&self, count: usize) -> Result<()>;
}

/// Common utilities for WASM executors
pub struct WasmExecutorUtils;

impl WasmExecutorUtils {
    /// Parse JSON output from script stdout
    pub fn parse_output(output: &str) -> Result<serde_json::Value> {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return Ok(serde_json::Value::Null);
        }

        // Try to parse as JSON
        match serde_json::from_str(trimmed) {
            Ok(val) => Ok(val),
            Err(_) => {
                // If not valid JSON, return as string
                Ok(serde_json::Value::String(trimmed.to_string()))
            }
        }
    }

    /// Parse logs from script stderr
    pub fn parse_logs(stderr: &str) -> Vec<LogEntry> {
        stderr
            .lines()
            .map(|line| LogEntry {
                level: Self::detect_log_level(line),
                message: line.to_string(),
                timestamp: Some(chrono::Utc::now()),
                source: None,
            })
            .collect()
    }

    /// Detect log level from line content
    fn detect_log_level(line: &str) -> LogLevel {
        let lower = line.to_lowercase();
        if lower.contains("error") || lower.contains("[e]") {
            LogLevel::Error
        } else if lower.contains("warn") || lower.contains("[w]") {
            LogLevel::Warn
        } else if lower.contains("debug") || lower.contains("[d]") {
            LogLevel::Debug
        } else {
            LogLevel::Info
        }
    }

    /// Build a successful result
    pub fn build_success(
        task: &ScriptTask,
        output: serde_json::Value,
        logs: Vec<LogEntry>,
        execution_time: Duration,
        fuel_consumed: u64,
    ) -> ScriptResult {
        let mut gas_report = ForeignGasReport::new();
        gas_report.add_compute(fuel_consumed);

        ScriptResult::success(&task.task_id, output, execution_time)
            .with_logs(logs)
            .with_gas_report(gas_report)
    }

    /// Build a failure result
    pub fn build_failure(
        task: &ScriptTask,
        error: impl Into<String>,
        logs: Vec<LogEntry>,
        execution_time: Duration,
    ) -> ScriptResult {
        ScriptResult::failure(&task.task_id, error, execution_time).with_logs(logs)
    }
}

/// Create a wasmtime module cache key
pub fn module_cache_key(runtime: ForeignRuntime, module_hash: &str) -> String {
    format!("{}:{}", runtime.name(), module_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_json() {
        let output = r#"{"result": 42, "status": "ok"}"#;
        let parsed = WasmExecutorUtils::parse_output(output).unwrap();
        assert_eq!(parsed["result"], 42);
    }

    #[test]
    fn test_parse_output_string() {
        let output = "Hello, World!";
        let parsed = WasmExecutorUtils::parse_output(output).unwrap();
        assert_eq!(parsed, serde_json::Value::String("Hello, World!".to_string()));
    }

    #[test]
    fn test_detect_log_level() {
        assert!(matches!(
            WasmExecutorUtils::detect_log_level("ERROR: something failed"),
            LogLevel::Error
        ));
        assert!(matches!(
            WasmExecutorUtils::detect_log_level("[W] warning message"),
            LogLevel::Warn
        ));
        assert!(matches!(
            WasmExecutorUtils::detect_log_level("info: all good"),
            LogLevel::Info
        ));
    }
}
