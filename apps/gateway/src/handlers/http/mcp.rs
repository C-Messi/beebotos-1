//! MCP HTTP handlers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use gateway::middleware::{require_any_role, AuthUser};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use toml_edit::{value, Array, DocumentMut, InlineTable, Item, Table};

use crate::config::{McpServerConfig, McpTransportConfig};
use crate::error::GatewayError;
use crate::AppState;

const CONFIG_PATH: &str = "config/beebotos.toml";

#[derive(Debug, Serialize)]
pub struct McpServerInfo {
    pub name: String,
    pub connected: bool,
    pub config: McpServerConfigResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfigResponse {
    pub name: String,
    #[serde(flatten)]
    pub transport: McpTransportConfigResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransportConfigResponse {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        auth_token_set: bool,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        use_sse: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct McpServerConfigRequest {
    pub name: String,
    #[serde(flatten)]
    pub transport: McpTransportConfigRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransportConfigRequest {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },
    Http {
        url: String,
        #[serde(default)]
        auth_token: Option<String>,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        use_sse: bool,
    },
}

#[derive(Debug, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft_token_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct McpListToolsResponse {
    pub server: String,
    pub tools: Vec<McpToolInfo>,
}

#[derive(Debug, Deserialize)]
pub struct McpCallToolRequest {
    #[serde(default)]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct McpCallToolResponse {
    pub success: bool,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Serialize)]
pub struct McpMutationResponse {
    pub server: McpServerInfo,
}

pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<McpServerInfo>>, GatewayError> {
    let config = current_config(&state).await;
    let mut servers = Vec::new();
    for server_config in &config.mcp.servers {
        servers.push(server_info(&state, server_config).await);
    }
    Ok(Json(servers))
}

pub async fn create_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpServerConfigRequest>,
) -> Result<Json<McpMutationResponse>, GatewayError> {
    let server = request_to_config(req)?;
    let mut config = current_config(&state).await;
    if config
        .mcp
        .servers
        .iter()
        .any(|existing| existing.name == server.name)
    {
        return Err(GatewayError::bad_request(format!(
            "MCP server '{}' already exists",
            server.name
        )));
    }
    config.mcp.servers.push(server.clone());
    persist_and_reload(&state, &config).await?;
    let info = connect_and_report(&state, &server).await;
    Ok(Json(McpMutationResponse { server: info }))
}

pub async fn update_server(
    State(state): State<Arc<AppState>>,
    AxumPath(name): AxumPath<String>,
    Json(req): Json<McpServerConfigRequest>,
) -> Result<Json<McpMutationResponse>, GatewayError> {
    let mut server = request_to_config(req)?;
    server.name = name.clone();
    let mut config = current_config(&state).await;
    let pos = config
        .mcp
        .servers
        .iter()
        .position(|existing| existing.name == name)
        .ok_or_else(|| GatewayError::not_found("MCP server", &name))?;
    config.mcp.servers[pos] = server.clone();
    persist_and_reload(&state, &config).await?;
    let info = connect_and_report(&state, &server).await;
    Ok(Json(McpMutationResponse { server: info }))
}

pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let mut config = current_config(&state).await;
    let before = config.mcp.servers.len();
    config.mcp.servers.retain(|server| server.name != name);
    if before == config.mcp.servers.len() {
        return Err(GatewayError::not_found("MCP server", &name));
    }
    persist_and_reload(&state, &config).await?;
    state.mcp_manager.remove_client(&name).await;
    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn connect_server(
    State(state): State<Arc<AppState>>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<McpMutationResponse>, GatewayError> {
    let config = current_config(&state).await;
    let server = config
        .mcp
        .servers
        .iter()
        .find(|server| server.name == name)
        .cloned()
        .ok_or_else(|| GatewayError::not_found("MCP server", &name))?;
    let info = connect_and_report(&state, &server).await;
    Ok(Json(McpMutationResponse { server: info }))
}

pub async fn disconnect_server(
    State(state): State<Arc<AppState>>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<McpMutationResponse>, GatewayError> {
    let config = current_config(&state).await;
    let server = config
        .mcp
        .servers
        .iter()
        .find(|server| server.name == name)
        .cloned()
        .ok_or_else(|| GatewayError::not_found("MCP server", &name))?;
    state.mcp_manager.remove_client(&name).await;
    Ok(Json(McpMutationResponse {
        server: server_info(&state, &server).await,
    }))
}

pub async fn list_tools(
    State(state): State<Arc<AppState>>,
    AxumPath(server_name): AxumPath<String>,
) -> Result<Json<McpListToolsResponse>, GatewayError> {
    let manager = &state.mcp_manager;

    let client = manager
        .get_client(&server_name)
        .await
        .ok_or_else(|| GatewayError::not_found("MCP server", &server_name))?;

    let result = client.list_tools(None).await.map_err(|e| {
        GatewayError::service_unavailable(
            "MCP",
            format!("Failed to list tools from '{}': {}", server_name, e),
        )
    })?;

    let tools = result
        .tools
        .into_iter()
        .map(|t| McpToolInfo {
            name: t.name,
            description: t.description,
            input_schema: t.input_schema,
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

pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    AxumPath((server_name, tool_name)): AxumPath<(String, String)>,
    Json(req): Json<McpCallToolRequest>,
) -> Result<Json<McpCallToolResponse>, GatewayError> {
    let manager = &state.mcp_manager;

    let client = manager
        .get_client(&server_name)
        .await
        .ok_or_else(|| GatewayError::not_found("MCP server", &server_name))?;

    let args = if req.arguments.is_empty() {
        None
    } else {
        Some(req.arguments)
    };

    let result = client.call_tool(&tool_name, args).await.map_err(|e| {
        GatewayError::service_unavailable(
            "MCP",
            format!(
                "MCP tool call failed (server='{}', tool='{}'): {}",
                server_name, tool_name, e
            ),
        )
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

async fn current_config(state: &AppState) -> crate::config::BeeBotOSConfig {
    if let Some(manager) = &state.config_manager {
        manager.config().await.clone()
    } else {
        state.config.clone()
    }
}

async fn persist_and_reload(
    state: &AppState,
    config: &crate::config::BeeBotOSConfig,
) -> Result<(), GatewayError> {
    let path = state
        .config_manager
        .as_ref()
        .and_then(|manager| manager.source_path().cloned())
        .unwrap_or_else(|| PathBuf::from(CONFIG_PATH));
    write_mcp_config(&path, &config.mcp)?;
    if let Some(manager) = &state.config_manager {
        manager
            .reload()
            .await
            .map_err(|e| GatewayError::service_unavailable("Configuration", e.to_string()))?;
    }
    Ok(())
}

async fn connect_and_report(state: &AppState, server: &McpServerConfig) -> McpServerInfo {
    match connect_mcp_client(state, server).await {
        Ok(()) => server_info(state, server).await,
        Err(e) => McpServerInfo {
            name: server.name.clone(),
            connected: false,
            config: response_from_config(server),
            error_message: Some(e.to_string()),
        },
    }
}

async fn connect_mcp_client(
    state: &AppState,
    server: &McpServerConfig,
) -> Result<(), beebotos_agents::mcp::MCPError> {
    let manager = &state.mcp_manager;

    manager.remove_client(&server.name).await;

    let config = current_config(state).await;
    let client_config = beebotos_agents::mcp::ClientConfig {
        server_url: match &server.transport {
            McpTransportConfig::Http { url, .. } => url.clone(),
            _ => "stdio".to_string(),
        },
        timeout_ms: server.timeout_ms.unwrap_or(config.mcp.timeout_ms),
        retry_count: server.retry_count.unwrap_or(config.mcp.retry_count),
    };

    let client = match &server.transport {
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            working_dir,
        } => {
            let stdio_config = beebotos_agents::mcp::StdioTransportConfig {
                command: command.clone(),
                args: args.clone(),
                env: env.clone(),
                working_dir: working_dir.as_ref().map(PathBuf::from),
            };
            beebotos_agents::mcp::MCPClient::connect_stdio_with_policy(
                client_config,
                stdio_config,
                &config.mcp.allowed_commands,
            )
            .await?
        }
        McpTransportConfig::Http {
            url,
            auth_token,
            headers,
            use_sse,
        } => {
            if config.mcp.enforce_tls && !url.to_ascii_lowercase().starts_with("https://") {
                return Err(beebotos_agents::mcp::MCPError::ConnectionFailed(format!(
                    "non-TLS URL '{}' rejected by enforce_tls=true",
                    url
                )));
            }
            let http_config = beebotos_agents::mcp::HttpTransportConfig {
                base_url: url.clone(),
                auth_token: auth_token.clone(),
                headers: headers.clone(),
                timeout_ms: server.timeout_ms.unwrap_or(config.mcp.timeout_ms),
                use_sse: *use_sse,
            };
            beebotos_agents::mcp::MCPClient::connect_http(client_config, http_config).await?
        }
    };
    client.initialize().await?;
    manager.register_client(&server.name, client).await;
    Ok(())
}

async fn server_info(state: &AppState, server: &McpServerConfig) -> McpServerInfo {
    let connected = if let Some(client) = state.mcp_manager.get_client(&server.name).await {
        client.ping().await.is_ok()
    } else {
        false
    };
    McpServerInfo {
        name: server.name.clone(),
        connected,
        config: response_from_config(server),
        error_message: None,
    }
}

fn request_to_config(req: McpServerConfigRequest) -> Result<McpServerConfig, GatewayError> {
    validate_server_name(&req.name)?;
    let transport = match req.transport {
        McpTransportConfigRequest::Stdio {
            command,
            args,
            env,
            working_dir,
        } => {
            if command.trim().is_empty() {
                return Err(GatewayError::bad_request("stdio command is required"));
            }
            McpTransportConfig::Stdio {
                command,
                args,
                env,
                working_dir,
            }
        }
        McpTransportConfigRequest::Http {
            url,
            auth_token,
            headers,
            use_sse,
        } => {
            if url.trim().is_empty() {
                return Err(GatewayError::bad_request("http url is required"));
            }
            McpTransportConfig::Http {
                url,
                auth_token: auth_token
                    .filter(|value| !value.trim().is_empty())
                    .map(SecretString::new),
                headers,
                use_sse,
            }
        }
    };
    Ok(McpServerConfig {
        name: req.name,
        transport,
        timeout_ms: req.timeout_ms,
        retry_count: req.retry_count,
    })
}

fn response_from_config(config: &McpServerConfig) -> McpServerConfigResponse {
    let transport = match &config.transport {
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            working_dir,
        } => McpTransportConfigResponse::Stdio {
            command: command.clone(),
            args: args.clone(),
            env: env.clone(),
            working_dir: working_dir.clone(),
        },
        McpTransportConfig::Http {
            url,
            auth_token,
            headers,
            use_sse,
        } => McpTransportConfigResponse::Http {
            url: url.clone(),
            auth_token_set: auth_token.is_some(),
            headers: headers.clone(),
            use_sse: *use_sse,
        },
    };
    McpServerConfigResponse {
        name: config.name.clone(),
        transport,
        timeout_ms: config.timeout_ms,
        retry_count: config.retry_count,
    }
}

fn validate_server_name(name: &str) -> Result<(), GatewayError> {
    if name.trim().is_empty() {
        return Err(GatewayError::bad_request("MCP server name is required"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(GatewayError::bad_request(
            "MCP server name may only contain letters, numbers, '_' and '-'",
        ));
    }
    Ok(())
}

fn write_mcp_config(path: &Path, mcp: &crate::config::McpConfig) -> Result<(), GatewayError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        GatewayError::service_unavailable("Configuration", format!("Failed to read config: {}", e))
    })?;
    let mut doc = content.parse::<DocumentMut>().map_err(|e| {
        GatewayError::service_unavailable("Configuration", format!("Invalid TOML: {}", e))
    })?;

    let mut table = Table::new();
    table["allowed_commands"] = value(array_from_strings(&mcp.allowed_commands));
    table["auto_init"] = value(mcp.auto_init);
    table["enforce_tls"] = value(mcp.enforce_tls);
    table["retry_count"] = value(i64::from(mcp.retry_count));
    table["timeout_ms"] = value(mcp.timeout_ms as i64);
    for server in &mcp.servers {
        let mut item = table_from_server(server);
        item.set_implicit(false);
        table["servers"].or_insert(Item::ArrayOfTables(Default::default()));
        table["servers"]
            .as_array_of_tables_mut()
            .ok_or_else(|| {
                GatewayError::service_unavailable("Configuration", "Invalid mcp.servers table")
            })?
            .push(item);
    }
    doc["mcp"] = Item::Table(table);
    std::fs::write(path, doc.to_string()).map_err(|e| {
        GatewayError::service_unavailable("Configuration", format!("Failed to write config: {}", e))
    })
}

fn table_from_server(server: &McpServerConfig) -> Table {
    let mut table = Table::new();
    table["name"] = value(server.name.clone());
    if let Some(timeout_ms) = server.timeout_ms {
        table["timeout_ms"] = value(timeout_ms as i64);
    }
    if let Some(retry_count) = server.retry_count {
        table["retry_count"] = value(i64::from(retry_count));
    }
    match &server.transport {
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            working_dir,
        } => {
            table["transport"] = value("stdio");
            table["command"] = value(command.clone());
            table["args"] = value(array_from_strings(args));
            if !env.is_empty() {
                table["env"] = value(inline_table_from_map(env));
            }
            if let Some(working_dir) = working_dir {
                table["working_dir"] = value(working_dir.clone());
            }
        }
        McpTransportConfig::Http {
            url,
            auth_token,
            headers,
            use_sse,
        } => {
            table["transport"] = value("http");
            table["url"] = value(url.clone());
            if let Some(auth_token) = auth_token {
                table["auth_token"] = value(auth_token.expose_secret().to_string());
            }
            if !headers.is_empty() {
                table["headers"] = value(inline_table_from_map(headers));
            }
            table["use_sse"] = value(*use_sse);
        }
    }
    table
}

fn array_from_strings(values: &[String]) -> Array {
    let mut array = Array::default();
    for value in values {
        array.push(value.as_str());
    }
    array
}

fn inline_table_from_map(map: &HashMap<String, String>) -> InlineTable {
    let mut table = InlineTable::default();
    let mut entries = map.iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in entries {
        table.insert(key, value.as_str().into());
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_mcp_config_preserves_unrelated_toml_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("beebotos.toml");
        std::fs::write(
            &path,
            r#"[server]
port = 8000

[mcp]
allowed_commands = []
auto_init = true
enforce_tls = true
retry_count = 3
timeout_ms = 60000

[models]
default_provider = "deepseek"
"#,
        )
        .unwrap();

        let mcp = crate::config::McpConfig {
            servers: vec![McpServerConfig {
                name: "filesystem".to_string(),
                transport: McpTransportConfig::Stdio {
                    command: "npx".to_string(),
                    args: vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-filesystem".to_string(),
                    ],
                    env: HashMap::new(),
                    working_dir: Some("/tmp".to_string()),
                },
                timeout_ms: Some(1000),
                retry_count: None,
            }],
            timeout_ms: 60000,
            retry_count: 3,
            auto_init: true,
            allowed_commands: vec!["npx".to_string()],
            enforce_tls: true,
        };

        write_mcp_config(&path, &mcp).unwrap();
        let updated = std::fs::read_to_string(path).unwrap();

        assert!(updated.contains("[server]"));
        assert!(updated.contains("port = 8000"));
        assert!(updated.contains("[models]"));
        assert!(updated.contains("default_provider = \"deepseek\""));
        assert!(updated.contains("[[mcp.servers]]"));
        assert!(updated.contains("name = \"filesystem\""));
        assert!(updated.contains("working_dir = \"/tmp\""));
    }
}
