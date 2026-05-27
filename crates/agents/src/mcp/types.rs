//! MCP Types
//!
//! Core types for Model Context Protocol.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<RequestId>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Request ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
    Null,
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::String(s) => write!(f, "{}", s),
            RequestId::Number(n) => write!(f, "{}", n),
            RequestId::Null => write!(f, "null"),
        }
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        RequestId::String(s.to_string())
    }
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        RequestId::Number(n)
    }
}

impl From<u64> for RequestId {
    fn from(n: u64) -> Self {
        RequestId::Number(n as i64)
    }
}

/// MCP protocol version
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPVersion {
    pub protocol_version: String,
}

impl MCPVersion {
    pub fn current() -> Self {
        Self {
            protocol_version: "2024-11-05".to_string(),
        }
    }
}

/// Initialize request params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

/// Initialize result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Implementation,
}

/// Client capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
}

/// Server capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
}

/// Tools capability
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    pub list_changed: bool,
}

/// Resources capability
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    pub subscribe: bool,
    pub list_changed: bool,
}

/// Prompts capability
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    pub list_changed: bool,
}

/// Implementation info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
}

/// Tool result content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: ResourceContents },
}

/// Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// Resource contents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ResourceContents {
    #[serde(rename = "text")]
    Text {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
    #[serde(rename = "blob")]
    Blob {
        uri: String,
        mime_type: Option<String>,
        blob: String,
    },
}

/// Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Prompt argument
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
}

/// Prompt message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    pub role: Role,
    pub content: PromptContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum PromptContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: ResourceContents },
}

/// List tools result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Call tool params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, Value>>,
}

/// Call tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

/// List resources result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    pub resources: Vec<Resource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Read resource params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceParams {
    pub uri: String,
}

/// Read resource result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContents>,
}

/// List prompts result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResult {
    pub prompts: Vec<Prompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Get prompt params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, String>>,
}

/// Get prompt result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptResult {
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

/// Progress notification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressParams {
    pub progress_token: ProgressToken,
    pub progress: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// Progress token
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    String(String),
    Number(i64),
}

/// Logging level
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// Log message params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogMessageParams {
    pub level: LoggingLevel,
    pub logger: Option<String>,
    pub data: Value,
}

/// Ping request (keepalive)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingParams {}
