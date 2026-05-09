//! ReAct Executor for Knowledge / Code Skills
//!
//! Implements a lightweight ReAct loop using plain-text tool invocations
//! (no function-calling API required). Compatible with any LLM provider
//! that implements `LLMCallInterface`.

use std::collections::HashMap;
use std::sync::Arc;

use regex::Regex;
use tracing::{debug, info, warn};

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;
use crate::skills::tool_set::SkillTool;

/// ReAct executor configuration
pub struct ReActConfig {
    pub max_steps: usize,
    pub stop_phrases: Vec<String>,
    /// Maximum characters of history to retain. Older content is truncated.
    pub max_history_chars: usize,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            stop_phrases: vec!["FINAL ANSWER:".to_string(), "Task completed".to_string()],
            max_history_chars: 8000,
        }
    }
}

/// Parsed tool invocation from LLM text output
#[derive(Debug, Clone)]
pub struct ParsedToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Lightweight ReAct executor
pub struct ReActExecutor {
    llm: Arc<dyn LLMCallInterface>,
    tools: HashMap<String, Box<dyn SkillTool>>,
    config: ReActConfig,
    tool_call_re: Regex,
}

impl ReActExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>, tools: HashMap<String, Box<dyn SkillTool>>) -> Self {
        // Matches:
        // ACTION: tool_name
        // PARAMETERS: {"key": "value"}
        let re = Regex::new(
            r"(?i)ACTION:\s*(?P<name>[a-zA-Z0-9_]+)\s*\nPARAMETERS:\s*(?P<params>\{.*?\})",
        )
        .unwrap();

        Self {
            llm,
            tools,
            config: ReActConfig::default(),
            tool_call_re: re,
        }
    }

    pub fn with_config(mut self, config: ReActConfig) -> Self {
        self.config = config;
        self
    }

    /// Run the ReAct loop until the LLM produces a final answer or max_steps is
    /// reached.
    pub async fn execute(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<String, AgentError> {
        // 🆕 FIX: If the LLM provider supports native function calling, use it
        if self.llm.supports_native_tools() {
            return self.execute_native_tools(system_prompt, user_input).await;
        }

        // Fallback: pure-text ReAct prompt
        let tools_desc = self.render_tools_description();
        let react_instructions = format!(
            "{system_prompt}\n\n{tools_desc}\n\nWhen you need to use a tool, respond **exactly** \
             in this format:\nACTION: <tool_name>\nPARAMETERS: <JSON object>\n\nAfter the tool \
             result is provided, continue reasoning or provide your final answer.\nTo finish, \
             provide a clear final answer without the ACTION header.\n\nFORBIDDEN: Do NOT \
             introduce yourself, describe your capabilities, or say what you are. Do NOT output \
             text like 'I am the ... skill' or 'I can help you with ...'. If you are unsure what \
             to do, call the most relevant tool with your best guess."
        );

        let mut history =
            format!("[System]: {react_instructions}\n\n[User]: {user_input}\n\n[Assistant]: ");

        for step in 0..self.config.max_steps {
            let messages = vec![CommMessage::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                history.clone(),
            )];

            debug!(
                "ReAct step {}/{}, sending prompt ({} chars)",
                step + 1,
                self.config.max_steps,
                history.len()
            );

            let response = self
                .llm
                .call_llm(messages, None)
                .await
                .map_err(|e| AgentError::Execution(format!("LLM call failed: {}", e)))?;

            debug!("LLM response: {}", response);

            // 🆕 FIX: Detect self-introduction responses and treat them as "no tool call"
            // instead of final answers, giving the LLM a chance to retry.
            let looks_like_intro = Self::is_self_introduction(&response);
            let has_stop_phrase = self
                .config
                .stop_phrases
                .iter()
                .any(|p| response.to_uppercase().contains(&p.to_uppercase()));
            let has_tool_call = self.tool_call_re.is_match(&response);

            if !has_tool_call && looks_like_intro && !has_stop_phrase {
                info!(
                    "ReAct step {}: LLM output self-introduction, guiding to retry with tool call",
                    step + 1
                );
                history.push_str(&response);
                history.push_str(
                    "\n\n[System]: Self-introduction is NOT allowed. You MUST call a tool to \
                     fulfill the user's request. Respond exactly with:\nACTION: \
                     <tool_name>\nPARAMETERS: <JSON object>\n\n[Assistant]: ",
                );
                continue;
            }

            // Check for stop phrases indicating final answer
            if self.is_final_answer(&response) {
                info!("ReAct loop terminated by final answer at step {}", step + 1);
                return Ok(self.extract_final_answer(&response));
            }

            // Try to parse a tool call
            if let Some(tool_call) = self.parse_tool_call(&response) {
                info!("Executing tool '{}' at step {}", tool_call.name, step + 1);

                let observation = match self.tools.get(&tool_call.name) {
                    Some(tool) => match tool.execute(&tool_call.arguments).await {
                        Ok(result) => result,
                        Err(e) => format!("Error: {}", e),
                    },
                    None => {
                        let available: Vec<_> = self.tools.keys().cloned().collect();
                        format!(
                            "Error: Tool '{}' not found. Available tools: {:?}",
                            tool_call.name, available
                        )
                    }
                };

                history.push_str(&response);
                history.push_str(&format!(
                    "\n\n[Observation]: {}\n\n[Assistant]: ",
                    observation
                ));

                // Truncate history if it grows beyond the limit to avoid exceeding LLM context
                // windows
                if history.len() > self.config.max_history_chars {
                    let excess = history.len() - self.config.max_history_chars;
                    let split_at = history.find('\n').unwrap_or(0).max(excess);
                    history = history.split_off(split_at.min(history.len()));
                    history.insert_str(0, "...[truncated]\n");
                }
            } else {
                // No tool call detected — ask LLM to retry with correct format
                info!(
                    "No tool call detected at step {}, guiding LLM to retry",
                    step + 1
                );
                history.push_str(&response);
                history.push_str(
                    "\n\n[System]: No tool call detected. If you need a tool, respond exactly \
                     with:\nACTION: <tool_name>\nPARAMETERS: <JSON object>\nOtherwise provide \
                     your final answer directly.\n\n[Assistant]: ",
                );
            }
        }

        warn!(
            "ReAct loop reached max_steps ({}) without final answer",
            self.config.max_steps
        );
        Ok(
            "I reached the maximum number of reasoning steps without a definitive answer."
                .to_string(),
        )
    }

    /// Execute using native function calling API (more reliable than text
    /// parsing)
    async fn execute_native_tools(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<String, AgentError> {
        let tool_defs: Vec<crate::communication::ToolDefinition> = self
            .tools
            .values()
            .map(|t| crate::communication::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect();

        let messages = vec![
            CommMessage::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                system_prompt.to_string(),
            ),
            CommMessage::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                user_input.to_string(),
            ),
        ];

        info!(
            "Using native function calling with {} tools, max {} rounds",
            tool_defs.len(),
            self.config.max_steps
        );

        // For native tools, the LLMCallInterface::call_llm_with_tools handles the loop
        // internally. We cap max rounds via context parameter.
        let mut context = std::collections::HashMap::new();
        context.insert(
            "max_tool_rounds".to_string(),
            self.config.max_steps.to_string(),
        );

        self.llm
            .call_llm_with_tools(messages, tool_defs, Some(context))
            .await
            .map_err(|e| AgentError::Execution(format!("Native tool calling failed: {}", e)))
    }

    fn render_tools_description(&self) -> String {
        let mut lines = vec!["You have access to the following tools:".to_string()];
        for (_, tool) in &self.tools {
            lines.push(format!("- {}: {}", tool.name(), tool.description()));
        }
        lines.join("\n")
    }

    /// Determine whether the LLM response indicates a final answer.
    ///
    /// Returns `true` if an explicit stop phrase is present, or if the
    /// response does not contain a recognizable tool call (treated as a
    /// direct answer).
    fn is_final_answer(&self, text: &str) -> bool {
        let upper = text.to_uppercase();
        self.config
            .stop_phrases
            .iter()
            .any(|p| upper.contains(&p.to_uppercase()))
            || !self.tool_call_re.is_match(text)
    }

    /// Detect whether the LLM is introducing itself instead of acting.
    fn is_self_introduction(text: &str) -> bool {
        let lower = text.to_lowercase();
        let intro_markers = [
            "i am the ",
            "i'm the ",
            "i am a ",
            "i'm a ",
            "i can help you",
            "i can assist you",
            "i am ready to help",
            "i'm ready to help",
            "skill executor",
            "web search skill",
            "my name is",
            "you can call me",
            "i am an ai",
            "i'm an ai",
        ];
        intro_markers.iter().any(|m| lower.contains(m))
    }

    fn extract_final_answer(&self, text: &str) -> String {
        // If there's a "FINAL ANSWER:" marker, extract after it
        if let Some(pos) = text.to_uppercase().find("FINAL ANSWER:") {
            text[pos + 13..].trim().to_string()
        } else {
            text.trim().to_string()
        }
    }

    fn parse_tool_call(&self, text: &str) -> Option<ParsedToolCall> {
        let caps = self.tool_call_re.captures(text)?;
        let name = caps.name("name")?.as_str().to_string();
        let params_str = caps.name("params")?.as_str();
        let arguments: serde_json::Value = serde_json::from_str(params_str).ok()?;
        Some(ParsedToolCall { name, arguments })
    }
}
