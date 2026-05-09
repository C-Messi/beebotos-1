//! MCP Tools

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// MCP Tool registry
pub struct McpToolRegistry {
    tools: Vec<McpTool>,
}

impl McpToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: McpTool) {
        self.tools.push(tool);
    }

    pub fn list(&self) -> &[McpTool] {
        &self.tools
    }

    pub fn get(&self, name: &str) -> Option<&McpTool> {
        self.tools.iter().find(|t| t.name == name)
    }
}

impl Default for McpToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
