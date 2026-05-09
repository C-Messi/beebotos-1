//! MCP Skill Parameter Extractor
//!
//! Extracts structured parameters from natural language input for MCP tools.
//! Phase 1 of the MCP interactive ordering flow.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tracing::{debug, info, warn};

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;

/// Pending interactive parameter form state.
#[derive(Debug, Clone)]
pub struct PendingParameterForm {
    pub request_id: String,
    pub skill_id: String,
    pub user_input: String,
    pub partial_params: serde_json::Map<String, Value>,
    pub missing_fields: Vec<FieldSchema>,
    pub submitted_at: Instant,
    pub expires_at: Instant,
}

impl PendingParameterForm {
    pub fn new(
        request_id: String,
        skill_id: String,
        user_input: String,
        partial_params: serde_json::Map<String, Value>,
        missing_fields: Vec<FieldSchema>,
    ) -> Self {
        let now = Instant::now();
        Self {
            request_id,
            skill_id,
            user_input,
            partial_params,
            missing_fields,
            submitted_at: now,
            expires_at: now + Duration::from_secs(300), // 5 minutes TTL
        }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Result of parameter extraction from natural language.
#[derive(Debug, Clone)]
pub enum ExtractedParams {
    /// All required parameters successfully extracted.
    Complete(serde_json::Map<String, Value>),
    /// Some parameters extracted, but others need user input.
    Partial {
        partial: serde_json::Map<String, Value>,
        missing: Vec<FieldSchema>,
    },
    /// User intent is ambiguous or cannot be determined.
    Unclear { reason: String },
}

/// Schema for a single parameter field (used for interactive forms).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub description: String,
    pub param_type: String,
    pub required: bool,
}

/// LLM-based parameter extraction engine for MCP tools.
pub struct McpParameterExtractor {
    llm: Arc<dyn LLMCallInterface>,
}

impl McpParameterExtractor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self { llm }
    }

    /// Extract parameters from natural language input based on the tool's JSON
    /// schema.
    ///
    /// # Arguments
    /// * `user_input` - The user's natural language request.
    /// * `tool_schema` - The MCP tool's input JSON schema.
    /// * `tool_name` - The tool name (for prompt context).
    /// * `tool_description` - The tool description (for prompt context).
    pub async fn extract(
        &self,
        user_input: &str,
        tool_schema: &Value,
        tool_name: &str,
        tool_description: &str,
    ) -> Result<ExtractedParams, AgentError> {
        let param_desc = schema_to_parameter_description(tool_schema).unwrap_or_else(|| {
            "No specific parameter schema defined. Extract any relevant values from the user \
             request."
                .to_string()
        });

        let prompt = build_extraction_prompt(user_input, tool_name, tool_description, &param_desc);

        info!(
            "McpParameterExtractor: extracting params for '{}' from input: {}",
            tool_name,
            user_input.chars().take(80).collect::<String>()
        );

        let messages = vec![CommMessage::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            prompt,
        )];

        let response = self.llm.call_llm(messages, None).await.map_err(|e| {
            AgentError::Execution(format!("Parameter extraction LLM call failed: {}", e))
        })?;

        debug!("McpParameterExtractor raw response: {}", response);

        Self::parse_extraction_response(&response, tool_schema)
    }

    /// Parse the LLM extraction response into structured result.
    fn parse_extraction_response(
        response: &str,
        tool_schema: &Value,
    ) -> Result<ExtractedParams, AgentError> {
        // Try to extract JSON from the response (handle markdown code blocks)
        let json_str = Self::extract_json_block(response).unwrap_or(response.trim());

        let parsed: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "Failed to parse extraction response as JSON: {}. Raw: {}",
                    e, response
                );
                return Ok(ExtractedParams::Unclear {
                    reason: format!("Could not parse LLM extraction result: {}", e),
                });
            }
        };

        // Check for special markers
        if let Some(true) = parsed.get("_unclear").and_then(|v| v.as_bool()) {
            let reason = parsed
                .get("_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("User intent is unclear")
                .to_string();
            return Ok(ExtractedParams::Unclear { reason });
        }

        // Collect missing fields
        let missing_names: Vec<String> = parsed
            .get("_missing")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let properties = tool_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();

        let required: Vec<String> = tool_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let mut params = serde_json::Map::new();
        let mut missing_fields = Vec::new();

        for (key, value) in parsed.as_object().unwrap_or(&serde_json::Map::new()) {
            if key.starts_with('_') {
                continue; // skip internal markers
            }
            params.insert(key.clone(), value.clone());
        }

        // Determine which required fields are still missing
        for req in &required {
            if !params.contains_key(req) {
                if let Some(prop) = properties.get(req) {
                    let field = FieldSchema {
                        name: req.clone(),
                        description: prop
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string(),
                        param_type: prop
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("string")
                            .to_string(),
                        required: true,
                    };
                    if !missing_fields
                        .iter()
                        .any(|f: &FieldSchema| f.name == field.name)
                    {
                        missing_fields.push(field);
                    }
                }
            }
        }

        // Also include explicitly reported missing fields
        for missing_name in missing_names {
            if !params.contains_key(&missing_name)
                && !missing_fields.iter().any(|f| f.name == missing_name)
            {
                if let Some(prop) = properties.get(&missing_name) {
                    let field = FieldSchema {
                        name: missing_name,
                        description: prop
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string(),
                        param_type: prop
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("string")
                            .to_string(),
                        required: true,
                    };
                    missing_fields.push(field);
                }
            }
        }

        if missing_fields.is_empty() {
            info!(
                "McpParameterExtractor: complete extraction, params: {:?}",
                params.keys().collect::<Vec<_>>()
            );
            Ok(ExtractedParams::Complete(params))
        } else {
            info!(
                "McpParameterExtractor: partial extraction, got {:?}, missing: {:?}",
                params.keys().collect::<Vec<_>>(),
                missing_fields.iter().map(|f| &f.name).collect::<Vec<_>>()
            );
            Ok(ExtractedParams::Partial {
                partial: params,
                missing: missing_fields,
            })
        }
    }

    /// Extract JSON from markdown code blocks or raw text.
    fn extract_json_block(text: &str) -> Option<&str> {
        // Try fenced code block
        if let Some(start) = text.find("```json") {
            let after_start = &text[start + 7..];
            if let Some(end) = after_start.find("```") {
                return Some(after_start[..end].trim());
            }
        }
        if let Some(start) = text.find("```") {
            let after_start = &text[start + 3..];
            if let Some(end) = after_start.find("```") {
                return Some(after_start[..end].trim());
            }
        }
        // Try finding first { and last }
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                if end > start {
                    return Some(&text[start..=end]);
                }
            }
        }
        None
    }
}

/// Convert a JSON schema's properties into a human-readable parameter
/// description.
fn schema_to_parameter_description(schema: &Value) -> Option<String> {
    let properties = schema.get("properties").and_then(|p| p.as_object())?;
    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut lines = vec![];
    for (name, prop) in properties {
        let ty = prop
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("string");
        let desc = prop
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        let is_req = required.contains(name);
        let enum_vals: Option<String> = prop.get("enum").and_then(|e| e.as_array()).map(|arr| {
            let vals: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            format!(" (allowed values: {})", vals.join(", "))
        });

        lines.push(format!(
            "- {} ({}{}): {}{}",
            name,
            ty,
            if is_req { ", required" } else { ", optional" },
            desc,
            enum_vals.unwrap_or_default()
        ));
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Build the parameter extraction prompt.
fn build_extraction_prompt(
    user_input: &str,
    tool_name: &str,
    tool_description: &str,
    parameter_descriptions: &str,
) -> String {
    format!(
        r#"You are a precise parameter extraction engine. Extract structured parameters from the user's natural language request for the specified tool.

## Tool Information
Name: {}
Description: {}

## Parameter Schema
{}

## User Request
"{}"

## Extraction Rules
1. Extract EXACTLY the fields defined in the schema. Do NOT invent new fields.
2. For required fields:
   - If present in the user request, extract the value.
   - If NOT present, include the field name in the `_missing` array.
3. For optional fields:
   - Only include if explicitly mentioned or clearly implied.
   - If not mentioned, omit the field entirely.
4. Type conversions:
   - Currency amounts like "100 美元", "$100", "100 USD", "不要超过 100 USD" → number: 100
   - "买入", "买", "开多", "开一单" → side: "buy"; "卖出", "卖", "开空" → side: "sell"
   - "BTC", "比特币" → symbol: "BTC/USD" (use standard trading pair format)
   - "ETH", "以太坊" → symbol: "ETH/USD"
5. If the user's intent is completely unclear, set `_unclear` to true and explain why in `_reason`.
6. Do NOT include markdown formatting, explanations, or any text outside the JSON object.

## Output Format
Return ONLY a JSON object. Example outputs:
- Complete: {{"symbol":"BTC/USD","side":"buy","notional":100}}
- Partial: {{"symbol":"BTC/USD","_missing":["side","notional"]}}
- Unclear: {{"_unclear":true,"_reason":"User did not specify what to trade"}}
"#,
        tool_name, tool_description, parameter_descriptions, user_input
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_block() {
        let text = r#"Some text
```json
{"symbol":"BTC/USD","side":"buy"}
```
More text"#;
        assert_eq!(
            McpParameterExtractor::extract_json_block(text),
            Some(r#"{"symbol":"BTC/USD","side":"buy"}"#)
        );

        let text2 = r#"{"symbol":"BTC/USD"}"#;
        assert_eq!(
            McpParameterExtractor::extract_json_block(text2),
            Some(r#"{"symbol":"BTC/USD"}"#)
        );
    }

    #[test]
    fn test_schema_to_parameter_description() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Trading pair symbol" },
                "notional": { "type": "number", "description": "Dollar amount" },
                "side": { "type": "string", "description": "buy or sell", "enum": ["buy", "sell"] }
            },
            "required": ["symbol", "side"]
        });

        let desc = schema_to_parameter_description(&schema).unwrap();
        assert!(desc.contains("symbol (string, required): Trading pair symbol"));
        assert!(desc.contains("notional (number, optional): Dollar amount"));
        assert!(desc.contains("side (string, required): buy or sell (allowed values: buy, sell)"));
    }

    #[test]
    fn test_parse_complete_extraction() {
        let response = r#"{"symbol":"BTC/USD","side":"buy","notional":100}"#;
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string" },
                "notional": { "type": "number" },
                "side": { "type": "string" }
            },
            "required": ["symbol", "side"]
        });

        let result = McpParameterExtractor::parse_extraction_response(response, &schema).unwrap();
        match result {
            ExtractedParams::Complete(params) => {
                assert_eq!(params.get("symbol").unwrap().as_str().unwrap(), "BTC/USD");
                assert_eq!(params.get("side").unwrap().as_str().unwrap(), "buy");
                assert_eq!(params.get("notional").unwrap().as_u64().unwrap(), 100);
            }
            other => panic!("Expected Complete, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_partial_extraction() {
        let response = r#"{"symbol":"BTC/USD","_missing":["side","notional"]}"#;
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Trading pair" },
                "notional": { "type": "number", "description": "Amount" },
                "side": { "type": "string", "description": "Direction" }
            },
            "required": ["symbol", "side", "notional"]
        });

        let result = McpParameterExtractor::parse_extraction_response(response, &schema).unwrap();
        match result {
            ExtractedParams::Partial { partial, missing } => {
                assert_eq!(partial.get("symbol").unwrap().as_str().unwrap(), "BTC/USD");
                assert_eq!(missing.len(), 2);
                assert!(missing.iter().any(|f| f.name == "side"));
                assert!(missing.iter().any(|f| f.name == "notional"));
            }
            other => panic!("Expected Partial, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_unclear_extraction() {
        let response = r#"{"_unclear":true,"_reason":"User did not specify what to trade"}"#;
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string" }
            },
            "required": ["symbol"]
        });

        let result = McpParameterExtractor::parse_extraction_response(response, &schema).unwrap();
        match result {
            ExtractedParams::Unclear { reason } => {
                assert!(reason.contains("did not specify"));
            }
            other => panic!("Expected Unclear, got {:?}", other),
        }
    }
}
