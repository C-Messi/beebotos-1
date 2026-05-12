//! Unified ReAct Executor
//!
//! A JSON-format ReAct loop where the LLM autonomously decides each step.
//! The LLM outputs structured JSON with `thought`, `action`, `tool_name`,
//! and `arguments` — or a `final_answer`.
//!
//! This executor is used for:
//! - Investment decision analysis (crypto market data gathering + analysis)
//! - Transaction form submission (multi-turn parameter collection)
//! - Order confirmation flows
//! - Any multi-step task that requires autonomous planning
//!
//! Max rounds: 10 (hard cap). The LLM decides when to terminate via
//! `final_answer`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;
use crate::skills::tool_set::SkillTool;

/// Configuration for the Unified ReAct Executor
#[derive(Debug, Clone)]
pub struct UnifiedReActConfig {
    /// Maximum number of rounds (hard cap). Default: 30.
    pub max_rounds: usize,
    /// Timeout per LLM call in seconds. Default: 30.
    pub round_timeout_sec: u64,
    /// Whether to enable self-reflection on each step. Default: true.
    pub enable_reflection: bool,
    /// Whether the final answer must be valid JSON. Default: true.
    pub require_structured_output: bool,
    /// Optional cancellation receiver. When the watched value becomes true,
    /// the loop terminates early and returns the collected content so far.
    pub cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    /// Optional streaming output channel. When set, the final answer content
    /// is streamed in chunks as it becomes available.
    pub stream_tx: Option<tokio::sync::mpsc::Sender<String>>,
}

impl Default for UnifiedReActConfig {
    fn default() -> Self {
        Self {
            max_rounds: 30,
            round_timeout_sec: 30,
            enable_reflection: true,
            require_structured_output: true,
            cancel_rx: None,
            stream_tx: None,
        }
    }
}

/// A single round in the ReAct loop
#[derive(Debug, Clone)]
pub struct ReActRound {
    pub round_number: usize,
    pub llm_thought: String,
    pub action: ReActAction,
    pub observation: Option<String>,
    pub timestamp: Instant,
}

/// Action taken by the LLM in a round
#[derive(Debug, Clone)]
pub enum ReActAction {
    /// Call a tool to gather data
    CallTool {
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        reasoning: String,
    },
    /// Output the final answer (terminates the loop)
    FinalAnswer { content: String },
}

/// Parsed LLM response
#[derive(Debug, Clone)]
pub struct ParsedReActResponse {
    pub thought: String,
    pub action: ReActAction,
}

/// Unified ReAct executor — LLM-driven autonomous planning and execution
pub struct UnifiedReActExecutor {
    llm: Arc<dyn LLMCallInterface>,
    config: UnifiedReActConfig,
    tool_dispatcher: Option<Arc<dyn ToolDispatcher>>,
}

/// Optional host-side dispatcher for tools that need Agent-level services
/// (for example `skill_call`, which needs the skill registry, MCP manager,
/// approval gate, and pending form state).
#[async_trait::async_trait]
pub trait ToolDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<String, String>;
}

impl UnifiedReActExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self {
            llm,
            config: UnifiedReActConfig::default(),
            tool_dispatcher: None,
        }
    }

    pub fn with_config(mut self, config: UnifiedReActConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_tool_dispatcher(mut self, dispatcher: Arc<dyn ToolDispatcher>) -> Self {
        self.tool_dispatcher = Some(dispatcher);
        self
    }

    /// Execute the ReAct loop.
    ///
    /// # Arguments
    /// * `system_prompt` — The full System Prompt (role + tools + rules + user
    ///   context)
    /// * `user_request` — The user's original input
    /// * `available_tools` — Tools the LLM can call during the loop
    ///
    /// # Returns
    /// The `final_answer.content` string (typically JSON)
    pub async fn execute(
        &self,
        system_prompt: &str,
        user_request: &str,
        available_tools: &HashMap<String, Box<dyn SkillTool>>,
    ) -> Result<String, AgentError> {
        let mut rounds: Vec<ReActRound> = Vec::new();
        let mut messages = Vec::new();

        // Round 0: inject system prompt as the first message
        messages.push(CommMessage::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            system_prompt.to_string(),
        ));

        info!(
            "Starting Unified ReAct loop: max_rounds={}, tools={}",
            self.config.max_rounds,
            available_tools.len()
        );

        for round in 1..=self.config.max_rounds {
            // Check for external cancellation signal
            if let Some(ref rx) = self.config.cancel_rx {
                if *rx.borrow() {
                    info!(
                        "ReAct loop cancelled by user at round {}/{} ({} rounds executed)",
                        round,
                        self.config.max_rounds,
                        rounds.len()
                    );
                    let content = self.build_interrupted_answer(&rounds, user_request);
                    // 🆕 FIX: Stream the interrupted answer so the user sees the summary
                    if let Some(ref stream_tx) = self.config.stream_tx {
                        let _ = stream_tx.send(content.clone()).await;
                    }
                    return Ok(content);
                }
            }

            // Build the round prompt (includes history of all previous rounds)
            let round_prompt = self.build_round_prompt(&rounds, user_request);
            messages.push(CommMessage::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                round_prompt,
            ));

            debug!(
                "ReAct round {}/{}: sending {} messages",
                round,
                self.config.max_rounds,
                messages.len()
            );

            // Call LLM
            let llm_response = match self.llm.call_llm(messages.clone(), None).await {
                Ok(resp) => resp,
                Err(e) => {
                    return Err(AgentError::Execution(format!(
                        "Round {} LLM call failed: {}",
                        round, e
                    )));
                }
            };

            debug!(
                "Round {} LLM raw response: {} chars",
                round,
                llm_response.len()
            );

            // Parse LLM output
            let parsed = match parse_react_response(&llm_response) {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "Round {}: Failed to parse LLM response: {}. Response: {}",
                        round,
                        e,
                        &llm_response[..llm_response.len().min(200)]
                    );
                    // Guide LLM to retry with correct format
                    messages.push(CommMessage::new(
                        uuid::Uuid::new_v4(),
                        PlatformType::Custom,
                        format!(
                            "[System] 输出格式错误: {}。请严格使用 JSON 格式输出，包含 thought, \
                             action, tool_name/arguments 或 final_answer。",
                            e
                        ),
                    ));
                    continue;
                }
            };

            match parsed.action {
                ReActAction::CallTool {
                    tool_name,
                    arguments,
                    reasoning,
                } => {
                    info!(
                        "Round {}/{}: LLM calls tool '{}' ({})",
                        round, self.config.max_rounds, tool_name, reasoning
                    );

                    // Check for duplicate tool calls with identical arguments
                    let is_duplicate = rounds.iter().any(|r| {
                        if let ReActAction::CallTool {
                            tool_name: prev_name,
                            arguments: prev_args,
                            ..
                        } = &r.action
                        {
                            prev_name == &tool_name && prev_args == &arguments
                        } else {
                            false
                        }
                    });

                    let observation = if is_duplicate {
                        warn!(
                            "Duplicate tool call detected: {} with same args. Skipping.",
                            tool_name
                        );
                        format!(
                            "[System Notice] \
                             该工具已在之前的轮次中使用过相同的参数调用过。请避免重复调用，\
                             尝试使用不同的参数或调用其他工具。"
                        )
                    } else {
                        // Execute the tool
                        match self
                            .execute_tool(&tool_name, arguments.clone(), available_tools)
                            .await
                        {
                            Ok(result) => truncate_observation(result),
                            Err(e) => format!("[Error] Tool execution failed: {}", e),
                        }
                    };

                    // Record this round
                    rounds.push(ReActRound {
                        round_number: round,
                        llm_thought: parsed.thought.clone(),
                        action: ReActAction::CallTool {
                            tool_name: tool_name.clone(),
                            arguments: arguments.clone(),
                            reasoning: reasoning.clone(),
                        },
                        observation: Some(observation.clone()),
                        timestamp: Instant::now(),
                    });

                    // Append to messages for next round context
                    messages.push(CommMessage::new(
                        uuid::Uuid::new_v4(),
                        PlatformType::Custom,
                        format!(
                            "```json\n{}\n```",
                            serde_json::to_string(&serde_json::json!({
                                "thought": parsed.thought,
                                "action": "call_tool",
                                "tool_name": tool_name,
                                "arguments": arguments,
                                "reasoning": reasoning,
                            }))
                            .unwrap_or_default()
                        ),
                    ));
                    messages.push(CommMessage::new(
                        uuid::Uuid::new_v4(),
                        PlatformType::Custom,
                        format!(
                            "[Observation] 工具执行结果:\n{}\n\n请基于以上结果，决定下一步操作。",
                            observation
                        ),
                    ));

                    // Optional: reflection step
                    if self.config.enable_reflection && round > 1 {
                        let reflection = format!(
                            "[Reflection] 第 {} 轮已完成。已调用工具: \
                             {}。请回顾：数据是否足够？是否需要调整分析方向？",
                            round,
                            rounds
                                .iter()
                                .filter_map(|r| match &r.action {
                                    ReActAction::CallTool { tool_name, .. } =>
                                        Some(tool_name.as_str()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        messages.push(CommMessage::new(
                            uuid::Uuid::new_v4(),
                            PlatformType::Custom,
                            reflection,
                        ));
                    }

                    continue;
                }

                ReActAction::FinalAnswer { content } => {
                    let content = sanitize_final_answer(&content);
                    if looks_like_internal_output(&content) {
                        warn!(
                            "Round {}: final_answer looked like internal process output; asking \
                             LLM to retry",
                            round
                        );
                        messages.push(CommMessage::new(
                            uuid::Uuid::new_v4(),
                            PlatformType::Custom,
                            "[System] 你的 final_answer 包含思考过程、工具命令或内部 \
                             JSON，不能直接发给用户。请重新输出严格 \
                             JSON：{\"thought\":\"已整理结果\",\"action\":\"final_answer\",\"\
                             content\":\"只包含给用户看的最终答复；如果尚未执行必要工具，请改为 \
                             call_tool。\"}"
                                .to_string(),
                        ));
                        continue;
                    }

                    info!(
                        "ReAct loop terminated by LLM at round {}/{}, total_rounds: {}",
                        round,
                        self.config.max_rounds,
                        rounds.len()
                    );

                    rounds.push(ReActRound {
                        round_number: round,
                        llm_thought: parsed.thought,
                        action: ReActAction::FinalAnswer {
                            content: content.clone(),
                        },
                        observation: None,
                        timestamp: Instant::now(),
                    });

                    // 🆕 STREAMING: If stream_tx is set, stream the final answer in chunks
                    if let Some(ref stream_tx) = self.config.stream_tx {
                        let chars: Vec<char> = content.chars().collect();
                        let chunk_size = 10;
                        for chunk in chars.chunks(chunk_size) {
                            let chunk_str: String = chunk.iter().collect();
                            if stream_tx.send(chunk_str).await.is_err() {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                        }
                    }

                    return Ok(content);
                }
            }
        }

        // Max rounds reached without termination — force final answer
        warn!(
            "ReAct reached max_rounds ({}), forcing final_answer",
            self.config.max_rounds
        );

        messages.push(CommMessage::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            format!(
                "[System] 已达到最大思考轮数（{}轮）。请基于已收集的所有数据，\
                 立即输出最终分析结论（final_answer），不允许再调用工具。",
                self.config.max_rounds
            )
            .to_string(),
        ));

        let forced_response = self
            .llm
            .call_llm(messages, None)
            .await
            .map_err(|e| AgentError::Execution(format!("Forced final_answer failed: {}", e)))?;

        // Try to parse as final_answer
        let final_content = match parse_react_response(&forced_response) {
            Ok(parsed) => {
                if let ReActAction::FinalAnswer { content } = parsed.action {
                    content
                } else {
                    forced_response
                }
            }
            Err(_) => forced_response,
        };

        // 🆕 STREAMING: If stream_tx is set, stream the final answer in chunks
        if let Some(ref stream_tx) = self.config.stream_tx {
            let chars: Vec<char> = final_content.chars().collect();
            let chunk_size = 10;
            for chunk in chars.chunks(chunk_size) {
                let chunk_str: String = chunk.iter().collect();
                if stream_tx.send(chunk_str).await.is_err() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            }
        }

        Ok(final_content)
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
        available_tools: &HashMap<String, Box<dyn SkillTool>>,
    ) -> Result<String, String> {
        if let Some(tool) = available_tools.get(tool_name) {
            let params = serde_json::Value::Object(arguments.clone());
            match tool.execute(&params).await {
                Ok(result) => return Ok(result),
                Err(e)
                    if matches!(tool_name, "skill_call" | "parallel_delegate")
                        && self.tool_dispatcher.is_some() =>
                {
                    warn!(
                        "Descriptor {} returned '{}'; falling back to external dispatcher",
                        tool_name, e
                    );
                }
                Err(e) => return Err(e),
            }
        } else if !matches!(tool_name, "skill_call" | "parallel_delegate") {
            let available: Vec<_> = available_tools.keys().cloned().collect();
            return Err(format!(
                "Tool '{}' not found. Available tools: {:?}",
                tool_name, available
            ));
        }

        if matches!(tool_name, "skill_call" | "parallel_delegate") {
            if let Some(dispatcher) = &self.tool_dispatcher {
                return dispatcher.dispatch(tool_name, arguments).await;
            }
        }

        let available: Vec<_> = available_tools.keys().cloned().collect();
        Err(format!(
            "Tool '{}' not found. Available tools: {:?}",
            tool_name, available
        ))
    }

    /// Build the prompt for a specific round, including history
    fn build_round_prompt(&self, rounds: &[ReActRound], user_request: &str) -> String {
        if rounds.is_empty() {
            return format!(
                "用户请求：{}\n\n请只输出严格 JSON。\n- 如果需要外部信息或执行操作，输出 \
                 action=call_tool，并选择一个真实可用的 tool_name。\n- \
                 如果不需要工具即可回答，输出 action=final_answer。\n- final_answer.content \
                 只能包含发给用户的最终答复，不要包含思考过程、当前状态分析、工具命令、JSON \
                 字段说明或内部执行步骤。",
                user_request
            );
        }

        let mut history = String::new();
        history.push_str("## 已执行的工具调用历史\n\n");

        for round in rounds {
            history.push_str(&format!("### 第 {} 轮\n", round.round_number));
            history.push_str(&format!("Thought: {}\n", round.llm_thought));

            match &round.action {
                ReActAction::CallTool {
                    tool_name,
                    arguments,
                    reasoning,
                } => {
                    history.push_str(&format!(
                        "Action: call_tool({})\nReasoning: {}\nArguments: {}\n",
                        tool_name,
                        reasoning,
                        serde_json::to_string(arguments).unwrap_or_default()
                    ));
                }
                ReActAction::FinalAnswer { .. } => {
                    history.push_str("Action: final_answer\n");
                }
            }

            if let Some(obs) = &round.observation {
                let display = if obs.len() > 2000 {
                    format!("{}...[truncated]", &obs[..2000])
                } else {
                    obs.clone()
                };
                history.push_str(&format!("Observation: {}\n", display));
            }
            history.push('\n');
        }

        history.push_str("## 当前状态\n");
        history.push_str("基于以上已执行的工具调用和返回结果，请决定下一步：\n");
        history.push_str("- 如果还需要更多数据：调用一个工具（call_tool）\n");
        history.push_str("- 如果数据已足够：输出最终分析（final_answer）\n");
        history.push_str("- 如果已达最大轮数限制：必须输出 final_answer\n\n");
        history.push_str(
            "请输出 JSON 格式。final_answer.content 只能包含给用户看的最终答复，不要泄漏 \
             thought、工具命令或内部分析。",
        );

        history
    }

    /// Build an answer when the loop is interrupted by user cancellation.
    /// Summarizes the tools called and their observations into a
    /// natural-language reply.
    fn build_interrupted_answer(&self, rounds: &[ReActRound], _user_request: &str) -> String {
        if rounds.is_empty() {
            return "⏹️ 任务已中断，尚未开始执行。".to_string();
        }

        let mut lines = vec![
            "⏹️ 任务已根据您的指令中断。以下是已执行的操作和收集到的信息：".to_string(),
            String::new(),
        ];

        for round in rounds {
            match &round.action {
                ReActAction::CallTool {
                    tool_name,
                    arguments,
                    reasoning,
                } => {
                    lines.push(format!(
                        "**第 {} 轮** — 调用工具 `{}`",
                        round.round_number, tool_name
                    ));
                    lines.push(format!("- 目的：{}", reasoning));
                    lines.push(format!(
                        "- 参数：{}",
                        serde_json::to_string(arguments).unwrap_or_default()
                    ));
                    if let Some(obs) = &round.observation {
                        let display = if obs.len() > 500 {
                            format!("{}...", &obs[..500])
                        } else {
                            obs.clone()
                        };
                        lines.push(format!("- 结果：{}", display));
                    }
                    lines.push(String::new());
                }
                ReActAction::FinalAnswer { content } => {
                    lines.push(format!(
                        "**第 {} 轮** — 已输出最终答案（部分）",
                        round.round_number
                    ));
                    let display = if content.len() > 500 {
                        format!("{}...", &content[..500])
                    } else {
                        content.clone()
                    };
                    lines.push(display);
                    lines.push(String::new());
                }
            }
        }

        lines.push("---".to_string());
        lines
            .push("由于任务被中断，以上信息可能不完整。如需继续，请重新发送您的请求。".to_string());

        lines.join("\n")
    }
}

/// Parse the LLM's JSON response into a structured ReAct action
pub fn parse_react_response(response: &str) -> Result<ParsedReActResponse, String> {
    // Strategy 1: Extract JSON from markdown code block
    let json_str = extract_json_from_response(response);

    // Strategy 2: Parse as JSON
    let value: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            // Strategy 3: Try to find any JSON object in the text
            if let Some(extracted) = find_json_object(response) {
                match serde_json::from_str(&extracted) {
                    Ok(v) => v,
                    Err(_) => return Err(format!("JSON parse error: {}", e)),
                }
            } else {
                return Err(format!("JSON parse error: {}", e));
            }
        }
    };

    let thought = value
        .get("thought")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let action = value
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'action' field")?;

    match action {
        "call_tool" => {
            let tool_name = value
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'tool_name' field")?
                .to_string();
            let arguments = value
                .get("arguments")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            let reasoning = value
                .get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            Ok(ParsedReActResponse {
                thought,
                action: ReActAction::CallTool {
                    tool_name,
                    arguments,
                    reasoning,
                },
            })
        }
        "final_answer" => {
            let content = match value.get("content") {
                Some(v) => json_value_to_answer_content(v),
                None => return Err("Missing 'content' field in final_answer".to_string()),
            };

            Ok(ParsedReActResponse {
                thought,
                action: ReActAction::FinalAnswer { content },
            })
        }
        _ => Err(format!("Unknown action: '{}'", action)),
    }
}

fn json_value_to_answer_content(value: &serde_json::Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }

    for key in ["answer", "summary", "result", "message", "content"] {
        if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }

    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn sanitize_final_answer(content: &str) -> String {
    let trimmed = content.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return json_value_to_answer_content(&value).trim().to_string();
    }
    trimmed.to_string()
}

fn looks_like_internal_output(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    let internal_markers = [
        "\"action\"",
        "\"thought\"",
        "action:",
        "thought:",
        "tool_name",
        "arguments",
        "当前状态分析",
        "思考过程",
        "系统提示要求",
        "尚未收到具体的执行任务",
        "agent-browser ",
        "```json",
    ];

    internal_markers.iter().any(|marker| lower.contains(marker))
}

fn truncate_observation(result: String) -> String {
    if result.len() > 4000 {
        format!(
            "{}...[truncated {} chars]",
            &result[..4000],
            result.len() - 4000
        )
    } else {
        result
    }
}

/// Extract JSON from markdown code block or plain text
fn extract_json_from_response(response: &str) -> String {
    // Try ```json ... ```
    if let Some(start) = response.find("```json") {
        let after_start = &response[start + 7..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim().to_string();
        }
    }
    // Try ``` ... ```
    if let Some(start) = response.find("```") {
        let after_start = &response[start + 3..];
        if let Some(end) = after_start.find("```") {
            let extracted = after_start[..end].trim();
            if extracted.starts_with('{') || extracted.starts_with('[') {
                return extracted.to_string();
            }
        }
    }
    response.trim().to_string()
}

/// Find the first JSON object in a text block
fn find_json_object(text: &str) -> Option<String> {
    if let Some(start) = text.find('{') {
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;
        let bytes = text.as_bytes();

        for (i, &b) in bytes.iter().enumerate().skip(start) {
            if in_string {
                if escape {
                    escape = false;
                    continue;
                }
                if b == b'\\' {
                    escape = true;
                    continue;
                }
                if b == b'"' {
                    in_string = false;
                }
                continue;
            }

            if b == b'"' {
                in_string = true;
                continue;
            }

            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..=i].to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_call_tool() {
        let json = r#"{"thought": "I need price data", "action": "call_tool", "tool_name": "crypto_price", "arguments": {"symbol": "BTC"}, "reasoning": "Get current price"}"#;
        let parsed = parse_react_response(json).unwrap();
        assert_eq!(parsed.thought, "I need price data");
        match parsed.action {
            ReActAction::CallTool { tool_name, .. } => {
                assert_eq!(tool_name, "crypto_price");
            }
            _ => panic!("Expected CallTool"),
        }
    }

    #[test]
    fn test_parse_final_answer() {
        let json =
            r#"{"thought": "Done", "action": "final_answer", "content": {"verdict": "hold"}}"#;
        let parsed = parse_react_response(json).unwrap();
        match parsed.action {
            ReActAction::FinalAnswer { content } => {
                assert!(content.contains("verdict"));
            }
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_extract_from_codeblock() {
        let text = r#"Some text
```json
{"thought": "test", "action": "final_answer", "content": "result"}
```
More text"#;
        let parsed = parse_react_response(text).unwrap();
        match parsed.action {
            ReActAction::FinalAnswer { content } => {
                assert_eq!(content, "result");
            }
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_find_json_object() {
        let text = r#"Some intro {"key": "value", "nested": {"a": 1}} trailing text"#;
        let found = find_json_object(text).unwrap();
        assert!(found.contains("nested"));
    }
}
