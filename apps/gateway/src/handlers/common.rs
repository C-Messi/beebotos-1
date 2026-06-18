//! Common Handler Utilities
//!
//! Shared helper functions for HTTP handlers to reduce code duplication.

use gateway::error::GatewayError;
use gateway::middleware::AuthUser;
use gateway::{AgentInfo, QueryResult, StateQuery, StateStore};

use crate::models::AgentRecord;

/// Check if user is admin or owns the agent
///
/// Returns Ok(()) if user has permission, Err(GatewayError::forbidden)
/// otherwise
///
/// # Arguments
/// * `user` - The authenticated user
/// * `agent` - The agent record to check ownership
///
/// # Example
/// ```rust
/// check_ownership(&user, &agent)?;
/// ```
pub fn check_ownership(user: &AuthUser, agent: &AgentRecord) -> Result<(), GatewayError> {
    if user.is_admin() || agent.owner_id.as_deref() == Some(&user.user_id) {
        Ok(())
    } else {
        Err(GatewayError::forbidden(
            "You don't have permission to access this agent",
        ))
    }
}

/// Fetch an agent from StateStore and verify the user can access it.
pub async fn get_authorized_agent_info(
    state_store: &StateStore,
    user: &AuthUser,
    agent_id: &str,
) -> Result<AgentInfo, GatewayError> {
    let query_result = state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: agent_id.to_string(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;

    let info = match query_result {
        QueryResult::AgentInfo {
            agent_id,
            config,
            current_state,
            metadata,
            created_at,
            updated_at,
            task_count,
            success_count,
            failure_count,
        } => AgentInfo {
            agent_id,
            config,
            current_state,
            metadata,
            created_at,
            updated_at,
            task_count,
            success_count,
            failure_count,
        },
        _ => return Err(GatewayError::not_found("Agent", agent_id)),
    };

    if user.is_admin() || info.metadata.get("owner_id").map(String::as_str) == Some(&user.user_id) {
        Ok(info)
    } else {
        Err(GatewayError::forbidden(
            "You don't have permission to access this agent",
        ))
    }
}

/// Check if user is admin
///
/// Returns Ok(()) if user is admin, Err(GatewayError::forbidden) otherwise
#[allow(dead_code)]
pub fn require_admin(user: &AuthUser) -> Result<(), GatewayError> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(GatewayError::forbidden("Admin access required"))
    }
}

/// Get user ID or return error if not authenticated
///
/// Helper for handlers that need the user ID
#[allow(dead_code)]
pub fn get_user_id(user: &AuthUser) -> &str {
    &user.user_id
}

/// Check if user can access agent (admin or owner)
///
/// Returns true if user has permission, false otherwise
#[allow(dead_code)]
pub fn can_access_agent(user: &AuthUser, agent: &AgentRecord) -> bool {
    user.is_admin() || agent.owner_id.as_deref() == Some(&user.user_id)
}

/// Build forbidden error for agent access
#[allow(dead_code)]
pub fn agent_access_denied() -> GatewayError {
    GatewayError::forbidden("You don't have permission to access this agent")
}

#[cfg(test)]
mod tests {
    // Note: These tests would require mocking AuthUser and AgentRecord
    // which depends on the gateway-lib internals
}
