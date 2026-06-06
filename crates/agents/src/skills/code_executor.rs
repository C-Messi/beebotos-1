//! Code Skill Executor
//!
//! Executes "code-driven" skills (SKILL.md + .py/.js/.sh scripts) by
//! loading the markdown document, listing available scripts, and
//! delegating script execution to the unified ReAct executor.
//!
//! 🟢 P1 OPTIMIZE: Single-shot command generation for simple requests
//! avoids the expensive multi-turn ReAct loop (~60s → ~15s).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tracing::{debug, error, info};

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;
use crate::mcp::MCPManager;
use crate::skills::tool_set::{default_tool_set, ProcessExecTool, SkillTool};

/// Executor for code-based skills
pub struct CodeSkillExecutor {
    llm: Arc<dyn LLMCallInterface>,
    mcp_manager: Option<Arc<MCPManager>>,
}

impl CodeSkillExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self {
            llm,
            mcp_manager: None,
        }
    }

    pub fn with_mcp_manager(mut self, mcp_manager: Option<Arc<MCPManager>>) -> Self {
        self.mcp_manager = mcp_manager;
        self
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

        // 🆕 FIX: Build tool set with MCP tools if manager is available
        let mut tools = default_tool_set(&skill_path);
        if let Some(ref mcp_mgr) = self.mcp_manager {
            add_mcp_tools_to_set(mcp_mgr, &mut tools).await;
        }

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
                round_timeout_sec: 1200,
                tool_timeout_sec: 1200,
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

/// 🆕 FIX: Dynamically add MCP tools to the tool set for skill-internal ReAct.
async fn add_mcp_tools_to_set(
    mcp_manager: &Arc<MCPManager>,
    tools: &mut HashMap<String, Box<dyn SkillTool>>,
) {
    // Add mcp_tool_search
    tools.insert(
        "mcp_tool_search".to_string(),
        Box::new(McpToolSearchSkillTool::new(mcp_manager.clone())),
    );

    // Add all available MCP tools as dynamic SkillTools
    let client_names = mcp_manager.list_clients().await;
    for server_name in client_names {
        let client = match mcp_manager.get_client(&server_name).await {
            Some(c) => c,
            None => continue,
        };

        let tools_result = match client.list_tools(None).await {
            Ok(r) => r,
            Err(e) => {
                debug!("Failed to list MCP tools for '{}': {}", server_name, e);
                continue;
            }
        };

        for mcp_tool in tools_result.tools {
            let tool_id = format!("mcp:{}/{}", server_name, mcp_tool.name);
            tools.insert(
                tool_id.clone(),
                Box::new(McpDynamicSkillTool::new(
                    mcp_manager.clone(),
                    server_name.clone(),
                    mcp_tool.name.clone(),
                    mcp_tool.description.clone().unwrap_or_default(),
                    mcp_tool.input_schema.clone(),
                )),
            );
            debug!("Registered MCP tool '{}' for skill ReAct", tool_id);
        }
    }

    info!(
        "MCP tools registered for skill ReAct: {} total (including mcp_tool_search)",
        tools.len()
    );
}

/// 🆕 FIX: mcp_tool_search as a SkillTool for skill-internal ReAct
pub struct McpToolSearchSkillTool {
    mcp_manager: Arc<MCPManager>,
}

impl McpToolSearchSkillTool {
    pub fn new(mcp_manager: Arc<MCPManager>) -> Self {
        Self { mcp_manager }
    }
}

#[async_trait::async_trait]
impl SkillTool for McpToolSearchSkillTool {
    fn name(&self) -> &str {
        "mcp_tool_search"
    }

    fn description(&self) -> &str {
        "Load schema details for a connected MCP tool. Prefer passing the exact catalog name in \
         tool_name, for example mcp:server/tool. You may also search by query when you only know \
         the intent. This returns lightweight matches and dynamically exposes selected MCP tool \
         schemas for the next tool call."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tool_name": { "type": "string", "description": "Exact MCP catalog name in the form mcp:server/tool" },
                "query": { "type": "string", "description": "Natural-language task or capability to search for when the exact tool_name is unknown" },
                "server": { "type": "string", "description": "Optional MCP server name to restrict search" },
                "limit": { "type": "integer", "description": "Maximum tools to return", "default": 5 }
            }
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let tool_name = params["tool_name"].as_str();
        let query = params["query"].as_str();
        let server = params["server"].as_str();
        let limit = params["limit"].as_u64().unwrap_or(5) as usize;

        let summaries = match self.mcp_manager.list_tool_summaries().await {
            Ok(s) => s,
            Err(e) => return Err(format!("MCP tool search failed: {}", e)),
        };

        let mut results = Vec::new();
        for summary in summaries {
            let server_name = &summary.server_name;
            let tool_name_val = &summary.tool_name;

            // Server filter
            if let Some(s) = server {
                if server_name.to_lowercase() != s.to_lowercase() {
                    continue;
                }
            }

            // Exact match
            if let Some(tn) = tool_name {
                let expected = format!("mcp:{}/{}", server_name, tool_name_val);
                if tn == &expected {
                    let schema = match self
                        .mcp_manager
                        .get_tool_schema(server_name, tool_name_val)
                        .await
                    {
                        Ok(t) => t.input_schema,
                        Err(_) => Value::Null,
                    };
                    return Ok(format!(
                        "Exact match: {}\nDescription: {}\nSchema: {}",
                        expected,
                        summary.description.as_deref().unwrap_or("N/A"),
                        schema
                    ));
                }
            }

            // Query match
            if let Some(q) = query {
                let q_lower = q.to_lowercase();
                let desc = summary.description.as_deref().unwrap_or("").to_lowercase();
                if server_name.to_lowercase().contains(&q_lower)
                    || tool_name_val.to_lowercase().contains(&q_lower)
                    || desc.contains(&q_lower)
                {
                    results.push(format!(
                        "- mcp:{}/{}: {}",
                        server_name,
                        tool_name_val,
                        summary.description.as_deref().unwrap_or("N/A")
                    ));
                }
            }
        }

        if results.is_empty() {
            Ok("No MCP tools matched your search.".to_string())
        } else {
            results.truncate(limit.max(1));
            Ok(format!(
                "Found {} MCP tool(s):\n{}",
                results.len(),
                results.join("\n")
            ))
        }
    }
}

/// 🆕 FIX: Dynamic MCP tool wrapper as a SkillTool for skill-internal ReAct
pub struct McpDynamicSkillTool {
    mcp_manager: Arc<MCPManager>,
    server_name: String,
    tool_name: String,
    description: String,
    params_schema: Value,
}

impl McpDynamicSkillTool {
    pub fn new(
        mcp_manager: Arc<MCPManager>,
        server_name: String,
        tool_name: String,
        description: String,
        params_schema: Value,
    ) -> Self {
        Self {
            mcp_manager,
            server_name,
            tool_name,
            description,
            params_schema,
        }
    }
}

#[async_trait::async_trait]
impl SkillTool for McpDynamicSkillTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.params_schema.clone()
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let client = match self.mcp_manager.get_client(&self.server_name).await {
            Some(c) => c,
            None => return Err(format!("MCP client '{}' not found", self.server_name)),
        };

        let args = params.as_object().cloned();
        match client.call_tool(&self.tool_name, args).await {
            Ok(result) => {
                let mut texts = Vec::new();
                for content in &result.content {
                    if let crate::mcp::types::ToolContent::Text { text } = content {
                        texts.push(text.clone());
                    }
                }
                let output = if texts.is_empty() {
                    serde_json::to_string(&result).unwrap_or_default()
                } else {
                    texts.join("\n")
                };
                if output.len() > 4000 {
                    Ok(format!(
                        "{}...[truncated {} chars]",
                        &output[..4000],
                        output.len() - 4000
                    ))
                } else {
                    Ok(output)
                }
            }
            Err(e) => {
                error!(
                    "MCP tool {}::{} failed: {}",
                    self.server_name, self.tool_name, e
                );
                Err(format!("MCP tool error: {}", e))
            }
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
