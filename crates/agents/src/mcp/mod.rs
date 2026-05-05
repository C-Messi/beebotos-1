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
mod tools;
mod context;

pub use client::{ClientConfig, MCPClient};
pub use server::{MCPServer, ServerConfig};
pub use transport::{HttpTransport, HttpTransportConfig, StdioTransport, StdioTransportConfig, Transport, TransportBridge};
pub use types::*;

// Re-export internal types for backward compatibility
pub use context::McpContext;
pub use tools::{McpTool, McpToolRegistry};

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

/// MCP Manager for handling multiple MCP connections
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

    /// List all registered servers
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Initialize all connections
    pub async fn initialize_all(&self) -> Result<(), MCPError> {
        let clients = self.clients.read().await;
        for (name, client) in clients.iter() {
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
