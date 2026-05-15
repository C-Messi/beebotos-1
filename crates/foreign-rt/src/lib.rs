//! BeeBotOS Foreign Runtime
//!
//! Provides Python and Node.js execution support within the BeeBotOS kernel
//! and agent runtime. Implements a dual-path execution model:
//!
//! - **WASM Path**: Pyodide (Python) and QuickJS (Node.js) executed within
//!   wasmtime sandbox, leveraging existing Kernel WASM infrastructure.
//! - **Process Path**: CPython and Node.js executed in isolated processes
//!   using Linux namespaces, seccomp-bpf, and cgroup v2.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                        Foreign Runtime Manager                               │
//! │                                                                               │
//! │  ┌─────────────────┐     ┌─────────────────┐     ┌───────────────────────┐  │
//! │  │  RuntimeRouter  │────▶│  RuntimePool    │────▶│  Wasm/Process Exec    │  │
//! │  │  (path select)  │     │  (warm pool)    │     │  (execute task)       │  │
//! │  └─────────────────┘     └─────────────────┘     └───────────────────────┘  │
//! │                                                                               │
//! │  ┌─────────────────────────────────────────────────────────────────────────┐│
//! │  │                    Host Function Bridge                                  ││
//! │  │  storage::get/put · ipc::send · llm::chat · chain::call · log::write   ││
//! │  └─────────────────────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

#![warn(missing_docs)]

pub mod bridge;
pub mod config;
pub mod error;
pub mod metering;
pub mod pool;
pub mod process_path;
pub mod router;
pub mod script_task;
pub mod wasm_path;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tracing::{error, info, instrument, warn};

pub use config::{
    AuditLevel, CgroupConfig, ExecutionRoute, ForeignRuntimeConfig, ProcessPathConfig,
    RouteHints, SeccompPolicy, SecurityConfig, WasmPathConfig, WasmPoolConfig,
};
pub use error::{ForeignRtError, Result};
pub use metering::{ForeignGasReport, GasLimit, GasOracle, StandardGasOracle};
pub use pool::{ObjectPool, PooledInstance, ProcessSlotToken, RuntimePool};
pub use router::RuntimeRouter;
pub use script_task::{
    ForeignRuntime, LogEntry, LogLevel, PathMapping, RuntimePoolStats, ScriptArtifact, ScriptResult,
    ScriptSource, ScriptTask, ScriptTaskBuilder, SandboxRequirements,
};

use crate::process_path::ProcessSandboxExecutor;
use crate::wasm_path::{pyodide::PyodideExecutor, quickjs::QuickJsExecutor, WasmRuntimeEngine, WasmRuntimeExecutor};

/// Foreign Runtime Manager trait
///
/// The main entry point for executing foreign runtime tasks.
#[async_trait]
pub trait ForeignRuntimeManager: Send + Sync {
    /// Execute a script task
    async fn execute(&self, task: ScriptTask) -> Result<ScriptResult>;

    /// Pre-warm runtime instances
    async fn prewarm(&self, runtime: ForeignRuntime) -> Result<()>;

    /// Get runtime pool statistics
    fn stats(&self) -> RuntimePoolStats;

    /// Check if a runtime is available
    fn is_available(&self, runtime: ForeignRuntime) -> bool;
}

/// Default foreign runtime manager implementation
pub struct DefaultForeignRuntimeManager {
    /// Configuration
    config: ForeignRuntimeConfig,
    /// WASM runtime engine
    wasm_engine: Option<Arc<WasmRuntimeEngine>>,
    /// Pyodide executor
    pyodide: Option<PyodideExecutor>,
    /// QuickJS executor
    quickjs: Option<QuickJsExecutor>,
    /// Process sandbox executor
    process_executor: Option<ProcessSandboxExecutor>,
    /// Runtime router
    router: RuntimeRouter,
    /// Runtime pool
    pool: RuntimePool,
    /// Gas limit (optional)
    gas_limit: Option<GasLimit>,
}

impl DefaultForeignRuntimeManager {
    /// Create a new foreign runtime manager from configuration
    pub fn new(config: ForeignRuntimeConfig) -> Result<Self> {
        if !config.enabled {
            return Ok(Self {
                config: config.clone(),
                wasm_engine: None,
                pyodide: None,
                quickjs: None,
                process_executor: None,
                router: RuntimeRouter::new(config.clone()),
                pool: RuntimePool::new(config.wasm.pool.clone()),
                gas_limit: None,
            });
        }

        // Initialize WASM engine
        let wasm_engine = if config.wasm.pyodide_module_path.is_some()
            || config.wasm.quickjs_module_path.is_some()
        {
            match WasmRuntimeEngine::new(config.wasm.clone()) {
                Ok(engine) => {
                    info!("WASM engine initialized for foreign runtime");
                    Some(Arc::new(engine))
                }
                Err(e) => {
                    warn!("Failed to initialize WASM engine: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Initialize WASM executors
        let pyodide = wasm_engine.as_ref().and_then(|engine| {
            match PyodideExecutor::new(engine.clone(), config.wasm.clone()) {
                Ok(executor) if executor.is_available() => {
                    info!("Pyodide executor initialized");
                    Some(executor)
                }
                Ok(_) => {
                    warn!("Pyodide module not available");
                    None
                }
                Err(e) => {
                    warn!("Failed to initialize Pyodide executor: {}", e);
                    None
                }
            }
        });

        let quickjs = wasm_engine.as_ref().and_then(|engine| {
            match QuickJsExecutor::new(engine.clone(), config.wasm.clone()) {
                Ok(executor) if executor.is_available() => {
                    info!("QuickJS executor initialized");
                    Some(executor)
                }
                Ok(_) => {
                    warn!("QuickJS module not available");
                    None
                }
                Err(e) => {
                    warn!("Failed to initialize QuickJS executor: {}", e);
                    None
                }
            }
        });

        // Initialize process executor
        let process_executor = if config.process.python_rootfs.is_some()
            || config.process.nodejs_rootfs.is_some()
        {
            let executor = ProcessSandboxExecutor::new(
                config.process.clone(),
                config.security.clone(),
            );
            info!("Process sandbox executor initialized");
            Some(executor)
        } else {
            warn!("No process rootfs configured, process path unavailable");
            None
        };

        let router = RuntimeRouter::new(config.clone());
        let pool = RuntimePool::new(config.wasm.pool.clone());

        Ok(Self {
            config,
            wasm_engine,
            pyodide,
            quickjs,
            process_executor,
            router,
            pool,
            gas_limit: None,
        })
    }

    /// Set gas limit for all executions
    pub fn with_gas_limit(mut self, limit: GasLimit) -> Self {
        self.gas_limit = Some(limit);
        self
    }

    /// Validate that the given runtime can be executed
    pub fn validate_runtime(&self, runtime: ForeignRuntime) -> Result<()> {
        self.router.validate_runtime(runtime)
    }
}

#[async_trait]
impl ForeignRuntimeManager for DefaultForeignRuntimeManager {
    #[instrument(skip(self, task), fields(task_id = %task.task_id, runtime = %task.runtime))]
    async fn execute(&self, task: ScriptTask) -> Result<ScriptResult> {
        if !self.config.enabled {
            return Err(ForeignRtError::RuntimeNotAvailable(
                "Foreign runtime is disabled".to_string(),
            ));
        }

        // Select execution route
        let hints = RouteHints::default();
        let route = self.router.select(
            task.runtime,
            &task.source,
            &task.sandbox,
            &hints,
        )?;

        info!(route = route.name(), "Selected execution route");

        // Record execution start metric
        let runtime_label = task.runtime.name();
        let route_label = route.name();
        metrics::counter!(
            "beebotos_foreign_rt_executions_total",
            "runtime" => runtime_label,
            "path" => route_label,
            "status" => "started"
        )
        .increment(1);

        let exec_start = std::time::Instant::now();

        // Execute based on route
        let result = match route {
            ExecutionRoute::WasmPyodide => {
                if let Some(ref executor) = self.pyodide {
                    executor.execute(&task).await
                } else {
                    Err(ForeignRtError::RuntimeNotAvailable(
                        "Pyodide executor not available".to_string(),
                    ))
                }
            }
            ExecutionRoute::WasmQuickJS => {
                if let Some(ref executor) = self.quickjs {
                    executor.execute(&task).await
                } else {
                    Err(ForeignRtError::RuntimeNotAvailable(
                        "QuickJS executor not available".to_string(),
                    ))
                }
            }
            ExecutionRoute::ProcessPython | ExecutionRoute::ProcessNodeJs => {
                if let Some(ref executor) = self.process_executor {
                    // Acquire process slot
                    let _permit = self
                        .pool
                        .acquire_process_slot(task.runtime)
                        .await
                        .map_err(|e| {
                            error!("Failed to acquire process slot: {}", e);
                            e
                        })?;

                    executor.execute(&task).await
                } else {
                    Err(ForeignRtError::RuntimeNotAvailable(
                        "Process sandbox executor not available".to_string(),
                    ))
                }
            }
        };

        let exec_duration = exec_start.elapsed().as_secs_f64();
        let status_label = match &result {
            Ok(ref r) if r.success => "success",
            Ok(_) => "failure",
            Err(_) => "error",
        };

        // Record execution completion metrics
        metrics::counter!(
            "beebotos_foreign_rt_executions_total",
            "runtime" => runtime_label,
            "path" => route_label,
            "status" => status_label
        )
        .increment(1);
        metrics::histogram!(
            "beebotos_foreign_rt_execution_duration_seconds",
            "runtime" => runtime_label,
            "path" => route_label
        )
        .record(exec_duration);

        if let Ok(ref r) = result {
            metrics::counter!(
                "beebotos_foreign_rt_gas_used_total",
                "runtime" => runtime_label,
                "path" => route_label,
                "resource" => "compute"
            )
            .increment(r.gas_report.compute_gas);
            metrics::counter!(
                "beebotos_foreign_rt_gas_used_total",
                "runtime" => runtime_label,
                "path" => route_label,
                "resource" => "memory"
            )
            .increment(r.gas_report.memory_gas);
        }

        // Update stats
        match &result {
            Ok(ref r) if r.success => self.pool.record_success(),
            _ => self.pool.record_failure(),
        }

        // Check gas limits
        if let (Some(ref limit), Ok(ref result)) = (self.gas_limit, &result) {
            if let Err(e) = limit.check(&result.gas_report) {
                warn!(task_id = %task.task_id, "Gas limit exceeded: {}", e);
                return Err(e);
            }
        }

        result
    }

    async fn prewarm(&self, runtime: ForeignRuntime) -> Result<()> {
        info!(runtime = runtime.name(), "Pre-warming runtime");

        match runtime {
            ForeignRuntime::Python => {
                if let Some(ref executor) = self.pyodide {
                    executor.prewarm(self.config.wasm.pool.pyodide_warm_instances).await?;
                }
            }
            ForeignRuntime::NodeJs => {
                if let Some(ref executor) = self.quickjs {
                    executor.prewarm(self.config.wasm.pool.quickjs_warm_instances).await?;
                }
            }
        }

        Ok(())
    }

    fn stats(&self) -> RuntimePoolStats {
        self.pool.stats()
    }

    fn is_available(&self, runtime: ForeignRuntime) -> bool {
        match runtime {
            ForeignRuntime::Python => {
                self.pyodide.is_some() || self.process_executor.as_ref().map_or(false, |e| e.is_available(ForeignRuntime::Python))
            }
            ForeignRuntime::NodeJs => {
                self.quickjs.is_some() || self.process_executor.as_ref().map_or(false, |e| e.is_available(ForeignRuntime::NodeJs))
            }
        }
    }
}

/// Builder for foreign runtime manager
pub struct ForeignRuntimeManagerBuilder {
    config: ForeignRuntimeConfig,
    gas_limit: Option<GasLimit>,
}

impl ForeignRuntimeManagerBuilder {
    /// Create a new builder with default config
    pub fn new() -> Self {
        Self {
            config: ForeignRuntimeConfig::default(),
            gas_limit: None,
        }
    }

    /// Create a new builder with config
    pub fn with_config(config: ForeignRuntimeConfig) -> Self {
        Self {
            config,
            gas_limit: None,
        }
    }

    /// Set WASM path config
    pub fn with_wasm_config(mut self, config: WasmPathConfig) -> Self {
        self.config.wasm = config;
        self
    }

    /// Set process path config
    pub fn with_process_config(mut self, config: ProcessPathConfig) -> Self {
        self.config.process = config;
        self
    }

    /// Set security config
    pub fn with_security_config(mut self, config: SecurityConfig) -> Self {
        self.config.security = config;
        self
    }

    /// Set gas limit
    pub fn with_gas_limit(mut self, limit: GasLimit) -> Self {
        self.gas_limit = Some(limit);
        self
    }

    /// Build the manager
    pub fn build(self) -> Result<DefaultForeignRuntimeManager> {
        let mut manager = DefaultForeignRuntimeManager::new(self.config)?;
        if let Some(limit) = self.gas_limit {
            manager.gas_limit = Some(limit);
        }
        Ok(manager)
    }
}

impl Default for ForeignRuntimeManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = ForeignRuntimeManagerBuilder::new();
        assert!(builder.config.enabled);
    }

    #[test]
    fn test_manager_disabled() {
        let mut config = ForeignRuntimeConfig::default();
        config.enabled = false;

        let manager = DefaultForeignRuntimeManager::new(config).unwrap();
        assert!(!manager.is_available(ForeignRuntime::Python));
        assert!(!manager.is_available(ForeignRuntime::NodeJs));
    }

    #[tokio::test]
    async fn test_execute_disabled() {
        let mut config = ForeignRuntimeConfig::default();
        config.enabled = false;

        let manager = DefaultForeignRuntimeManager::new(config).unwrap();
        let task = ScriptTask {
            task_id: "test".to_string(),
            runtime: ForeignRuntime::Python,
            source: ScriptSource::Inline { code: "1+1".to_string() },
            entrypoint: "main".to_string(),
            input: serde_json::Value::Null,
            sandbox: SandboxRequirements::default(),
            permissions: vec![],
            timeout: Duration::from_secs(5),
            agent_id: None,
        };

        let result = manager.execute(task).await;
        assert!(result.is_err());
    }
}
