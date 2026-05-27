//! MCP Server API Service
//!
//! Manages MCP (Model Context Protocol) client configurations locally.

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::client::{ApiClient, ApiError};

/// MCP Server transport configuration
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransport {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    #[serde(rename = "sse")]
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    #[serde(rename = "websocket")]
    Websocket {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
}

impl Default for McpTransport {
    fn default() -> Self {
        McpTransport::Stdio {
            command: String::new(),
            args: Vec::new(),
            env: None,
        }
    }
}

/// MCP Server configuration
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    pub key: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            key: String::new(),
            name: String::new(),
            enabled: true,
            transport: McpTransport::default(),
            description: None,
        }
    }
}

/// MCP Server runtime status
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum McpServerStatus {
    #[default]
    Disconnected,
    Connected,
    Error,
    Connecting,
}

/// MCP Server with runtime info
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpServer {
    pub config: McpServerConfig,
    #[serde(default)]
    pub status: McpServerStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpTool>>,
}

/// MCP Tool info
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Import MCP config request format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpImportConfig {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: Option<HashMap<String, McpServerEntry>>,
}

/// Single MCP server entry in import format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

const MCP_STORAGE_KEY: &str = "beebotos_mcp_servers";

/// MCP Server Service
///
/// Manages MCP server configurations via local storage.
#[derive(Clone)]
pub struct McpServerService {
    _client: ApiClient,
}

impl McpServerService {
    pub fn new(client: ApiClient) -> Self {
        Self { _client: client }
    }

    /// Load all MCP servers from local storage
    pub fn list(&self) -> Result<Vec<McpServer>, ApiError> {
        let stored: Result<Vec<McpServerConfig>, _> = LocalStorage::get(MCP_STORAGE_KEY);
        let configs = stored.unwrap_or_default();
        Ok(configs
            .into_iter()
            .map(|config| McpServer {
                status: if config.enabled {
                    // In a real implementation, this would check actual connection status
                    // For demo, we simulate based on a stored status
                    McpServerStatus::Disconnected
                } else {
                    McpServerStatus::Disconnected
                },
                error_message: None,
                tools: None,
                config,
            })
            .collect())
    }

    /// Get a single MCP server by key
    pub fn get(&self, key: &str) -> Result<Option<McpServer>, ApiError> {
        let servers = self.list()?;
        Ok(servers.into_iter().find(|s| s.config.key == key))
    }

    /// Save or update an MCP server
    pub fn save(&self, server: McpServerConfig) -> Result<McpServerConfig, ApiError> {
        let mut configs: Vec<McpServerConfig> =
            LocalStorage::get(MCP_STORAGE_KEY).unwrap_or_default();

        let key = server.key.clone();
        // Update existing or add new
        let pos = configs.iter().position(|c| c.key == key);
        match pos {
            Some(idx) => configs[idx] = server,
            None => configs.push(server),
        }

        LocalStorage::set(MCP_STORAGE_KEY, &configs).map_err(|e| {
            ApiError::Network(format!("Failed to save MCP config: {}", e))
        })?;

        Ok(configs
            .into_iter()
            .find(|c| c.key == key)
            .unwrap_or_default())
    }

    /// Delete an MCP server by key
    pub fn delete(&self, key: &str) -> Result<(), ApiError> {
        let mut configs: Vec<McpServerConfig> =
            LocalStorage::get(MCP_STORAGE_KEY).unwrap_or_default();
        configs.retain(|c| c.key != key);
        LocalStorage::set(MCP_STORAGE_KEY, &configs).map_err(|e| {
            ApiError::Network(format!("Failed to delete MCP config: {}", e))
        })?;
        Ok(())
    }

    /// Import MCP servers from JSON config
    pub fn import_config(&self, json: &str) -> Result<Vec<McpServerConfig>, ApiError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| ApiError::Serialization(format!("Invalid JSON: {}", e)))?;

        let mut imported = Vec::new();

        if let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) {
            for (key, entry) in servers {
                if let Ok(entry) = serde_json::from_value::<McpServerEntry>(entry.clone()) {
                    let config = McpServerConfig {
                        key: key.clone(),
                        name: entry.name.clone().unwrap_or_else(|| key.clone()),
                        enabled: true,
                        transport: McpTransport::Stdio {
                            command: entry.command,
                            args: entry.args,
                            env: entry.env,
                        },
                        description: None,
                    };
                    imported.push(config);
                }
            }
        } else if let Ok(entry) = serde_json::from_value::<McpServerEntry>(value.clone()) {
            // Single config format - try to extract key from the value
            let key = value
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("imported")
                .to_string();
            let config = McpServerConfig {
                key: key.clone(),
                name: entry.name.clone().unwrap_or_else(|| key.clone()),
                enabled: true,
                transport: McpTransport::Stdio {
                    command: entry.command,
                    args: entry.args,
                    env: entry.env,
                },
                description: None,
            };
            imported.push(config);
        }

        if imported.is_empty() {
            return Err(ApiError::Serialization(
                "No valid MCP servers found in config".to_string(),
            ));
        }

        // Merge with existing configs
        let mut configs: Vec<McpServerConfig> =
            LocalStorage::get(MCP_STORAGE_KEY).unwrap_or_default();
        for new_config in &imported {
            let pos = configs.iter().position(|c| c.key == new_config.key);
            match pos {
                Some(idx) => configs[idx] = new_config.clone(),
                None => configs.push(new_config.clone()),
            }
        }

        LocalStorage::set(MCP_STORAGE_KEY, &configs).map_err(|e| {
            ApiError::Network(format!("Failed to import MCP config: {}", e))
        })?;

        Ok(imported)
    }

    /// Simulate connecting to an MCP server
    pub async fn connect(&self, key: &str) -> Result<McpServer, ApiError> {
        // Simulate connection delay
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            TimeoutFuture::new(1000).await;
        }

        let mut servers = self.list()?;
        if let Some(server) = servers.iter_mut().find(|s| s.config.key == key) {
            // Simulate random success/failure for demo
            server.status = McpServerStatus::Connected;
            server.error_message = None;
            Ok(server.clone())
        } else {
            Err(ApiError::NotFound)
        }
    }

    /// Simulate disconnecting from an MCP server
    pub async fn disconnect(&self, key: &str) -> Result<McpServer, ApiError> {
        let mut servers = self.list()?;
        if let Some(server) = servers.iter_mut().find(|s| s.config.key == key) {
            server.status = McpServerStatus::Disconnected;
            server.error_message = None;
            Ok(server.clone())
        } else {
            Err(ApiError::NotFound)
        }
    }

    /// Get tools for an MCP server
    pub async fn list_tools(&self, key: &str) -> Result<Vec<McpTool>, ApiError> {
        let server = self
            .get(key)?
            .ok_or_else(|| ApiError::NotFound)?;

        if server.status != McpServerStatus::Connected {
            return Err(ApiError::Network(
                "MCP client not connected".to_string(),
            ));
        }

        // Return demo tools for connected servers
        Ok(vec![
            McpTool {
                name: "execute_command".to_string(),
                description: Some("Execute a shell command".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command to execute" }
                    },
                    "required": ["command"]
                }),
            },
            McpTool {
                name: "read_file".to_string(),
                description: Some("Read file contents".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" }
                    },
                    "required": ["path"]
                }),
            },
        ])
    }
}
