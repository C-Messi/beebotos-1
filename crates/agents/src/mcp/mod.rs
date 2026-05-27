//! Model Context Protocol (MCP)
//!
//! Anthropic MCP implementation for tool/resource/prompt management.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub mod client;
pub mod server;
pub mod skill_bridge;
pub mod transport;
pub mod types;

// Internal helper modules
mod context;
mod tools;

pub use client::{ClientConfig, MCPClient};
// Re-export internal types for backward compatibility
pub use context::McpContext;
pub use server::{MCPServer, ServerConfig};
pub use tools::{McpTool, McpToolRegistry};
pub use transport::{
    HttpTransport, HttpTransportConfig, StdioTransport, StdioTransportConfig, Transport,
    TransportBridge,
};
pub use types::*;

/// MCP capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPCapability {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub sampling: bool,
}

impl Default for MCPCapability {
    fn default() -> Self {
        Self {
            tools: true,
            resources: true,
            prompts: false,
            sampling: false,
        }
    }
}

/// MCP implementation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPImplementation {
    pub name: String,
    pub version: String,
}

impl MCPImplementation {
    pub fn beebot() -> Self {
        Self {
            name: "BeeBotOS MCP".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Lightweight MCP tool metadata used for search-mode discovery.
#[derive(Debug, Clone)]
pub struct MCPToolSummary {
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
}

/// MCP Manager for handling multiple MCP connections
#[derive(Clone)]
pub struct MCPManager {
    clients: Arc<RwLock<HashMap<String, Arc<MCPClient>>>>,
    servers: Arc<RwLock<HashMap<String, Arc<MCPServer>>>>,
}

impl MCPManager {
    /// Create new MCP manager
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a client
    pub async fn register_client(&self, name: impl Into<String>, client: Arc<MCPClient>) {
        let mut clients = self.clients.write().await;
        clients.insert(name.into(), client);
    }

    /// Register a server
    pub async fn register_server(&self, name: impl Into<String>, server: Arc<MCPServer>) {
        let mut servers = self.servers.write().await;
        servers.insert(name.into(), server);
    }

    /// Get client by name
    pub async fn get_client(&self, name: &str) -> Option<Arc<MCPClient>> {
        let clients = self.clients.read().await;
        clients.get(name).cloned()
    }

    /// List all registered clients
    pub async fn list_clients(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }

    /// List MCP tools as lightweight summaries.
    ///
    /// The MCP protocol returns full tool descriptors from `tools/list`;
    /// callers should use this method when they only need search/index
    /// metadata and do not want to expose schemas to the LLM yet.
    pub async fn list_tool_summaries(&self) -> Result<Vec<MCPToolSummary>, MCPError> {
        let clients = self.clients.read().await;
        let client_entries: Vec<(String, Arc<MCPClient>)> = clients
            .iter()
            .map(|(name, client)| (name.clone(), client.clone()))
            .collect();
        drop(clients);

        let mut summaries = Vec::new();
        for (server_name, client) in client_entries {
            let result = client.list_tools(None).await?;
            summaries.extend(result.tools.into_iter().map(|tool| MCPToolSummary {
                server_name: server_name.clone(),
                tool_name: tool.name,
                description: tool.description,
            }));
        }

        Ok(summaries)
    }

    /// Load one MCP tool descriptor with its full schema on demand.
    pub async fn get_tool_schema(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Result<types::Tool, MCPError> {
        let client = self
            .get_client(server_name)
            .await
            .ok_or_else(|| MCPError::ToolNotFound(format!("{}/{}", server_name, tool_name)))?;
        let result = client.list_tools(None).await?;
        result
            .tools
            .into_iter()
            .find(|tool| tool.name == tool_name)
            .ok_or_else(|| MCPError::ToolNotFound(format!("{}/{}", server_name, tool_name)))
    }

    /// List all registered servers
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Initialize all connections
    ///
    /// FIX: Skips already-initialized clients to support shared MCPManager
    /// across multiple agents.
    pub async fn initialize_all(&self) -> Result<(), MCPError> {
        let clients = self.clients.read().await;
        for (name, client) in clients.iter() {
            if client.is_initialized() {
                continue;
            }
            client
                .initialize()
                .await
                .map_err(|e| MCPError::InitializationFailed(format!("{}: {}", name, e)))?;
        }
        Ok(())
    }

    /// Close all connections
    pub async fn close_all(&self) {
        let clients = self.clients.read().await;
        for (_, client) in clients.iter() {
            let _ = client.close().await;
        }
    }
}

impl Default for MCPManager {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP errors
///
/// 🟠 HIGH FIX: Proper thiserror derive with source chains
#[derive(Debug, Clone, thiserror::Error)]
pub enum MCPError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Serialization failed: {0}")]
    SerializationFailed(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Invalid params: {0}")]
    InvalidParams(String),

    #[error("Request timed out")]
    Timeout,

    #[error("MCP not initialized")]
    NotInitialized,
}
