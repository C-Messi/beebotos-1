//! Error Handling
//!
//! Re-exports from gateway-lib and minimal app-specific error types.
//!
//! For most error handling, use gateway::error::GatewayError directly.

pub use gateway::error::GatewayError;

/// Result type alias using GatewayError
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, GatewayError>;

/// Application-specific errors
///
/// These are internal errors that get converted to GatewayError for HTTP
/// responses.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Not found
    #[error("{0} not found")]
    NotFound(String),

    /// Kernel error
    #[error("Kernel error: {0}")]
    Kernel(String),

    /// Agent error
    #[error("Agent error: {0}")]
    Agent(#[from] beebotos_agents::error::AgentError),

    /// Chain/Blockchain error
    #[error("Chain error: {0}")]
    Chain(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Validation error
    #[error("Validation error")]
    Validation(Vec<ValidationError>),

    /// Unauthorized
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Validation error details
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: String,
}

impl AppError {
    /// Create a database error
    pub fn database(e: sqlx::Error) -> Self {
        Self::Database(e)
    }

    /// Create a not found error
    pub fn not_found(resource: &str, id: &str) -> Self {
        Self::NotFound(format!("{}: {}", resource, id))
    }

    /// Create a kernel error
    pub fn kernel(msg: impl Into<String>) -> Self {
        Self::Kernel(msg.into())
    }

    /// Create a chain error
    pub fn chain(msg: impl Into<String>) -> Self {
        Self::Chain(msg.into())
    }

    /// Create a validation error
    pub fn validation(errors: Vec<ValidationError>) -> Self {
        Self::Validation(errors)
    }

    /// Create a bad request error
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::Internal(format!("Bad request: {}", msg.into()))
    }
}

impl From<AppError> for GatewayError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Database(e) => GatewayError::internal(format!("Database error: {}", e)),
            AppError::NotFound(msg) => GatewayError::not_found("resource", &msg),
            AppError::Kernel(msg) => GatewayError::internal(format!("Kernel error: {}", msg)),
            AppError::Agent(agent_err) => convert_agent_error(agent_err),
            AppError::Chain(msg) => GatewayError::internal(format!("Chain error: {}", msg)),
            AppError::Configuration(msg) => GatewayError::service_unavailable("Configuration", msg),
            AppError::NotImplemented(msg) => GatewayError::bad_request(msg),
            AppError::Validation(errors) => GatewayError::validation(
                errors
                    .into_iter()
                    .map(|e| gateway::error::ValidationError {
                        field: e.field,
                        message: e.message,
                        code: e.code,
                    })
                    .collect(),
            ),
            AppError::Unauthorized(msg) => GatewayError::Unauthorized {
                message: msg,
                code: "AUTH_FAILED".to_string(),
            },
            AppError::Internal(msg) => GatewayError::internal(msg),
        }
    }
}

/// 🟢 P1 FIX: Direct conversion from AgentError to GatewayError
///
/// Convert AgentError to GatewayError
///
/// This function provides detailed mapping from AgentError to appropriate
/// GatewayError
///
/// 🟢 P1 FIX: Comprehensive error conversion covering all AgentError variants
/// with appropriate HTTP status codes and user-facing messages.
///
/// Usage:
/// ```rust
/// let result = agent_runtime
///     .execute_task(&id, task)
///     .await
///     .map_err(convert_agent_error)?;
/// ```

/// Convert AgentError to GatewayError
/// This function provides detailed mapping from AgentError to appropriate
/// GatewayError
///
/// 🟢 P1 FIX: Comprehensive error conversion covering all AgentError variants
/// with appropriate HTTP status codes and user-facing messages.
pub fn convert_agent_error(err: beebotos_agents::error::AgentError) -> GatewayError {
    use beebotos_agents::error::AgentError;

    // Log the original error for debugging (with correlation ID)
    let correlation_id = uuid::Uuid::new_v4().to_string();
    tracing::debug!(
        correlation_id = %correlation_id,
        agent_error = %err,
        "Converting AgentError to GatewayError"
    );

    match err {
        // 4xx Client Errors
        AgentError::AgentNotFound(msg) => GatewayError::not_found("Agent", msg),
        AgentError::SkillNotFound(msg) => GatewayError::not_found("Skill", msg),
        AgentError::InvalidConfig(msg) => {
            GatewayError::bad_request(format!("Invalid configuration: {}", msg))
        }
        AgentError::NotConfigured(msg) => {
            GatewayError::bad_request(format!("Not configured: {}", msg))
        }
        AgentError::UnsupportedTaskType(msg) => {
            GatewayError::bad_request(format!("Unsupported task type: {}", msg))
        }
        AgentError::AgentExists(msg) => GatewayError::Validation {
            errors: vec![gateway::error::ValidationError {
                field: "agent".to_string(),
                message: format!("Agent already exists: {}", msg),
                code: "ALREADY_EXISTS".to_string(),
            }],
        },

        // 401 Unauthorized
        AgentError::Authentication(msg) | AgentError::AuthenticationFailed(msg) => {
            GatewayError::Unauthorized {
                message: msg,
                code: "AGENT_AUTH_FAILED".to_string(),
            }
        }

        // 403 Forbidden
        AgentError::CapabilityDenied(msg) => {
            GatewayError::forbidden(format!("Capability denied: {}", msg))
        }

        // 429 Rate Limited
        AgentError::RateLimited(msg) => GatewayError::rate_limited(Some(extract_retry_after(&msg))),

        // 503 Service Unavailable
        AgentError::NotConnected(msg) => {
            tracing::warn!(correlation_id = %correlation_id, details = %msg, "Agent not connected");
            GatewayError::service_unavailable(
                "Agent",
                "智能体当前没有连接成功，我已经停止继续执行。请稍后重试或检查通道/服务状态。",
            )
        }

        // 500 Internal Server Errors
        AgentError::Timeout(msg) => {
            tracing::warn!(correlation_id = %correlation_id, details = %msg, "Agent operation timed out");
            GatewayError::timeout("智能体任务执行", 30)
        }
        AgentError::Platform(msg) => friendly_agent_internal(
            correlation_id,
            "Platform error",
            msg,
            "平台通道处理失败，我已经停止继续执行。请稍后重试，或检查对应通道配置。",
        ),
        AgentError::Execution(msg) => friendly_agent_internal(
            correlation_id,
            "Execution error",
            msg,
            "任务执行过程中遇到问题，我已经停止继续重试，避免重复调用工具或陷入循环。",
        ),
        AgentError::TaskExecutionFailed(msg) => friendly_agent_internal(
            correlation_id,
            "Task execution failed",
            msg,
            "任务执行失败，我已经停止继续运行。请缩小任务范围或稍后重试。",
        ),
        AgentError::A2A(msg) => friendly_agent_internal(
            correlation_id,
            "A2A communication error",
            msg,
            "智能体之间通信失败，我已经停止继续执行。请稍后重试。",
        ),
        AgentError::Wasm(msg) => friendly_agent_internal(
            correlation_id,
            "WASM execution error",
            msg,
            "沙箱执行失败，我已经停止继续运行。请检查技能或运行时配置。",
        ),
        AgentError::MCPError(msg) => friendly_agent_internal(
            correlation_id,
            "MCP tool error",
            msg,
            "工具调用失败，我已经停止继续重试。请检查 MCP 工具配置、权限或参数。",
        ),
        AgentError::ServiceMesh(msg) => friendly_agent_internal(
            correlation_id,
            "Service mesh error",
            msg,
            "服务发现或路由失败，我已经停止继续执行。请稍后重试。",
        ),
        AgentError::DIDResolution(msg) => friendly_agent_internal(
            correlation_id,
            "DID resolution error",
            msg,
            "身份解析失败，我已经停止继续执行。请检查身份或凭证配置。",
        ),
        AgentError::CommunicationFailed(msg) => friendly_agent_internal(
            correlation_id,
            "Communication failed",
            msg,
            "消息发送或接收失败，我已经停止继续执行。请检查通道连接状态。",
        ),
        AgentError::Planning(msg) => friendly_agent_internal(
            correlation_id,
            "Planning error",
            msg,
            "任务规划失败，我已经停止继续运行。请缩小任务范围或补充更明确的目标。",
        ),
        AgentError::MessageReceiveFailed(msg) => friendly_agent_internal(
            correlation_id,
            "Message receive failed",
            msg,
            "消息接收失败，请稍后重试或检查通道状态。",
        ),
        AgentError::MessageSendFailed(msg) => friendly_agent_internal(
            correlation_id,
            "Message send failed",
            msg,
            "回复发送失败，请稍后重试或检查通道状态。",
        ),
        AgentError::Internal(msg) => friendly_agent_internal(
            correlation_id,
            "Internal agent error",
            msg,
            "智能体内部处理失败，我已经停止继续执行。请稍后重试。",
        ),
        AgentError::Database(msg) => friendly_agent_internal(
            correlation_id,
            "Database error",
            msg,
            "数据读写失败，我已经停止继续执行。请稍后重试。",
        ),
        AgentError::Serialization(msg) => friendly_agent_internal(
            correlation_id,
            "Serialization error",
            msg,
            "数据格式处理失败，我已经停止继续执行。请检查输入内容后重试。",
        ),
        AgentError::ResourceLimit(msg) => friendly_agent_internal(
            correlation_id,
            "Resource limit exceeded",
            msg,
            "任务触达资源上限，我已经停止继续执行，避免占用过多系统资源。",
        ),
        AgentError::NotFound(msg) => GatewayError::not_found("resource", msg),
        AgentError::Wallet(msg) => friendly_agent_internal(
            correlation_id,
            "Wallet error",
            msg,
            "钱包操作失败，我已经停止继续执行。请检查钱包配置、余额或权限。",
        ),
        AgentError::TimeoutMsg(msg) => {
            tracing::warn!(correlation_id = %correlation_id, details = %msg, "Agent operation timed out");
            GatewayError::timeout("智能体任务执行", 30)
        }
    }
}

fn friendly_agent_internal(
    correlation_id: String,
    label: &str,
    details: String,
    user_message: &str,
) -> GatewayError {
    tracing::warn!(
        correlation_id = %correlation_id,
        error_label = %label,
        details = %details,
        "Agent error converted to user-facing message"
    );
    GatewayError::agent(format!("{}（参考编号：{}）", user_message, correlation_id))
}

/// Extract retry after seconds from rate limit message
fn extract_retry_after(msg: &str) -> u64 {
    // Try to extract number from message like "Rate limited: retry after 60
    // seconds"
    msg.chars()
        .filter(|c| c.is_numeric())
        .collect::<String>()
        .parse::<u64>()
        .unwrap_or(60) // Default 60 seconds
}

/// Helper macros for early returns
#[macro_export]
macro_rules! bail {
    ($err:expr) => {
        return Err($err.into());
    };
}

#[macro_export]
macro_rules! not_found {
    ($resource:expr, $id:expr) => {
        return Err($crate::error::GatewayError::not_found($resource, $id));
    };
}
