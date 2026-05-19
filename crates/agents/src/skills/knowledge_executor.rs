//! Knowledge Skill Executor
//!
//! Executes "knowledge-driven" skills (pure SKILL.md) by loading the
//! markdown document and sending it to the LLM as a system prompt.
//! 🆕 FIX: Most knowledge skills use a single LLM call. However, if the
//! SKILL.md references tools (web_fetch, bash_shell, etc.), the unified
//! ReAct loop is used so the tools are actually executed.

use std::path::Path;
use std::sync::Arc;

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;

/// Executor for knowledge-based skills
pub struct KnowledgeSkillExecutor {
    llm: Arc<dyn LLMCallInterface>,
}

impl KnowledgeSkillExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self { llm }
    }

    /// Execute a knowledge skill.
    /// 🆕 FIX: Pure knowledge skills use a single LLM call.
    /// If the SKILL.md references tools (web_fetch, bash_shell, etc.),
    /// the unified ReAct loop is used instead.
    pub async fn execute(&self, skill_path: &Path, user_input: &str) -> Result<String, AgentError> {
        let (skill_md, skill_name) = if skill_path.is_dir() {
            let md = skill_path.join("SKILL.md");
            let content = tokio::fs::read_to_string(&md)
                .await
                .map_err(|e| AgentError::Execution(format!("Failed to read SKILL.md: {}", e)))?;
            let name = skill_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (content, name)
        } else if skill_path.extension().and_then(|e| e.to_str()) == Some("md") {
            let content = tokio::fs::read_to_string(skill_path)
                .await
                .map_err(|e| AgentError::Execution(format!("Failed to read skill file: {}", e)))?;
            let name = skill_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (content, name)
        } else {
            return Err(AgentError::Execution(
                "SKILL.md not found in skill directory".to_string(),
            ));
        };

        // Detect whether this skill references executable tools
        let needs_tools = Self::skill_needs_tools(&skill_md);

        if needs_tools {
            tracing::info!(
                "Knowledge skill '{}' references tools, using unified ReAct",
                skill_name
            );
            return self
                .execute_with_react(skill_path, &skill_md, &skill_name, user_input)
                .await;
        }

        // 🆕 FIX: Strip SKILL.md down to essential instructions to prevent LLM from
        // outputting skill self-introduction instead of answering the user directly.
        let skill_instructions = crate::skills::code_executor::extract_skill_usage(&skill_md);

        // Build a clean system prompt without ReAct tool-calling overhead
        let system_prompt = format!(
            "You are the '{}' skill. Your ONLY job is to help the user based on the instructions \
             below. Do NOT introduce yourself, describe your capabilities, or explain what you \
             are. Answer the user's request directly and concisely.\n\nSkill instructions:\n{}",
            skill_name, skill_instructions
        );

        let messages = vec![
            CommMessage::new(uuid::Uuid::new_v4(), PlatformType::Custom, system_prompt),
            CommMessage::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                user_input.to_string(),
            ),
        ];

        let response = self.llm.call_llm(messages, None).await.map_err(|e| {
            AgentError::Execution(format!("Knowledge skill LLM call failed: {}", e))
        })?;

        Ok(response)
    }

    /// Check whether the SKILL.md references tools that need execution.
    fn skill_needs_tools(skill_md: &str) -> bool {
        let tool_names = [
            "web_fetch",
            "bash_shell",
            "process_exec",
            "file_list",
            "file_read",
            "file_write",
        ];
        let lower = skill_md.to_lowercase();
        tool_names.iter().any(|t| lower.contains(t))
    }

    /// Execute a knowledge skill that references tools using the unified ReAct
    /// loop.
    async fn execute_with_react(
        &self,
        skill_path: &Path,
        skill_md: &str,
        skill_name: &str,
        user_input: &str,
    ) -> Result<String, AgentError> {
        let skill_dir = if skill_path.is_dir() {
            skill_path.to_path_buf()
        } else {
            skill_path.parent().unwrap_or(skill_path).to_path_buf()
        };

        let skill_instructions = crate::skills::code_executor::extract_skill_usage(skill_md);
        let tools = crate::skills::tool_set::default_tool_set(&skill_dir);
        let tools_prompt = crate::skills::general_react_prompt::build_general_react_prompt(&tools);
        let system_prompt = format!(
            "{tools_prompt}\n\n## Knowledge Skill Context\n\nYou are the '{}' skill executor. \
             Your ONLY job is to help the user by using the available tools according to the \
             instructions below.\n\nSkill instructions:\n{}\n\nIMPORTANT: Execute the necessary \
             tool immediately. Do NOT introduce yourself or describe your capabilities. If a tool \
             is needed, output action=call_tool in the unified ReAct JSON format.",
            skill_name, skill_instructions
        );

        let executor = crate::skills::UnifiedReActExecutor::new(self.llm.clone()).with_config(
            crate::skills::UnifiedReActConfig {
                max_rounds: 6,
                round_timeout_sec: 30,
                tool_timeout_sec: 60,
                max_parse_failures: 3,
                max_duplicate_tool_calls: 2,
                max_consecutive_tool_errors: 3,
                enable_reflection: false,
                require_structured_output: false,
                cancel_rx: None,
                stream_tx: None,
            },
        );

        executor.execute(&system_prompt, user_input, &tools).await
    }
}
