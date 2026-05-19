//! Code Skill Executor
//!
//! Executes "code-driven" skills (SKILL.md + .py/.js/.sh scripts) by
//! loading the markdown document, listing available scripts, and
//! delegating script execution to the unified ReAct executor.
//!
//! 🟢 P1 OPTIMIZE: Single-shot command generation for simple requests
//! avoids the expensive multi-turn ReAct loop (~60s → ~15s).

use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info};

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;
use crate::skills::tool_set::{default_tool_set, ProcessExecTool, SkillTool};

/// Executor for code-based skills
pub struct CodeSkillExecutor {
    llm: Arc<dyn LLMCallInterface>,
}

impl CodeSkillExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self { llm }
    }

    /// Execute a code skill
    pub async fn execute(&self, skill_path: &Path, user_input: &str) -> Result<String, AgentError> {
        // Normalize to absolute path so prompts and working directories are
        // unambiguous.
        let skill_path = if skill_path.is_absolute() {
            skill_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(skill_path))
                .unwrap_or_else(|_| skill_path.to_path_buf())
        };

        if !skill_path.is_dir() {
            return Err(AgentError::Execution(
                "Code skills must be directory-based".to_string(),
            ));
        }

        let skill_md_path = skill_path.join("SKILL.md");
        let skill_md = if skill_md_path.exists() {
            tokio::fs::read_to_string(&skill_md_path)
                .await
                .map_err(|e| AgentError::Execution(format!("Failed to read SKILL.md: {}", e)))?
        } else {
            return Err(AgentError::Execution(
                "SKILL.md not found in skill directory".to_string(),
            ));
        };

        let scripts = list_scripts(&skill_path).await;
        let scripts_info = if scripts.is_empty() {
            "No executable scripts found in this skill.".to_string()
        } else {
            format!(
                "Available scripts in this skill:\n{}",
                scripts
                    .iter()
                    .map(|(name, path)| format!("  - {} ({})", name, path))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        let skill_dir_str = skill_path.to_string_lossy().to_string();
        // Replace {SKILL_DIR} placeholder with actual path so the LLM generates valid
        // commands
        let skill_md = skill_md.replace("{SKILL_DIR}", &skill_dir_str);

        // 🟢 P1 OPTIMIZE: Try single-shot command generation first.
        // For simple requests (e.g. "run hello.py") this avoids the expensive ReAct
        // loop.
        match self
            .try_single_shot(
                &skill_md,
                &scripts_info,
                &skill_dir_str,
                user_input,
                &skill_path,
            )
            .await
        {
            Ok(result) => {
                info!("Single-shot skill execution succeeded");
                return Ok(result);
            }
            Err(e) => {
                debug!("Single-shot failed ({}), falling back to ReAct", e);
            }
        }

        // Fallback: unified ReAct loop
        // 🆕 FIX: Strip SKILL.md down to essential script usage to prevent LLM from
        // outputting skill self-introduction instead of executing the tool.
        let skill_instructions = extract_skill_usage(&skill_md);
        let tools = default_tool_set(&skill_path);
        let tools_prompt = crate::skills::general_react_prompt::build_general_react_prompt(&tools);
        let system_prompt = format!(
            "{tools_prompt}\n\n## Code Skill Context\n\nYou are the '{}' skill executor. Your \
             ONLY job is to run the appropriate script to fulfill the user's request. Do NOT \
             introduce yourself, describe your capabilities, or explain what you \
             are.\n\n{scripts_info}\n\nWhen constructing commands, use the absolute skill \
             directory path: {skill_dir_str}\n\nScript usage \
             instructions:\n{skill_instructions}\n\nIMPORTANT: If the user has provided enough \
             information, execute the script immediately using the process_exec tool via \
             action=call_tool. Do not ask follow-up questions unless critical information is \
             missing.",
            skill_path.file_name().unwrap_or_default().to_string_lossy(),
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

    /// 🟢 P1 OPTIMIZE: Single-shot command generation.
    ///
    /// Ask the LLM to produce a JSON object with the exact shell command.
    /// If parsing succeeds, execute it directly via ProcessExecTool.
    /// If the LLM signals ambiguity or JSON parsing fails, return Err so
    /// the caller can fall back to ReAct.
    async fn try_single_shot(
        &self,
        skill_md: &str,
        scripts_info: &str,
        skill_dir_str: &str,
        user_input: &str,
        skill_path: &Path,
    ) -> Result<String, AgentError> {
        // 🆕 FIX: Use stripped skill usage to prevent LLM from generating intros
        // instead of commands.
        let skill_instructions = extract_skill_usage(skill_md);
        let prompt = format!(
            "You are a code-skill executor. Your job is to turn the user's request into a single \
             shell command that fulfills it. Do NOT introduce yourself.\n\nScript \
             usage:\n{skill_instructions}\n\n{scripts_info}\n\nWhen constructing commands, use \
             the absolute skill directory path: {skill_dir_str}\n\nUser request: \
             {user_input}\n\nRespond with a JSON object ONLY — no markdown, no explanation \
             outside the JSON:\n{{\"command\":\"the exact shell command to \
             run\",\"working_dir\":\"{skill_dir_str}\",\"reasoning\":\"brief \
             explanation\"}}\n\nIf the request is unclear or missing critical information, \
             respond with:\n{{\"needs_react\":true,\"reasoning\":\"why\"}}"
        );

        let messages = vec![CommMessage::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            prompt,
        )];

        // 🟢 P1 OPTIMIZE: Use one_shot context flag to avoid carrying heavy
        // conversation history into skill command generation.
        let mut context = std::collections::HashMap::new();
        context.insert("one_shot".to_string(), "true".to_string());

        let response = self
            .llm
            .call_llm(messages, Some(context))
            .await
            .map_err(|e| AgentError::Execution(format!("Single-shot LLM call failed: {}", e)))?;

        debug!("Single-shot LLM response: {}", response);

        // Try to extract JSON from the response (LLMs sometimes wrap it in markdown)
        let json_str = extract_json(&response);
        let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
            AgentError::Execution(format!("Failed to parse single-shot JSON: {}", e))
        })?;

        if parsed
            .get("needs_react")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Err(AgentError::Execution(
                "LLM indicated single-shot is insufficient".to_string(),
            ));
        }

        let command = parsed
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AgentError::Execution("Single-shot JSON missing 'command' field".to_string())
            })?;

        // 🟢 P1 OPTIMIZE: Do NOT pass working_dir — let ProcessExecTool use its
        // default (the skill directory). Passing a relative path here causes
        // resolve_working_dir to incorrectly join it with the default dir.
        info!(
            "Single-shot command: {} (skill dir: {})",
            command, skill_dir_str
        );

        // Execute directly via ProcessExecTool
        let tool = ProcessExecTool::new(vec![skill_path.to_path_buf()]);
        let params = serde_json::json!({
            "command": command,
            "timeout_ms": 30000
        });

        match tool.execute(&params).await {
            Ok(output) => Ok(format!("Command executed successfully.\n\n{}", output)),
            Err(e) => Err(AgentError::Execution(format!(
                "Single-shot command failed: {}",
                e
            ))),
        }
    }
}

/// Strip SKILL.md down to script usage blocks and tool descriptions.
/// Removes marketing copy, feature lists, and examples that distract the LLM.
pub fn extract_skill_usage(skill_md: &str) -> String {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut in_usage_section = false;

    for line in skill_md.lines() {
        let trimmed = line.trim();

        // Detect code blocks (bash examples are critical)
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(line.to_string());
            continue;
        }

        if in_code_block {
            lines.push(line.to_string());
            continue;
        }

        // Detect usage/工具使用说明 sections
        if trimmed.to_lowercase().contains("usage")
            || trimmed.to_lowercase().contains("使用说明")
            || trimmed.to_lowercase().contains("工具使用")
            || trimmed.to_lowercase().contains("使用示例")
            || trimmed.starts_with("## ")
            || trimmed.starts_with("### ")
        {
            in_usage_section = true;
            lines.push(line.to_string());
            continue;
        }

        // Skip feature lists, marketing copy, and empty lines outside usage sections
        if in_usage_section {
            // Skip emoji-only lines and decorative separators
            if trimmed.chars().all(|c| {
                c.is_whitespace()
                    || c == '-'
                    || c == '*'
                    || c == '>'
                    || c == '•'
                    || c == '#'
                    || c.is_ascii_punctuation()
            }) {
                continue;
            }
            lines.push(line.to_string());
        }
    }

    let result = lines.join("\n");
    if result.trim().is_empty() {
        // Fallback: return first 2000 chars if no usage section found
        skill_md.chars().take(2000).collect()
    } else {
        result
    }
}

/// Extract the first JSON object from a string, handling markdown fences.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    // Handle ```json ... ``` fences
    if let Some(start) = trimmed.find("```json") {
        if let Some(end) = trimmed[start + 7..].find("```") {
            return trimmed[start + 7..start + 7 + end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        if let Some(end) = trimmed[start + 3..].find("```") {
            return trimmed[start + 3..start + 3 + end].trim();
        }
    }
    // Try to find the first '{' and last '}'
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }
    trimmed
}

/// Scan a directory for script files (.py, .js, .ts, .sh).
async fn scan_dir_for_scripts(dir: &Path) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(ext, "py" | "js" | "sh" | "ts") {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let abs = path.to_string_lossy().to_string();
                result.push((name, abs));
            }
        }
    }
    result
}

/// List all scripts in a skill directory.
/// Checks both the root directory and the `scripts/` subdirectory.
async fn list_scripts(dir: &Path) -> Vec<(String, String)> {
    let mut result = Vec::new();
    // 1. Scan root directory
    result.extend(scan_dir_for_scripts(dir).await);
    // 2. Scan scripts/ subdirectory
    result.extend(scan_dir_for_scripts(&dir.join("scripts")).await);
    result
}
