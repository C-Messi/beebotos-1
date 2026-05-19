//! Foreign Runtime HTTP Handlers
//!
//! Provides REST API endpoints for executing Python and Node.js scripts
//! within the BeeBotOS foreign runtime sandbox.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use beebotos_foreign_rt::ForeignRuntimeManager;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::GatewayError;
use crate::AppState;

/// Script source type
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptSourceType {
    /// Inline code string
    Inline,
    /// Prebuilt module reference
    Prebuilt,
    /// File path reference
    File,
}

/// Script source specification
#[derive(Debug, Deserialize)]
pub struct ScriptSourceRequest {
    /// Source type
    #[serde(rename = "type")]
    pub source_type: ScriptSourceType,
    /// Code content (for inline) or path/module ID (for file/prebuilt)
    pub content: String,
}

/// Sandbox configuration request
#[derive(Debug, Deserialize, Default)]
pub struct SandboxConfigRequest {
    /// Maximum memory in MB
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: usize,
    /// Whether network access is allowed
    #[serde(default)]
    pub network_allowed: bool,
    /// Allowed domains (if network_allowed)
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// Whether GPU access is allowed
    #[serde(default)]
    pub gpu_allowed: bool,
    /// Filesystem path mappings
    #[serde(default)]
    pub filesystem_paths: Vec<PathMappingRequest>,
}

fn default_max_memory() -> usize {
    256
}

/// Filesystem path mapping request
#[derive(Debug, Deserialize)]
pub struct PathMappingRequest {
    /// Host path
    pub host_path: String,
    /// Guest path
    pub guest_path: String,
    /// Read-only access
    #[serde(default)]
    pub read_only: bool,
}

/// Execute script request
#[derive(Debug, Deserialize)]
pub struct ExecuteScriptRequest {
    /// Target runtime: "python" or "nodejs"
    pub runtime: String,
    /// Script source
    pub source: ScriptSourceRequest,
    /// Entrypoint function name (optional, defaults to "main")
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    /// Input parameters as JSON
    #[serde(default)]
    pub input: serde_json::Value,
    /// Sandbox configuration
    #[serde(default)]
    pub sandbox: SandboxConfigRequest,
    /// Execution timeout in seconds (default: 30)
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Target agent ID (optional, for capability checking)
    pub agent_id: Option<String>,
    /// Execution route hint: "auto", "wasm", "process"
    #[serde(default = "default_route_hint")]
    pub route_hint: String,
}

fn default_entrypoint() -> String {
    "main".to_string()
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_route_hint() -> String {
    "auto".to_string()
}

/// Log entry in response
#[derive(Debug, Serialize)]
pub struct LogEntryResponse {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

/// Gas usage report
#[derive(Debug, Serialize)]
pub struct GasReportResponse {
    pub compute: u64,
    pub memory: u64,
    pub io: u64,
    pub network: u64,
    pub storage: u64,
    pub total: u64,
}

/// Execute script response
#[derive(Debug, Serialize)]
pub struct ExecuteScriptResponse {
    pub success: bool,
    pub output: serde_json::Value,
    pub execution_time_ms: u64,
    pub gas_used: GasReportResponse,
    pub logs: Vec<LogEntryResponse>,
    pub execution_route: String,
}

/// List available runtimes response
#[derive(Debug, Serialize)]
pub struct ListRuntimesResponse {
    pub runtimes: Vec<RuntimeInfoResponse>,
}

/// Runtime info
#[derive(Debug, Serialize)]
pub struct RuntimeInfoResponse {
    pub name: String,
    pub available: bool,
    pub wasm_available: bool,
    pub process_available: bool,
    pub default_max_memory_mb: usize,
    pub default_timeout_secs: u64,
}

/// Execute a foreign runtime script
pub async fn execute_script(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ExecuteScriptRequest>,
) -> Result<Json<ExecuteScriptResponse>, GatewayError> {
    info!(
        runtime = %req.runtime,
        source_type = ?req.source.source_type,
        "Executing foreign runtime script"
    );

    // Get foreign runtime manager
    let manager = state.foreign_rt_manager.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable(
            "foreign_runtime",
            "Foreign runtime manager not initialized",
        )
    })?;

    // Parse runtime type
    let runtime = match req.runtime.as_str() {
        "python" => beebotos_foreign_rt::ForeignRuntime::Python,
        "nodejs" | "node" => beebotos_foreign_rt::ForeignRuntime::NodeJs,
        other => {
            return Err(GatewayError::validation(vec![
                gateway::error::ValidationError {
                    field: "runtime".to_string(),
                    message: format!("Unsupported runtime: {}. Use 'python' or 'nodejs'", other),
                    code: "invalid_runtime".to_string(),
                },
            ]));
        }
    };

    // Check runtime availability
    if !manager.is_available(runtime) {
        return Err(GatewayError::service_unavailable(
            "foreign_runtime",
            &format!("{} runtime is not available", runtime.name()),
        ));
    }

    // Build script source
    let source = match req.source.source_type {
        ScriptSourceType::Inline => beebotos_foreign_rt::ScriptSource::Inline {
            code: req.source.content,
        },
        ScriptSourceType::Prebuilt => beebotos_foreign_rt::ScriptSource::Prebuilt {
            module_id: req.source.content,
            entrypoint: None,
        },
        ScriptSourceType::File => beebotos_foreign_rt::ScriptSource::File {
            path: std::path::PathBuf::from(&req.source.content),
        },
    };

    // Build sandbox requirements
    let mut sandbox = beebotos_foreign_rt::SandboxRequirements::default();
    sandbox.max_memory_mb = req.sandbox.max_memory_mb;
    sandbox.network_allowed = req.sandbox.network_allowed;
    sandbox.gpu_allowed = req.sandbox.gpu_allowed;
    for mapping in req.sandbox.filesystem_paths {
        sandbox
            .filesystem_paths
            .push(beebotos_foreign_rt::PathMapping {
                host_path: std::path::PathBuf::from(mapping.host_path),
                guest_path: std::path::PathBuf::from(mapping.guest_path),
                read_only: mapping.read_only,
            });
    }

    // Build script task
    let task_id = uuid::Uuid::new_v4().to_string();
    let task = beebotos_foreign_rt::ScriptTask {
        task_id: task_id.clone(),
        runtime,
        source,
        entrypoint: req.entrypoint,
        input: req.input,
        sandbox,
        permissions: vec![],
        timeout: std::time::Duration::from_secs(req.timeout_secs),
        agent_id: req.agent_id,
    };

    // Execute
    let start = std::time::Instant::now();
    let result = manager
        .execute(task)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Foreign runtime execution failed: {}", e),
            correlation_id: task_id.clone(),
        })?;

    let execution_time_ms = start.elapsed().as_millis() as u64;

    // Determine execution route from result or logs
    let execution_route = if result.success {
        "auto".to_string()
    } else {
        "unknown".to_string()
    };

    // Convert logs
    let logs: Vec<LogEntryResponse> = result
        .logs
        .into_iter()
        .map(|log| LogEntryResponse {
            level: format!("{:?}", log.level).to_lowercase(),
            message: log.message,
            timestamp: log
                .timestamp
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        })
        .collect();

    // Build gas report
    let gas = result.gas_report;
    let gas_used = GasReportResponse {
        compute: gas.compute_gas,
        memory: gas.memory_gas,
        io: gas.io_gas,
        network: gas.network_gas,
        storage: gas.storage_gas,
        total: gas.total(),
    };

    info!(
        task_id = %task_id,
        success = result.success,
        execution_time_ms,
        gas_total = gas_used.total,
        "Foreign runtime script execution completed"
    );

    Ok(Json(ExecuteScriptResponse {
        success: result.success,
        output: result.output,
        execution_time_ms,
        gas_used,
        logs,
        execution_route,
    }))
}

/// List available foreign runtimes
pub async fn list_runtimes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListRuntimesResponse>, GatewayError> {
    let manager = state.foreign_rt_manager.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable(
            "foreign_runtime",
            "Foreign runtime manager not initialized",
        )
    })?;

    let runtimes = vec![
        RuntimeInfoResponse {
            name: "python".to_string(),
            available: manager.is_available(beebotos_foreign_rt::ForeignRuntime::Python),
            wasm_available: manager.is_wasm_available(beebotos_foreign_rt::ForeignRuntime::Python),
            process_available: manager
                .is_process_available(beebotos_foreign_rt::ForeignRuntime::Python),
            default_max_memory_mb: 256,
            default_timeout_secs: 30,
        },
        RuntimeInfoResponse {
            name: "nodejs".to_string(),
            available: manager.is_available(beebotos_foreign_rt::ForeignRuntime::NodeJs),
            wasm_available: manager.is_wasm_available(beebotos_foreign_rt::ForeignRuntime::NodeJs),
            process_available: manager
                .is_process_available(beebotos_foreign_rt::ForeignRuntime::NodeJs),
            default_max_memory_mb: 256,
            default_timeout_secs: 30,
        },
    ];

    Ok(Json(ListRuntimesResponse { runtimes }))
}

/// Health check for foreign runtime
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let manager = state.foreign_rt_manager.as_ref();

    let python_available = manager
        .map(|m| m.is_available(beebotos_foreign_rt::ForeignRuntime::Python))
        .unwrap_or(false);
    let nodejs_available = manager
        .map(|m| m.is_available(beebotos_foreign_rt::ForeignRuntime::NodeJs))
        .unwrap_or(false);

    Ok(Json(serde_json::json!({
        "status": if python_available || nodejs_available { "healthy" } else { "degraded" },
        "python": {
            "available": python_available,
        },
        "nodejs": {
            "available": nodejs_available,
        }
    })))
}
