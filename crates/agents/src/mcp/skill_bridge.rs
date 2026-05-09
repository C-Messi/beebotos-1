//! MCP → Skill Bridge
//!
//! Automatically exposes MCP server tools as BeeBotOS skills.
//!
//! Skill ID format: `mcp:{server_name}/{tool_name}`
//!
//! Example: `mcp:filesystem/read_file`

use std::path::PathBuf;

use tracing::{info, warn};

use crate::mcp::types::Tool;
use crate::mcp::{MCPError, MCPManager};
use crate::skills::{
    FunctionDef, FunctionParameter, LoadedSkill, SkillManifest, SkillRegistry, Version,
};

/// Bridges MCP tools into the SkillRegistry.
pub struct McpSkillBridge;

impl McpSkillBridge {
    /// Bridge all MCP servers' tools into the skill registry.
    ///
    /// Iterates over all registered MCP clients, lists their tools,
    /// and registers each tool as a skill with ID `mcp:{server}/{tool}`.
    pub async fn bridge_all(
        mcp_manager: &MCPManager,
        skill_registry: &SkillRegistry,
    ) -> Result<usize, MCPError> {
        let client_names = mcp_manager.list_clients().await;
        if client_names.is_empty() {
            info!("ℹ️ No MCP clients registered, skipping skill bridge");
            return Ok(0);
        }

        let mut total_registered = 0;

        for name in client_names {
            match Self::bridge_server(mcp_manager, &name, skill_registry).await {
                Ok(count) => total_registered += count,
                Err(e) => {
                    warn!("⚠️ Failed to bridge MCP server '{}': {}", name, e);
                }
            }
        }

        info!(
            "✅ MCP Skill Bridge completed: {} tool(s) registered across {} server(s)",
            total_registered,
            mcp_manager.list_clients().await.len()
        );

        Ok(total_registered)
    }

    /// Bridge a single MCP server's tools into the skill registry.
    pub async fn bridge_server(
        mcp_manager: &MCPManager,
        server_name: &str,
        skill_registry: &SkillRegistry,
    ) -> Result<usize, MCPError> {
        let client = mcp_manager.get_client(server_name).await.ok_or_else(|| {
            MCPError::ConnectionFailed(format!("Client '{}' not found", server_name))
        })?;

        let tools_result = client.list_tools(None).await?;
        let mut registered = 0;

        for tool in tools_result.tools {
            let skill_id = format!("mcp:{}/{}", server_name, tool.name);
            let loaded_skill = Self::tool_to_skill(&skill_id, server_name, &tool);

            let mut tags = vec![server_name.to_string(), "mcp".to_string()];
            let tool_name_lower = tool.name.to_lowercase();
            if tool_name_lower.contains("order")
                || tool_name_lower.contains("buy")
                || tool_name_lower.contains("sell")
                || tool_name_lower.contains("place")
            {
                tags.push("trading".to_string());
            }
            if tool_name_lower.contains("crypto")
                || tool_name_lower.contains("btc")
                || tool_name_lower.contains("eth")
            {
                tags.push("crypto".to_string());
                tags.push("cryptocurrency".to_string());
            }
            if tool_name_lower.contains("stock") || tool_name_lower.contains("equity") {
                tags.push("stock".to_string());
                tags.push("equity".to_string());
            }
            if tool_name_lower.contains("quote")
                || tool_name_lower.contains("price")
                || tool_name_lower.contains("snapshot")
                || tool_name_lower.contains("bar")
                || tool_name_lower.contains("trade")
            {
                tags.push("market-data".to_string());
            }
            if tool_name_lower.contains("weather")
                || tool_name_lower.contains("forecast")
                || tool_name_lower.contains("temperature")
            {
                tags.push("weather".to_string());
            }

            skill_registry.register(loaded_skill, "mcp", tags).await;

            registered += 1;
            info!(
                "🔧 Registered MCP skill '{}' from server '{}'",
                skill_id, server_name
            );
        }

        Ok(registered)
    }

    /// Convert an MCP Tool into a BeeBotOS LoadedSkill.
    fn tool_to_skill(skill_id: &str, server_name: &str, tool: &Tool) -> LoadedSkill {
        let description = tool.description.clone().unwrap_or_default();

        // 🆕 FIX: Enrich description with parameter hints so keyword matching
        // can surface tools even when the main docstring is short.
        let mut rich_description = description.clone();
        if let Some(props) = tool
            .input_schema
            .get("properties")
            .and_then(|p| p.as_object())
        {
            for (name, prop) in props {
                if let Some(desc) = prop.get("description").and_then(|d| d.as_str()) {
                    if !desc.is_empty() {
                        rich_description.push_str(&format!(" {}: {}.", name, desc));
                    }
                }
            }
        }

        // 🆕 FIX: Append Chinese keywords for CJK query matching.
        let tool_name_lower = tool.name.to_lowercase();
        if tool_name_lower.contains("order") || tool_name_lower.contains("place") {
            rich_description.push_str(" 下单 购买 买入 卖出 buy sell order trade trading");
        }
        if tool_name_lower.contains("crypto") {
            rich_description.push_str(" 加密货币 比特币 BTC 以太坊 ETH cryptocurrency");
        }
        if tool_name_lower.contains("stock") {
            rich_description.push_str(" 股票 stock equity shares AAPL TSLA");
        }
        if tool_name_lower.contains("quote")
            || tool_name_lower.contains("price")
            || tool_name_lower.contains("snapshot")
            || tool_name_lower.contains("bar")
            || tool_name_lower.contains("trade")
        {
            rich_description.push_str(" 行情 价格 price quote snapshot market data");
        }

        // Convert JSON Schema input_schema into FunctionDef parameters
        let functions = vec![Self::schema_to_function(
            &tool.name,
            &description,
            &tool.input_schema,
        )];

        let manifest = SkillManifest {
            id: skill_id.to_string(),
            name: tool.name.clone(),
            version: Version::new(1, 0, 0),
            description: format!(
                "MCP tool '{}' from server '{}'. {}",
                tool.name, server_name, rich_description
            ),
            author: format!("mcp:{}", server_name),
            capabilities: vec!["mcp".to_string(), "tool".to_string()],
            permissions: vec![],
            entry_point: tool.name.clone(),
            license: "MCP".to_string(),
            functions,
            prompt_template: Self::build_prompt_template(
                &tool.name,
                &description,
                &tool.input_schema,
            ),
            examples: String::new(),
            when_to_use: format!(
                "Use this skill when you need to call the MCP tool '{}' from server '{}'",
                tool.name, server_name
            ),
            when_not_to_use: None,
            activation_examples: vec![format!("Call the {} tool", tool.name)],
            activation_negative_examples: vec![],
            dependencies: vec![],
        };

        LoadedSkill {
            id: skill_id.to_string(),
            name: tool.name.clone(),
            version: Version::new(1, 0, 0),
            wasm_path: PathBuf::new(),
            source_path: PathBuf::new(),
            manifest,
        }
    }

    /// Convert a JSON Schema object into a FunctionDef with parameters.
    fn schema_to_function(
        name: &str,
        description: &str,
        schema: &serde_json::Value,
    ) -> FunctionDef {
        let inputs = Self::extract_parameters(schema);

        FunctionDef {
            name: name.to_string(),
            description: description.to_string(),
            inputs,
            outputs: vec![FunctionParameter {
                name: "result".to_string(),
                param_type: "string".to_string(),
                required: false,
                description: "Tool execution result".to_string(),
                default_value: String::new(),
            }],
            example: String::new(),
        }
    }

    /// Extract FunctionParameter list from a JSON Schema properties object.
    fn extract_parameters(schema: &serde_json::Value) -> Vec<FunctionParameter> {
        let mut params = Vec::new();

        let properties = match schema.get("properties") {
            Some(serde_json::Value::Object(props)) => props,
            _ => return params,
        };

        let required: Vec<String> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        for (name, prop) in properties {
            let param_type = prop
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("string");

            let description = prop
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();

            let default_value = prop
                .get("default")
                .map(|d| d.to_string())
                .unwrap_or_default();

            params.push(FunctionParameter {
                name: name.clone(),
                param_type: param_type.to_string(),
                required: required.contains(name),
                description,
                default_value,
            });
        }

        params
    }

    /// Build a prompt template for LLM fallback execution.
    ///
    /// This helps the agent understand how to use the MCP tool
    /// when the skill is invoked without explicit parameters.
    fn build_prompt_template(name: &str, description: &str, schema: &serde_json::Value) -> String {
        let mut template = format!(
            "You are using the MCP tool '{}'.\n\nDescription: {}\n\n",
            name, description
        );

        let params = Self::extract_parameters(schema);
        if !params.is_empty() {
            template.push_str("Parameters:\n");
            for p in &params {
                template.push_str(&format!(
                    "- {} ({}{}): {}\n",
                    p.name,
                    p.param_type,
                    if p.required {
                        ", required"
                    } else {
                        ", optional"
                    },
                    p.description
                ));
            }
            template.push_str("\n");
        }

        template.push_str(
            "Instructions: Call this tool with the appropriate parameters based on the user's \
             request. ",
        );
        template.push_str(
            "When calling, output the skill ID followed by a '|' and a JSON object with the \
             parameters. ",
        );
        template
            .push_str("Example: SKILL:my_skill|{\"param1\":\"value1\",\"param2\":\"value2\"}. ");
        template.push_str("If no parameters are needed, use SKILL:my_skill|{}.");
        template
    }
}

/// Validate tool arguments against a JSON Schema.
///
/// Performs basic validation: checks that all required fields are present
/// and that each provided field matches the expected JSON type.
/// Returns `Ok(())` if valid, or an error message describing the issue.
pub fn validate_tool_arguments(
    schema: &serde_json::Value,
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let properties = match schema.get("properties") {
        Some(serde_json::Value::Object(props)) => props,
        _ => {
            // No schema defined; allow anything
            return Ok(());
        }
    };

    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Check required fields
    for req in &required {
        if !arguments.contains_key(req) {
            return Err(format!("Missing required parameter: '{}'", req));
        }
    }

    // Check that each provided argument has a matching property in the schema
    for (arg_name, arg_value) in arguments {
        if !properties.contains_key(arg_name) {
            return Err(format!("Unknown parameter: '{}'", arg_name));
        }

        // Basic type check
        if let Some(prop) = properties.get(arg_name) {
            if let Some(expected_type) = prop.get("type").and_then(|t| t.as_str()) {
                let actual_type = json_type_name(arg_value);
                if !types_compatible(expected_type, &actual_type) {
                    return Err(format!(
                        "Parameter '{}' expected type '{}' but got '{}'",
                        arg_name, expected_type, actual_type
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Return the JSON Schema type name for a serde_json::Value.
fn json_type_name(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => "integer".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

/// Check if two JSON Schema types are compatible.
fn types_compatible(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    // "number" accepts both "number" and "integer"
    if expected == "number" && actual == "integer" {
        return true;
    }
    false
}

/// Parse an MCP skill ID into (server_name, tool_name).
///
/// Format: `mcp:{server_name}/{tool_name}`
///
/// Examples:
/// - `mcp:filesystem/read_file` → ("filesystem", "read_file")
/// - `mcp:github/create_issue` → ("github", "create_issue")
pub fn parse_mcp_skill_id(skill_id: &str) -> Option<(&str, &str)> {
    let stripped = skill_id.strip_prefix("mcp:")?;
    let slash_pos = stripped.find('/')?;
    Some((&stripped[..slash_pos], &stripped[slash_pos + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_skill_id() {
        assert_eq!(
            parse_mcp_skill_id("mcp:filesystem/read_file"),
            Some(("filesystem", "read_file"))
        );
        assert_eq!(
            parse_mcp_skill_id("mcp:github/create_issue"),
            Some(("github", "create_issue"))
        );
        assert_eq!(parse_mcp_skill_id("filesystem/read_file"), None);
        assert_eq!(parse_mcp_skill_id("mcp:invalid"), None);
    }

    #[test]
    fn test_validate_tool_arguments_ok() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["path"]
        });

        let mut args = serde_json::Map::new();
        args.insert(
            "path".to_string(),
            serde_json::Value::String("/tmp".to_string()),
        );
        args.insert("limit".to_string(), serde_json::Value::Number(10.into()));

        assert!(validate_tool_arguments(&schema, &args).is_ok());
    }

    #[test]
    fn test_validate_tool_arguments_missing_required() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        });

        let args = serde_json::Map::new();
        let result = validate_tool_arguments(&schema, &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_validate_tool_arguments_unknown_param() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            }
        });

        let mut args = serde_json::Map::new();
        args.insert(
            "unknown".to_string(),
            serde_json::Value::String("x".to_string()),
        );

        let result = validate_tool_arguments(&schema, &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown parameter"));
    }

    #[test]
    fn test_validate_tool_arguments_type_mismatch() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            }
        });

        let mut args = serde_json::Map::new();
        args.insert(
            "count".to_string(),
            serde_json::Value::String("not-a-number".to_string()),
        );

        let result = validate_tool_arguments(&schema, &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected type"));
    }

    #[test]
    fn test_validate_tool_arguments_empty_schema() {
        let schema = serde_json::json!({});
        let args = serde_json::Map::new();
        assert!(validate_tool_arguments(&schema, &args).is_ok());
    }

    #[test]
    fn test_extract_parameters_from_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to read"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max lines to read",
                    "default": 100
                }
            },
            "required": ["path"]
        });

        let params = McpSkillBridge::extract_parameters(&schema);
        assert_eq!(params.len(), 2);

        // Find params by name to avoid relying on HashMap iteration order
        let path_param = params
            .iter()
            .find(|p| p.name == "path")
            .expect("path param");
        assert_eq!(path_param.param_type, "string");
        assert!(path_param.required);

        let limit_param = params
            .iter()
            .find(|p| p.name == "limit")
            .expect("limit param");
        assert_eq!(limit_param.param_type, "integer");
        assert!(!limit_param.required);
        assert_eq!(limit_param.default_value, "100");
    }
}
