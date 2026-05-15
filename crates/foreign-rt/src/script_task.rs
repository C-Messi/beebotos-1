//! Unified script execution request and result types

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::metering::ForeignGasReport;

/// Foreign runtime type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ForeignRuntime {
    /// Python runtime
    #[serde(rename = "python")]
    Python,
    /// Node.js runtime
    #[serde(rename = "nodejs")]
    NodeJs,
}

impl ForeignRuntime {
    /// Get runtime name
    pub fn name(&self) -> &'static str {
        match self {
            ForeignRuntime::Python => "python",
            ForeignRuntime::NodeJs => "nodejs",
        }
    }

    /// Get file extension
    pub fn extension(&self) -> &'static str {
        match self {
            ForeignRuntime::Python => "py",
            ForeignRuntime::NodeJs => "js",
        }
    }
}

impl std::fmt::Display for ForeignRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Script source type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScriptSource {
    /// Inline code string
    Inline {
        /// Source code
        code: String,
    },
    /// File path (relative to workspace)
    File {
        /// File path
        path: PathBuf,
    },
    /// Prebuilt module (WASM or compiled artifact)
    Prebuilt {
        /// Module identifier
        module_id: String,
        /// Optional entry function name
        entrypoint: Option<String>,
    },
}

/// Filesystem path mapping for sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathMapping {
    /// Host path
    pub host_path: PathBuf,
    /// Guest path (inside sandbox)
    pub guest_path: PathBuf,
    /// Read-only
    pub read_only: bool,
}

/// Sandbox requirements for script execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRequirements {
    /// Minimum memory in MB
    #[serde(default = "default_min_memory")]
    pub min_memory_mb: usize,
    /// Maximum memory in MB
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: usize,
    /// Network access allowed
    #[serde(default)]
    pub network_allowed: bool,
    /// Allowed domains (if network allowed)
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// Filesystem path mappings
    #[serde(default)]
    pub filesystem_paths: Vec<PathMapping>,
    /// GPU access allowed
    #[serde(default)]
    pub gpu_allowed: bool,
    /// Maximum CPU time in milliseconds
    #[serde(default = "default_cpu_time_ms")]
    pub max_cpu_time_ms: u64,
    /// Maximum number of PIDs (process path only)
    #[serde(default = "default_max_pids")]
    pub max_pids: u32,
}

fn default_min_memory() -> usize {
    64
}

fn default_max_memory() -> usize {
    256
}

fn default_cpu_time_ms() -> u64 {
    30000
}

fn default_max_pids() -> u32 {
    8
}

impl Default for SandboxRequirements {
    fn default() -> Self {
        Self {
            min_memory_mb: default_min_memory(),
            max_memory_mb: default_max_memory(),
            network_allowed: false,
            allowed_domains: Vec::new(),
            filesystem_paths: Vec::new(),
            gpu_allowed: false,
            max_cpu_time_ms: default_cpu_time_ms(),
            max_pids: default_max_pids(),
        }
    }
}

/// Log entry from script execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level
    pub level: LogLevel,
    /// Log message
    pub message: String,
    /// Optional timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Optional source (e.g., script file:line)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// Debug
    #[serde(rename = "debug")]
    Debug,
    /// Info
    #[serde(rename = "info")]
    Info,
    /// Warning
    #[serde(rename = "warn")]
    Warn,
    /// Error
    #[serde(rename = "error")]
    Error,
}

/// Script artifact (output file or data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptArtifact {
    /// Artifact identifier
    pub id: String,
    /// Artifact name
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// Content bytes (base64 encoded in JSON)
    #[serde(with = "serde_bytes")]
    pub content: Vec<u8>,
}

mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let bytes: Vec<u8> = Vec::deserialize(d)?;
        Ok(bytes)
    }
}

/// Unified script execution request
#[derive(Debug, Clone)]
pub struct ScriptTask {
    /// Task identifier
    pub task_id: String,
    /// Target runtime
    pub runtime: ForeignRuntime,
    /// Script source
    pub source: ScriptSource,
    /// Entrypoint function or file
    pub entrypoint: String,
    /// Input parameters (JSON)
    pub input: serde_json::Value,
    /// Sandbox requirements
    pub sandbox: SandboxRequirements,
    /// Required capability permissions
    pub permissions: Vec<String>,
    /// Execution timeout
    pub timeout: Duration,
    /// Agent ID that initiated the task
    pub agent_id: Option<String>,
}

impl ScriptTask {
    /// Create a new script task builder
    pub fn builder(runtime: ForeignRuntime, task_id: impl Into<String>) -> ScriptTaskBuilder {
        ScriptTaskBuilder::new(runtime, task_id)
    }
}

/// Builder for script tasks
#[derive(Debug, Clone)]
pub struct ScriptTaskBuilder {
    runtime: ForeignRuntime,
    task_id: String,
    source: Option<ScriptSource>,
    entrypoint: String,
    input: serde_json::Value,
    sandbox: SandboxRequirements,
    permissions: Vec<String>,
    timeout: Duration,
    agent_id: Option<String>,
}

impl ScriptTaskBuilder {
    /// Create new builder
    pub fn new(runtime: ForeignRuntime, task_id: impl Into<String>) -> Self {
        Self {
            runtime,
            task_id: task_id.into(),
            source: None,
            entrypoint: "main".to_string(),
            input: serde_json::Value::Null,
            sandbox: SandboxRequirements::default(),
            permissions: Vec::new(),
            timeout: Duration::from_secs(30),
            agent_id: None,
        }
    }

    /// Set source to inline code
    pub fn with_inline_code(mut self, code: impl Into<String>) -> Self {
        self.source = Some(ScriptSource::Inline { code: code.into() });
        self
    }

    /// Set source to file path
    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = Some(ScriptSource::File {
            path: path.into(),
        });
        self
    }

    /// Set entrypoint
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }

    /// Set input
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = input;
        self
    }

    /// Set sandbox requirements
    pub fn with_sandbox(mut self, sandbox: SandboxRequirements) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set agent ID
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// Add permission
    pub fn with_permission(mut self, perm: impl Into<String>) -> Self {
        self.permissions.push(perm.into());
        self
    }

    /// Build the task
    pub fn build(self) -> crate::error::Result<ScriptTask> {
        let source = self.source.ok_or_else(|| {
            crate::error::ForeignRtError::InvalidConfig("Script source is required".to_string())
        })?;

        Ok(ScriptTask {
            task_id: self.task_id,
            runtime: self.runtime,
            source,
            entrypoint: self.entrypoint,
            input: self.input,
            sandbox: self.sandbox,
            permissions: self.permissions,
            timeout: self.timeout,
            agent_id: self.agent_id,
        })
    }
}

/// Unified script execution result
#[derive(Debug, Clone)]
pub struct ScriptResult {
    /// Task identifier
    pub task_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Output value (JSON)
    pub output: serde_json::Value,
    /// Generated artifacts
    pub artifacts: Vec<ScriptArtifact>,
    /// Gas/resource consumption report
    pub gas_report: ForeignGasReport,
    /// Execution logs
    pub logs: Vec<LogEntry>,
    /// Execution duration
    pub execution_time: Duration,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl ScriptResult {
    /// Create a successful result
    pub fn success(
        task_id: impl Into<String>,
        output: serde_json::Value,
        execution_time: Duration,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            success: true,
            output,
            artifacts: Vec::new(),
            gas_report: ForeignGasReport::default(),
            logs: Vec::new(),
            execution_time,
            error: None,
        }
    }

    /// Create a failed result
    pub fn failure(
        task_id: impl Into<String>,
        error: impl Into<String>,
        execution_time: Duration,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            success: false,
            output: serde_json::Value::Null,
            artifacts: Vec::new(),
            gas_report: ForeignGasReport::default(),
            logs: Vec::new(),
            execution_time,
            error: Some(error.into()),
        }
    }

    /// Add artifact
    pub fn with_artifact(mut self, artifact: ScriptArtifact) -> Self {
        self.artifacts.push(artifact);
        self
    }

    /// Add gas report
    pub fn with_gas_report(mut self, report: ForeignGasReport) -> Self {
        self.gas_report = report;
        self
    }

    /// Add logs
    pub fn with_logs(mut self, logs: Vec<LogEntry>) -> Self {
        self.logs = logs;
        self
    }
}

/// Runtime pool statistics
#[derive(Debug, Clone, Default)]
pub struct RuntimePoolStats {
    /// Total WASM instances in pool
    pub wasm_instances_available: usize,
    /// Total WASM instances in use
    pub wasm_instances_in_use: usize,
    /// Total process slots available
    pub process_slots_available: usize,
    /// Total process slots in use
    pub process_slots_in_use: usize,
    /// Total executions processed
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
}
