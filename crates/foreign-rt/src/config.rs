//! Configuration for foreign runtime manager

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Foreign runtime manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignRuntimeConfig {
    /// Whether foreign runtime support is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// WASM path configuration
    #[serde(default)]
    pub wasm: WasmPathConfig,
    /// Process path configuration
    #[serde(default)]
    pub process: ProcessPathConfig,
    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,
}

fn default_true() -> bool {
    true
}

impl Default for ForeignRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            wasm: WasmPathConfig::default(),
            process: ProcessPathConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

/// WASM path configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPathConfig {
    /// Path to Pyodide WASM module
    pub pyodide_module_path: Option<PathBuf>,
    /// Path to Pyodide packages directory
    pub pyodide_packages_dir: Option<PathBuf>,
    /// Path to QuickJS WASM module
    pub quickjs_module_path: Option<PathBuf>,
    /// Maximum WASM memory in MB
    #[serde(default = "default_max_wasm_memory")]
    pub max_memory_mb: usize,
    /// Enable fuel metering
    #[serde(default = "default_true")]
    pub fuel_metering: bool,
    /// Pool configuration
    #[serde(default)]
    pub pool: WasmPoolConfig,
}

fn default_max_wasm_memory() -> usize {
    512
}

impl Default for WasmPathConfig {
    fn default() -> Self {
        Self {
            pyodide_module_path: None,
            pyodide_packages_dir: None,
            quickjs_module_path: None,
            max_memory_mb: default_max_wasm_memory(),
            fuel_metering: true,
            pool: WasmPoolConfig::default(),
        }
    }
}

/// WASM pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPoolConfig {
    /// Number of warmed Pyodide instances
    #[serde(default = "default_pyodide_warm")]
    pub pyodide_warm_instances: usize,
    /// Number of warmed QuickJS instances
    #[serde(default = "default_quickjs_warm")]
    pub quickjs_warm_instances: usize,
    /// Idle timeout before instance is dropped
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
}

fn default_pyodide_warm() -> usize {
    2
}

fn default_quickjs_warm() -> usize {
    4
}

fn default_idle_timeout() -> u64 {
    60
}

impl Default for WasmPoolConfig {
    fn default() -> Self {
        Self {
            pyodide_warm_instances: default_pyodide_warm(),
            quickjs_warm_instances: default_quickjs_warm(),
            idle_timeout_secs: default_idle_timeout(),
        }
    }
}

/// Process path configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPathConfig {
    /// Python rootfs path (extracted Docker image)
    pub python_rootfs: Option<PathBuf>,
    /// Node.js rootfs path
    pub nodejs_rootfs: Option<PathBuf>,
    /// nsjail configuration template path
    pub nsjail_config_template: Option<PathBuf>,
    /// Maximum concurrent process slots
    #[serde(default = "default_max_process_slots")]
    pub max_process_slots: usize,
    /// Cgroup configuration
    #[serde(default)]
    pub cgroup: CgroupConfig,
}

fn default_max_process_slots() -> usize {
    10
}

impl Default for ProcessPathConfig {
    fn default() -> Self {
        Self {
            python_rootfs: None,
            nodejs_rootfs: None,
            nsjail_config_template: None,
            max_process_slots: default_max_process_slots(),
            cgroup: CgroupConfig::default(),
        }
    }
}

/// Cgroup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupConfig {
    /// Parent cgroup path
    #[serde(default = "default_cgroup_parent")]
    pub parent_cgroup: String,
    /// Memory high watermark in MB (throttling starts)
    #[serde(default = "default_memory_high")]
    pub memory_high_mb: usize,
    /// Maximum swap in MB (0 = disabled)
    #[serde(default)]
    pub swap_max_mb: usize,
}

fn default_cgroup_parent() -> String {
    "beebotos/foreign_rt".to_string()
}

fn default_memory_high() -> usize {
    4096
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            parent_cgroup: default_cgroup_parent(),
            memory_high_mb: default_memory_high(),
            swap_max_mb: 0,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Seccomp policy level
    #[serde(default = "default_seccomp_policy")]
    pub seccomp_policy: SeccompPolicy,
    /// Audit level
    #[serde(default = "default_audit_level")]
    pub audit_level: AuditLevel,
    /// Output DLP scanning enabled
    #[serde(default = "default_true")]
    pub output_dlp_scan: bool,
}

/// Seccomp policy levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeccompPolicy {
    /// Highly restrictive - minimal syscalls
    #[serde(rename = "restrictive")]
    Restrictive,
    /// Standard - common safe syscalls
    #[serde(rename = "standard")]
    Standard,
    /// Permissive - for debugging only
    #[serde(rename = "permissive")]
    Permissive,
}

fn default_seccomp_policy() -> SeccompPolicy {
    SeccompPolicy::Restrictive
}

/// Audit level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditLevel {
    /// No auditing
    #[serde(rename = "none")]
    None,
    /// Syscall-level audit
    #[serde(rename = "syscall")]
    Syscall,
    /// Full audit (syscalls + file access + network)
    #[serde(rename = "full")]
    Full,
}

fn default_audit_level() -> AuditLevel {
    AuditLevel::Full
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            seccomp_policy: default_seccomp_policy(),
            audit_level: default_audit_level(),
            output_dlp_scan: true,
        }
    }
}

/// Execution route determined by the router
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRoute {
    /// WASM path via Pyodide
    WasmPyodide,
    /// WASM path via QuickJS
    WasmQuickJS,
    /// Process path for Python
    ProcessPython,
    /// Process path for Node.js
    ProcessNodeJs,
}

impl ExecutionRoute {
    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            ExecutionRoute::WasmPyodide => "wasm_pyodide",
            ExecutionRoute::WasmQuickJS => "wasm_quickjs",
            ExecutionRoute::ProcessPython => "process_python",
            ExecutionRoute::ProcessNodeJs => "process_nodejs",
        }
    }

    /// Check if this is a WASM route
    pub fn is_wasm(&self) -> bool {
        matches!(
            self,
            ExecutionRoute::WasmPyodide | ExecutionRoute::WasmQuickJS
        )
    }

    /// Check if this is a process route
    pub fn is_process(&self) -> bool {
        matches!(
            self,
            ExecutionRoute::ProcessPython | ExecutionRoute::ProcessNodeJs
        )
    }
}

/// Routing hints provided by the caller or skill metadata
#[derive(Debug, Clone, Default)]
pub struct RouteHints {
    /// Prefer WASM path if available
    pub prefer_wasm: bool,
    /// Force process path
    pub force_process: bool,
    /// Requires GPU
    pub requires_gpu: bool,
    /// Requires specific filesystem paths
    pub requires_paths: Vec<PathBuf>,
    /// Minimum memory required in MB
    pub min_memory_mb: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ForeignRuntimeConfig::default();
        assert!(config.enabled);
        assert_eq!(config.wasm.max_memory_mb, 512);
        assert!(config.wasm.fuel_metering);
        assert_eq!(config.wasm.pool.pyodide_warm_instances, 2);
        assert_eq!(config.wasm.pool.quickjs_warm_instances, 4);
        assert_eq!(config.process.max_process_slots, 10);
        assert_eq!(config.security.seccomp_policy, SeccompPolicy::Restrictive);
    }

    #[test]
    fn test_execution_route() {
        assert!(ExecutionRoute::WasmPyodide.is_wasm());
        assert!(!ExecutionRoute::ProcessPython.is_wasm());
        assert!(ExecutionRoute::ProcessNodeJs.is_process());
    }
}
