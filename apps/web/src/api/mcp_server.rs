//! MCP server API service.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::client::{ApiClient, ApiError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransport {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth_token: Option<String>,
        #[serde(default)]
        auth_token_set: bool,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        use_sse: bool,
    },
}

impl Default for McpTransport {
    fn default() -> Self {
        McpTransport::Stdio {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(flatten)]
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
}

impl McpServerConfig {
    pub fn key(&self) -> &str {
        &self.name
    }
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            transport: McpTransport::default(),
            timeout_ms: None,
            retry_count: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum McpServerStatus {
    #[default]
    Disconnected,
    Connected,
    Error,
    Connecting,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpServer {
    pub name: String,
    pub connected: bool,
    pub config: McpServerConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl McpServer {
    pub fn status(&self) -> McpServerStatus {
        if self.connected {
            McpServerStatus::Connected
        } else if self.error_message.is_some() {
            McpServerStatus::Error
        } else {
            McpServerStatus::Disconnected
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, rename = "input_schema")]
    pub parameters: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpListToolsResponse {
    pub server: String,
    pub tools: Vec<McpTool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpCallToolRequest {
    #[serde(default)]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpCallToolResponse {
    pub success: bool,
    pub output: String,
    pub is_error: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpMutationResponse {
    pub server: McpServer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Clone)]
pub struct McpServerService {
    client: ApiClient,
}

impl McpServerService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn list(&self) -> Result<Vec<McpServer>, ApiError> {
        self.client.get("/mcp/servers").await
    }

    pub async fn save(&self, server: McpServerConfig) -> Result<McpServerConfig, ApiError> {
        let response: McpMutationResponse = if server.name.trim().is_empty() {
            return Err(ApiError::Serialization(
                "MCP server name is required".to_string(),
            ));
        } else {
            match self
                .client
                .put(&format!("/mcp/servers/{}", server.name), &server)
                .await
            {
                Ok(response) => response,
                Err(ApiError::NotFound) => self.client.post("/mcp/servers", &server).await?,
                Err(e) => return Err(e),
            }
        };
        Ok(response.server.config)
    }

    pub async fn delete(&self, key: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("/mcp/servers/{}", key)).await
    }

    pub async fn import_config(&self, json: &str) -> Result<Vec<McpServerConfig>, ApiError> {
        let configs = parse_import_config(json)?;
        let mut saved = Vec::new();
        for config in configs {
            saved.push(self.save(config).await?);
        }
        Ok(saved)
    }

    pub async fn connect(&self, key: &str) -> Result<McpServer, ApiError> {
        let response: McpMutationResponse = self
            .client
            .post(
                &format!("/mcp/servers/{}/connect", key),
                &serde_json::json!({}),
            )
            .await?;
        Ok(response.server)
    }

    pub async fn disconnect(&self, key: &str) -> Result<McpServer, ApiError> {
        let response: McpMutationResponse = self
            .client
            .post(
                &format!("/mcp/servers/{}/disconnect", key),
                &serde_json::json!({}),
            )
            .await?;
        Ok(response.server)
    }

    pub async fn list_tools(&self, key: &str) -> Result<Vec<McpTool>, ApiError> {
        let response: McpListToolsResponse = self
            .client
            .get(&format!("/mcp/servers/{}/tools", key))
            .await?;
        Ok(response.tools)
    }

    pub async fn call_tool(
        &self,
        key: &str,
        tool: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<McpCallToolResponse, ApiError> {
        self.client
            .post(
                &format!("/mcp/servers/{}/tools/{}/call", key, tool),
                &McpCallToolRequest { arguments },
            )
            .await
    }
}

pub fn parse_import_config(json: &str) -> Result<Vec<McpServerConfig>, ApiError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| ApiError::Serialization(format!("Invalid JSON: {}", e)))?;

    let mut imported = Vec::new();
    if let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) {
        for (key, entry) in servers {
            if let Ok(entry) = serde_json::from_value::<McpServerEntry>(entry.clone()) {
                imported.push(McpServerConfig {
                    name: entry.name.unwrap_or_else(|| key.clone()),
                    transport: McpTransport::Stdio {
                        command: entry.command,
                        args: entry.args,
                        env: entry.env,
                        working_dir: None,
                    },
                    timeout_ms: None,
                    retry_count: None,
                });
            }
        }
    } else if let Ok(config) = serde_json::from_value::<McpServerConfig>(value.clone()) {
        imported.push(config);
    } else if let Ok(entry) = serde_json::from_value::<McpServerEntry>(value.clone()) {
        let name = value
            .get("name")
            .or_else(|| value.get("key"))
            .and_then(|v| v.as_str())
            .unwrap_or("imported")
            .to_string();
        imported.push(McpServerConfig {
            name,
            transport: McpTransport::Stdio {
                command: entry.command,
                args: entry.args,
                env: entry.env,
                working_dir: None,
            },
            timeout_ms: None,
            retry_count: None,
        });
    }

    if imported.is_empty() {
        return Err(ApiError::Serialization(
            "No valid MCP servers found in config".to_string(),
        ));
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_import_config_accepts_claude_style_mcp_servers() {
        let configs = parse_import_config(
            r#"{
                "mcpServers": {
                    "filesystem": {
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-filesystem"],
                        "env": { "ROOT": "/tmp" }
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "filesystem");
        match &configs[0].transport {
            McpTransport::Stdio {
                command, args, env, ..
            } => {
                assert_eq!(command, "npx");
                assert_eq!(args[1], "@modelcontextprotocol/server-filesystem");
                assert_eq!(env.get("ROOT").map(String::as_str), Some("/tmp"));
            }
            _ => panic!("expected stdio transport"),
        }
    }

    #[test]
    fn call_tool_payload_matches_gateway_shape() {
        let request = McpCallToolRequest {
            arguments: serde_json::json!({
                "path": "/tmp/hello.txt"
            })
            .as_object()
            .unwrap()
            .clone(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"arguments":{"path":"/tmp/hello.txt"}}"#);

        let response: McpCallToolResponse =
            serde_json::from_str(r#"{"success":true,"output":"ok","is_error":false}"#).unwrap();
        assert!(response.success);
        assert_eq!(response.output, "ok");
        assert!(!response.is_error);
    }
}
