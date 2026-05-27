//! MCP HTTP Handlers
//!
//! REST API for managing MCP (Model Context Protocol) connections and tools.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};

use crate::error::GatewayError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request/Response types
// ---------------------------------------------------------------------------

/// MCP server info response
#[derive(Debug, Serialize)]
pub struct McpServerInfo {
    pub name: String,
    pub connected: bool,
}

/// MCP tool info response
#[derive(Debug, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    /// Marketplace metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft_token_id: Option<String>,
}

/// List tools response
#[derive(Debug, Serialize)]
pub struct McpListToolsResponse {
    pub server: String,
    pub tools: Vec<McpToolInfo>,
}

/// Call tool request
#[derive(Debug, Deserialize)]
pub struct McpCallToolRequest {
    /// Tool arguments as a JSON object
    #[serde(default)]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

/// Call tool response
#[derive(Debug, Serialize)]
pub struct McpCallToolResponse {
    pub success: bool,
    pub output: String,
    pub is_error: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// List all connected MCP servers.
pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<McpServerInfo>>, GatewayError> {
    let servers = if let Some(ref manager) = state.mcp_manager {
        let client_names = manager.list_clients().await;
        let mut servers = Vec::new();
        for name in client_names {
            // Health check: try to ping the client
            let connected = if let Some(client) = manager.get_client(&name).await {
                client.ping().await.is_ok()
            } else {
                false
            };
            servers.push(McpServerInfo { name, connected });
        }
        servers
    } else {
        vec![]
    };

    Ok(Json(servers))
}

/// List tools available on a specific MCP server.
pub async fn list_tools(
    State(state): State<Arc<AppState>>,
    Path(server_name): Path<String>,
) -> Result<Json<McpListToolsResponse>, GatewayError> {
    let manager = state
        .mcp_manager
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("MCP", "MCP manager not initialized"))?;

    let client = manager
        .get_client(&server_name)
        .await
        .ok_or_else(|| GatewayError::NotFound {
            resource: "MCP server".to_string(),
            id: server_name.clone(),
        })?;

    let result = client
        .list_tools(None)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!(
                "Failed to list tools from MCP server '{}': {}",
                server_name, e
            ),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let tools = result
        .tools
        .into_iter()
        .map(|t| McpToolInfo {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema,
            // Default marketplace metadata for MCP tools (free, no NFT yet)
            price: Some("0".to_string()),
            royalty_percent: Some(0),
            nft_token_id: None,
        })
        .collect();

    Ok(Json(McpListToolsResponse {
        server: server_name,
        tools,
    }))
}

/// Call a tool on a specific MCP server.
pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Path((server_name, tool_name)): Path<(String, String)>,
    Json(req): Json<McpCallToolRequest>,
) -> Result<Json<McpCallToolResponse>, GatewayError> {
    let manager = state
        .mcp_manager
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("MCP", "MCP manager not initialized"))?;

    let client = manager
        .get_client(&server_name)
        .await
        .ok_or_else(|| GatewayError::NotFound {
            resource: "MCP server".to_string(),
            id: server_name.clone(),
        })?;

    let args = if req.arguments.is_empty() {
        None
    } else {
        Some(req.arguments)
    };

    let result = client
        .call_tool(&tool_name, args)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!(
                "MCP tool call failed (server='{}', tool='{}'): {}",
                server_name, tool_name, e
            ),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let output = result
        .content
        .iter()
        .filter_map(|c| match c {
            beebotos_agents::mcp::types::ToolContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Json(McpCallToolResponse {
        success: !result.is_error,
        output,
        is_error: result.is_error,
    }))
}

/// Legacy bridge endpoint.
///
/// MCP tools are no longer registered as skills. Agents use MCP tool search
/// mode: expose `mcp_tool_search` first, then dynamically load selected tool
/// schemas.
/// Admin only.
pub async fn bridge(
    State(_state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;
    Ok(Json(serde_json::json!({
        "success": false,
        "registered": 0,
        "message": "MCP Skill Bridge has been removed. MCP tools are discovered through mcp_tool_search and loaded dynamically."
    })))
}
