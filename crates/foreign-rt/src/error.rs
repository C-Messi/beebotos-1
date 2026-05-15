//! Error types for foreign runtime execution

use thiserror::Error;

/// Foreign runtime error type
#[derive(Debug, Error, Clone)]
pub enum ForeignRtError {
    /// Runtime not available or not initialized
    #[error("Runtime not available: {0}")]
    RuntimeNotAvailable(String),

    /// Script compilation/validation failed
    #[error("Script compilation failed: {0}")]
    CompilationFailed(String),

    /// Script execution failed
    #[error("Script execution failed: {0}")]
    ExecutionFailed(String),

    /// Sandbox violation detected
    #[error("Sandbox violation: {reason}")]
    SandboxViolation {
        /// Reason for violation
        reason: String,
        /// Runtime type
        runtime: String,
    },

    /// Resource limit exceeded
    #[error("Resource limit exceeded: {limit} (used {used}, max {max})")]
    ResourceLimitExceeded {
        /// Resource name
        limit: String,
        /// Used amount
        used: u64,
        /// Maximum allowed
        max: u64,
    },

    /// Timeout
    #[error("Execution timeout after {0:?}")]
    Timeout(std::time::Duration),

    /// Capability insufficient
    #[error("Insufficient capability: required {required:?}, have {current:?}")]
    InsufficientCapability {
        /// Required capability
        required: String,
        /// Current capability
        current: String,
    },

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Process sandbox error
    #[error("Process sandbox error: {0}")]
    ProcessSandbox(String),

    /// WASM runtime error
    #[error("WASM runtime error: {0}")]
    WasmRuntime(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Route not found
    #[error("No suitable execution route found for runtime {runtime}")]
    RouteNotFound {
        /// Runtime type
        runtime: String,
    },

    /// Pool exhausted
    #[error("Runtime pool exhausted for {runtime}")]
    PoolExhausted {
        /// Runtime type
        runtime: String,
    },
}

impl ForeignRtError {
    /// Create a sandbox violation error
    pub fn sandbox_violation(runtime: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::SandboxViolation {
            runtime: runtime.into(),
            reason: reason.into(),
        }
    }

    /// Create a resource limit exceeded error
    pub fn resource_limit(limit: impl Into<String>, used: u64, max: u64) -> Self {
        Self::ResourceLimitExceeded {
            limit: limit.into(),
            used,
            max,
        }
    }

    /// Create an IO error
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }
}

impl From<std::io::Error> for ForeignRtError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for ForeignRtError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, ForeignRtError>;
