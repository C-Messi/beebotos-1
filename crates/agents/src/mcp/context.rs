//! MCP Context

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// MCP context for tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContext {
    pub session_id: String,
    pub metadata: HashMap<String, String>,
}

impl McpContext {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

impl Default for McpContext {
    fn default() -> Self {
        Self::new("default")
    }
}
