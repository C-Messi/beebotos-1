//! Skill Tool Handler
//!
//! Bridges `crate::skills::SkillTool` to `crate::llm::ToolHandler`,
//! enabling LLM native function calling to execute real底层 tools
//! (file_read, file_write, file_glob, process_exec, etc.).

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;
use crate::skills::SkillTool;

/// Adapts a `SkillTool` (底层 tool implementation) to the LLM's
/// `ToolHandler` trait used by `chat_with_tools_react()`.
pub struct SkillToolHandler {
    tool: Box<dyn SkillTool>,
}

impl SkillToolHandler {
    /// Wrap a `SkillTool` instance for use in native function calling.
    pub fn new(tool: Box<dyn SkillTool>) -> Self {
        Self { tool }
    }
}

#[async_trait::async_trait]
impl ToolHandler for SkillToolHandler {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: self.tool.name().to_string(),
                description: Some(self.tool.description().to_string()),
                parameters: self.tool.parameters_schema(),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let params: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid JSON arguments: {}", e))?;
        self.tool.execute(&params).await
    }
}
