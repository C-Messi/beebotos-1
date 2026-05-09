//! Tool Chain Compression Module
//!
//! Provides conditional tool calling and multi-step chain compression.
//! Allows LLM to express conditional logic in a single function call,
//! reducing round-trip latency for multi-step tasks.
//!
//! Example:
//! ```text
//! STEP 1: get_stock_latest_quote|{"symbols":"AAPL"}
//! IF result.price > 180 THEN
//! STEP 2: place_stock_order|{"symbol":"AAPL","side":"buy","qty":"10"}
//! ELSE
//! STEP 2: notify|{"message":"Price not reached"}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A standard tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name / skill ID
    pub name: String,
    /// Parameters as JSON
    pub params: Value,
}

/// Conditional tool call — compresses multi-step reasoning into one expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalToolCall {
    /// Human-readable condition expression (e.g. "latest_price > 180")
    pub condition: String,
    /// Steps to execute if condition is true
    pub if_true: Vec<ToolChainStep>,
    /// Steps to execute if condition is false
    pub if_false: Vec<ToolChainStep>,
    /// Information that must be gathered before evaluating the condition
    pub required_info: Vec<ToolCall>,
}

/// A single step in a tool chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChainStep {
    /// Unconditional tool call
    Call(ToolCall),
    /// Nested conditional
    Conditional(ConditionalToolCall),
    /// Wait for a previous step's output
    Wait { step_ref: String },
}

/// Parsed tool chain ready for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChain {
    /// Chain identifier
    pub id: String,
    /// Sequential steps (some may be conditional)
    pub steps: Vec<ToolChainStep>,
    /// Original user query that generated this chain
    pub source_query: String,
}

/// Result of executing a conditional step
#[derive(Debug, Clone)]
pub enum ChainExecutionResult {
    /// Condition evaluated to true
    TrueBranch(Vec<Value>),
    /// Condition evaluated to false
    FalseBranch(Vec<Value>),
    /// Required info gathered but condition could not be evaluated
    NeedsEvaluation { gathered: Vec<Value> },
    /// Execution error
    Error(String),
}

/// Parser for composite tool call expressions
pub struct ToolChainParser;

impl ToolChainParser {
    /// Parse a simple text-based chain expression
    ///
    /// Format:
    /// ```text
    /// STEP 1: tool_name|{"key":"value"}
    /// IF condition THEN
    /// STEP 2: tool_name|{"key":"value"}
    /// ELSE
    /// STEP 2: tool_name|{"key":"value"}
    /// ```
    pub fn parse(text: &str) -> Result<ToolChain, ToolChainParseError> {
        let mut steps = Vec::new();
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            if line.starts_with("STEP ") {
                // Parse unconditional call
                if let Some(step) = Self::parse_step_line(line) {
                    steps.push(ToolChainStep::Call(step));
                }
            } else if line.starts_with("IF ") && line.contains("THEN") {
                // Parse conditional block
                let (conditional, consumed) = Self::parse_conditional(&lines[i..])?;
                steps.push(ToolChainStep::Conditional(conditional));
                i += consumed;
                continue;
            }
            i += 1;
        }

        if steps.is_empty() {
            return Err(ToolChainParseError::EmptyChain);
        }

        Ok(ToolChain {
            id: format!("chain_{}", uuid::Uuid::new_v4()),
            steps,
            source_query: text.to_string(),
        })
    }

    fn parse_step_line(line: &str) -> Option<ToolCall> {
        // Format: "STEP N: tool_name|{json}"
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        let body = parts[1].trim();
        Self::parse_tool_call(body)
    }

    fn parse_tool_call(body: &str) -> Option<ToolCall> {
        // Format: "tool_name|{json}" or "tool_name|json"
        if let Some(pipe_idx) = body.find('|') {
            let name = body[..pipe_idx].trim().to_string();
            let params_str = body[pipe_idx + 1..].trim();
            let params = serde_json::from_str(params_str).unwrap_or_else(|_| Value::Null);
            Some(ToolCall { name, params })
        } else {
            // No params
            Some(ToolCall {
                name: body.to_string(),
                params: Value::Null,
            })
        }
    }

    fn parse_conditional(
        lines: &[&str],
    ) -> Result<(ConditionalToolCall, usize), ToolChainParseError> {
        if lines.is_empty() {
            return Err(ToolChainParseError::InvalidCondition);
        }

        // First line: "IF condition THEN"
        let first = lines[0];
        let condition = first
            .strip_prefix("IF ")
            .and_then(|s| s.split("THEN").next())
            .map(|s| s.trim().to_string())
            .ok_or(ToolChainParseError::InvalidCondition)?;

        let mut if_true = Vec::new();
        let mut if_false = Vec::new();
        let mut in_else = false;
        let mut consumed = 1;

        while consumed < lines.len() {
            let line = lines[consumed];
            if line.starts_with("IF ") && !in_else && if_true.is_empty() {
                // Nested IF not supported in simple parser
                break;
            }
            if line == "ELSE" {
                in_else = true;
                consumed += 1;
                continue;
            }
            if line.starts_with("STEP ") {
                if let Some(call) = Self::parse_step_line(line) {
                    if in_else {
                        if_false.push(ToolChainStep::Call(call));
                    } else {
                        if_true.push(ToolChainStep::Call(call));
                    }
                }
            } else {
                // End of conditional block
                break;
            }
            consumed += 1;
        }

        Ok((
            ConditionalToolCall {
                condition,
                if_true,
                if_false,
                required_info: Vec::new(),
            },
            consumed,
        ))
    }

    /// Serialize a tool chain back to text format
    pub fn serialize(chain: &ToolChain) -> String {
        let mut output = String::new();
        for (i, step) in chain.steps.iter().enumerate() {
            Self::serialize_step(&mut output, step, i + 1);
        }
        output
    }

    fn serialize_step(output: &mut String, step: &ToolChainStep, step_num: usize) {
        match step {
            ToolChainStep::Call(call) => {
                output.push_str(&format!(
                    "STEP {}: {}|{}\n",
                    step_num,
                    call.name,
                    call.params.to_string()
                ));
            }
            ToolChainStep::Conditional(cond) => {
                output.push_str(&format!("IF {} THEN\n", cond.condition));
                for (j, s) in cond.if_true.iter().enumerate() {
                    Self::serialize_step(output, s, step_num * 10 + j);
                }
                if !cond.if_false.is_empty() {
                    output.push_str("ELSE\n");
                    for (j, s) in cond.if_false.iter().enumerate() {
                        Self::serialize_step(output, s, step_num * 10 + j + 5);
                    }
                }
            }
            ToolChainStep::Wait { step_ref } => {
                output.push_str(&format!("WAIT FOR {}\n", step_ref));
            }
        }
    }
}

/// Errors during tool chain parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolChainParseError {
    EmptyChain,
    InvalidCondition,
    InvalidStepFormat,
    MissingElseBranch,
}

impl std::fmt::Display for ToolChainParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolChainParseError::EmptyChain => write!(f, "Tool chain contains no steps"),
            ToolChainParseError::InvalidCondition => write!(f, "Invalid IF/THEN condition format"),
            ToolChainParseError::InvalidStepFormat => write!(f, "Invalid STEP format"),
            ToolChainParseError::MissingElseBranch => write!(f, "Missing ELSE branch"),
        }
    }
}

impl std::error::Error for ToolChainParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chain() {
        let text = r#"
STEP 1: get_quote|{"symbol":"AAPL"}
IF price > 180 THEN
STEP 2: place_order|{"symbol":"AAPL","side":"buy","qty":"10"}
ELSE
STEP 2: notify|{"message":"Price not reached"}
"#;
        let chain = ToolChainParser::parse(text).unwrap();
        assert_eq!(chain.steps.len(), 2);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let chain = ToolChain {
            id: "test".to_string(),
            steps: vec![
                ToolChainStep::Call(ToolCall {
                    name: "get_quote".to_string(),
                    params: serde_json::json!({"symbol": "AAPL"}),
                }),
                ToolChainStep::Conditional(ConditionalToolCall {
                    condition: "price > 180".to_string(),
                    if_true: vec![ToolChainStep::Call(ToolCall {
                        name: "buy".to_string(),
                        params: serde_json::json!({"qty": "10"}),
                    })],
                    if_false: vec![],
                    required_info: vec![],
                }),
            ],
            source_query: "test".to_string(),
        };
        let serialized = ToolChainParser::serialize(&chain);
        assert!(serialized.contains("get_quote"));
        assert!(serialized.contains("IF price > 180 THEN"));
    }

    #[test]
    fn test_parse_empty_fails() {
        let result = ToolChainParser::parse("");
        assert!(result.is_err());
    }
}
