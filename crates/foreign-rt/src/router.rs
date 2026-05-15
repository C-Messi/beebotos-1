//! Runtime router - selects optimal execution path (WASM vs Process)

use tracing::{debug, info, warn};

use crate::config::{ExecutionRoute, ForeignRuntimeConfig, RouteHints};
use crate::error::{ForeignRtError, Result};
use crate::script_task::{ForeignRuntime, SandboxRequirements, ScriptSource};

/// Runtime router that determines the best execution path
pub struct RuntimeRouter {
    config: ForeignRuntimeConfig,
}

impl RuntimeRouter {
    /// Create a new runtime router
    pub fn new(config: ForeignRuntimeConfig) -> Self {
        Self { config }
    }

    /// Select the best execution route for a given task
    ///
    /// Routing logic:
    /// 1. If force_process hint is set -> process path
    /// 2. If requires GPU -> process path (WASM doesn't support GPU)
    /// 3. If memory > wasm max_memory_mb -> process path
    /// 4. If source is prebuilt WASM module -> WASM path
    /// 5. If runtime is Python and Pyodide is available -> WASM path (default)
    /// 6. If runtime is Node.js and QuickJS is available -> WASM path (default)
    /// 7. Fallback to process path if WASM not configured
    pub fn select(
        &self,
        runtime: ForeignRuntime,
        source: &ScriptSource,
        sandbox: &SandboxRequirements,
        hints: &RouteHints,
    ) -> Result<ExecutionRoute> {
        debug!(
            runtime = runtime.name(),
            "Selecting execution route for foreign runtime task"
        );

        // Rule 1: Force process path
        if hints.force_process {
            info!(runtime = runtime.name(), "Forcing process path per hints");
            return Ok(process_route(runtime));
        }

        // Rule 2: GPU requirements
        if sandbox.gpu_allowed || hints.requires_gpu {
            info!(runtime = runtime.name(), "Selecting process path for GPU access");
            return Ok(process_route(runtime));
        }

        // Rule 3: Memory requirements exceed WASM limit
        if sandbox.max_memory_mb > self.config.wasm.max_memory_mb {
            info!(
                runtime = runtime.name(),
                required_mb = sandbox.max_memory_mb,
                wasm_max_mb = self.config.wasm.max_memory_mb,
                "Selecting process path due to memory requirements"
            );
            return Ok(process_route(runtime));
        }

        // Rule 4: Prebuilt WASM modules always go WASM path
        if matches!(source, ScriptSource::Prebuilt { .. }) {
            info!(runtime = runtime.name(), "Selecting WASM path for prebuilt module");
            return Ok(wasm_route(runtime));
        }

        // Rule 5-6: Default to WASM if available and preferred
        if !hints.prefer_wasm {
            // Check if process path might be better for heavy tasks
            if sandbox.max_memory_mb > 256 || sandbox.max_cpu_time_ms > 120000 {
                info!(
                    runtime = runtime.name(),
                    "Selecting process path for resource-intensive task"
                );
                return Ok(process_route(runtime));
            }
        }

        // Default: WASM path for safety
        let route = wasm_route(runtime);

        // Verify WASM path is actually available
        if route.is_wasm() && !self.is_wasm_available(runtime) {
            warn!(
                runtime = runtime.name(),
                "WASM path not available, falling back to process path"
            );
            return Ok(process_route(runtime));
        }

        info!(runtime = runtime.name(), route = route.name(), "Selected execution route");
        Ok(route)
    }

    /// Check if WASM path is available for a runtime
    fn is_wasm_available(&self, runtime: ForeignRuntime) -> bool {
        match runtime {
            ForeignRuntime::Python => self.config.wasm.pyodide_module_path.is_some(),
            ForeignRuntime::NodeJs => self.config.wasm.quickjs_module_path.is_some(),
        }
    }

    /// Check if process path is available for a runtime
    pub fn is_process_available(&self, runtime: ForeignRuntime) -> bool {
        match runtime {
            ForeignRuntime::Python => self.config.process.python_rootfs.is_some(),
            ForeignRuntime::NodeJs => self.config.process.nodejs_rootfs.is_some(),
        }
    }

    /// Validate that at least one path is available for the runtime
    pub fn validate_runtime(&self, runtime: ForeignRuntime) -> Result<()> {
        if self.is_wasm_available(runtime) || self.is_process_available(runtime) {
            Ok(())
        } else {
            Err(ForeignRtError::RuntimeNotAvailable(format!(
                "No execution path available for {}. Configure wasm module or process rootfs.",
                runtime.name()
            )))
        }
    }
}

/// Get WASM route for a runtime
fn wasm_route(runtime: ForeignRuntime) -> ExecutionRoute {
    match runtime {
        ForeignRuntime::Python => ExecutionRoute::WasmPyodide,
        ForeignRuntime::NodeJs => ExecutionRoute::WasmQuickJS,
    }
}

/// Get process route for a runtime
fn process_route(runtime: ForeignRuntime) -> ExecutionRoute {
    match runtime {
        ForeignRuntime::Python => ExecutionRoute::ProcessPython,
        ForeignRuntime::NodeJs => ExecutionRoute::ProcessNodeJs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProcessPathConfig, WasmPathConfig};

    fn test_config() -> ForeignRuntimeConfig {
        ForeignRuntimeConfig {
            enabled: true,
            wasm: WasmPathConfig {
                pyodide_module_path: Some("/opt/pyodide/pyodide.asm.wasm".into()),
                quickjs_module_path: Some("/opt/quickjs/qjs.wasm".into()),
                max_memory_mb: 512,
                ..Default::default()
            },
            process: ProcessPathConfig {
                python_rootfs: Some("/var/lib/beebotos/rootfs/python".into()),
                nodejs_rootfs: Some("/var/lib/beebotos/rootfs/nodejs".into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_select_wasm_default() {
        let router = RuntimeRouter::new(test_config());
        let sandbox = SandboxRequirements::default();
        let hints = RouteHints::default();

        let route = router.select(
            ForeignRuntime::Python,
            &ScriptSource::Inline { code: "1+1".to_string() },
            &sandbox,
            &hints,
        ).unwrap();

        assert_eq!(route, ExecutionRoute::WasmPyodide);
    }

    #[test]
    fn test_select_process_for_gpu() {
        let router = RuntimeRouter::new(test_config());
        let mut sandbox = SandboxRequirements::default();
        sandbox.gpu_allowed = true;
        let hints = RouteHints::default();

        let route = router.select(
            ForeignRuntime::Python,
            &ScriptSource::Inline { code: "".to_string() },
            &sandbox,
            &hints,
        ).unwrap();

        assert_eq!(route, ExecutionRoute::ProcessPython);
    }

    #[test]
    fn test_select_process_for_large_memory() {
        let router = RuntimeRouter::new(test_config());
        let mut sandbox = SandboxRequirements::default();
        sandbox.max_memory_mb = 1024; // > 512 wasm max
        let hints = RouteHints::default();

        let route = router.select(
            ForeignRuntime::NodeJs,
            &ScriptSource::Inline { code: "".to_string() },
            &sandbox,
            &hints,
        ).unwrap();

        assert_eq!(route, ExecutionRoute::ProcessNodeJs);
    }

    #[test]
    fn test_force_process_hint() {
        let router = RuntimeRouter::new(test_config());
        let sandbox = SandboxRequirements::default();
        let hints = RouteHints {
            force_process: true,
            ..Default::default()
        };

        let route = router.select(
            ForeignRuntime::Python,
            &ScriptSource::Inline { code: "".to_string() },
            &sandbox,
            &hints,
        ).unwrap();

        assert_eq!(route, ExecutionRoute::ProcessPython);
    }

    #[test]
    fn test_fallback_when_wasm_unavailable() {
        let mut config = test_config();
        config.wasm.pyodide_module_path = None;
        let router = RuntimeRouter::new(config);
        let sandbox = SandboxRequirements::default();
        let hints = RouteHints::default();

        let route = router.select(
            ForeignRuntime::Python,
            &ScriptSource::Inline { code: "".to_string() },
            &sandbox,
            &hints,
        ).unwrap();

        assert_eq!(route, ExecutionRoute::ProcessPython);
    }
}
