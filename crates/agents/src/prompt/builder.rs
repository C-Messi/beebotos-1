//! Prompt Builder
//!
//! Implements Hermes-style dynamic modular assembly and progressive disclosure.

use crate::memory::MemoryEntry;

/// Skill description at different disclosure levels
#[derive(Debug, Clone)]
pub enum SkillLevelDesc {
    /// L1: ~30 tokens — name + one-liner
    L1 {
        id: String,
        name: String,
        one_liner: String,
    },
    /// L2: ~200 tokens — summary with key concepts
    L2 {
        id: String,
        name: String,
        summary: String,
    },
    /// L3: ~2000 tokens — full SKILL.md content
    L3 {
        id: String,
        name: String,
        full_doc: String,
    },
}

impl SkillLevelDesc {
    pub fn id(&self) -> &str {
        match self {
            SkillLevelDesc::L1 { id, .. } => id,
            SkillLevelDesc::L2 { id, .. } => id,
            SkillLevelDesc::L3 { id, .. } => id,
        }
    }

    pub fn to_prompt_text(&self) -> String {
        match self {
            SkillLevelDesc::L1 {
                name, one_liner, ..
            } => {
                format!("- {}: {}", name, one_liner)
            }
            SkillLevelDesc::L2 { name, summary, .. } => {
                format!("## {}\n{}", name, summary)
            }
            SkillLevelDesc::L3 { name, full_doc, .. } => {
                format!("# {}\n{}", name, full_doc)
            }
        }
    }
}

/// Components that make up a system prompt
#[derive(Debug, Clone, Default)]
pub struct PromptComponents {
    /// Base persona (SOUL.md)
    pub soul: Option<String>,
    /// User profile (USER.md) — L2 memory
    pub user_profile: Option<String>,
    /// Project memory (MEMORY.md) — L1 memory
    pub project_memory: Option<String>,
    /// Dynamic memories (L3 retrieved context)
    pub memories: Vec<MemoryEntry>,
    /// Skills at different levels
    pub skills: Vec<SkillLevelDesc>,
    /// Available tools
    pub tools: Vec<ToolDefinition>,
    /// Model-specific instructions
    pub model_instructions: Option<String>,
    /// Context files
    pub context_files: Vec<ContextFile>,
    /// Current LLM provider/model
    pub model: String,
}

/// Tool definition for prompt injection
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Context file entry
#[derive(Debug, Clone)]
pub struct ContextFile {
    pub name: String,
    pub content: String,
}

/// Prompt builder for dynamic assembly
#[derive(Debug, Clone)]
pub struct PromptBuilder {
    components: PromptComponents,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            components: PromptComponents::default(),
        }
    }

    pub fn with_soul(mut self, soul: impl Into<String>) -> Self {
        self.components.soul = Some(soul.into());
        self
    }

    pub fn with_user_profile(mut self, profile: impl Into<String>) -> Self {
        self.components.user_profile = Some(profile.into());
        self
    }

    pub fn with_project_memory(mut self, memory: impl Into<String>) -> Self {
        self.components.project_memory = Some(memory.into());
        self
    }

    pub fn with_memories(mut self, memories: Vec<MemoryEntry>) -> Self {
        self.components.memories = memories;
        self
    }

    pub fn with_skills(mut self, skills: Vec<SkillLevelDesc>) -> Self {
        self.components.skills = skills;
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.components.tools = tools;
        self
    }

    pub fn with_model_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.components.model_instructions = Some(instructions.into());
        self
    }

    pub fn with_context_files(mut self, files: Vec<ContextFile>) -> Self {
        self.components.context_files = files;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.components.model = model.into();
        self
    }

    /// 🆕 Build the final prompt for unified ReAct mode.
    /// All skills (L1+L2), all tools, and all memories are injected.
    /// No intent-based filtering.
    pub fn build_unified_react(self) -> String {
        let mut parts = Vec::new();
        let c = self.components;

        // 1. Model-specific instructions (first — highest impact)
        if let Some(instr) = c.model_instructions {
            parts.push(instr);
        }

        // 2. Base persona (always loaded)
        if let Some(soul) = c.soul {
            parts.push(soul);
        }

        // 3. User profile (L2 memory, always loaded)
        if let Some(profile) = c.user_profile {
            parts.push(format!("[用户偏好]\n{}", profile));
        }

        // 4. Project memory (L1 memory, always loaded)
        if let Some(project) = c.project_memory {
            parts.push(format!("[项目约定]\n{}", project));
        }

        // 5. Dynamic memories (all, no intent filtering)
        if !c.memories.is_empty() {
            let memory_text = c.memories
                .iter()
                .take(10)
                .map(|m| format!("- {}", m.content))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("[相关记忆]\n{}", memory_text));
        }

        // 6. Skills (L1/L2 hierarchical — all skills)
        let skill_text = Self::build_hierarchical_skills(&c.skills);
        if !skill_text.is_empty() {
            parts.push(skill_text);
        }

        // 7. Tools (all, no intent filtering)
        if !c.tools.is_empty() {
            let tools_text = c
                .tools
                .iter()
                .map(|t| format!("- {}: {}\n  参数: {}",
                    t.name, t.description,
                    serde_json::to_string(&t.parameters).unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("[可用工具]\n{}", tools_text));
        }

        // 8. Context files
        for file in c.context_files {
            parts.push(format!("[{}]\n{}", file.name, file.content));
        }

        // 9. Unified ReAct rules
        parts.push(build_unified_react_rules().to_string());

        parts.join("\n\n")
    }

    /// Build the final prompt string based on intent and model (legacy, kept for compatibility)
    #[deprecated(note = "Use build_unified_react instead")]
    pub fn build(self, _intent: &crate::intent::UserIntent) -> String {
        let mut parts = Vec::new();
        let c = self.components;

        if let Some(instr) = c.model_instructions {
            parts.push(instr);
        }
        if let Some(soul) = c.soul {
            parts.push(soul);
        }
        if let Some(profile) = c.user_profile {
            parts.push(format!("[用户偏好]\n{}", profile));
        }
        if let Some(project) = c.project_memory {
            parts.push(format!("[项目约定]\n{}", project));
        }
        if !c.memories.is_empty() {
            let memory_text = c.memories
                .iter()
                .take(3)
                .map(|m| format!("- {}", m.content))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("[相关记忆]\n{}", memory_text));
        }
        let skill_text = Self::build_hierarchical_skills(&c.skills);
        if !skill_text.is_empty() {
            parts.push(skill_text);
        }
        if !c.tools.is_empty() {
            let tools_text = c
                .tools
                .iter()
                .map(|t| format!("- {}: {}", t.name, t.description))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("[可用工具]\n{}", tools_text));
        }
        for file in c.context_files {
            parts.push(format!("[{}]\n{}", file.name, file.content));
        }

        parts.join("\n\n")
    }

    /// 🆕 Build hierarchical skills section (L1 index + L2 summaries)
    /// All skills are included; L3 is injected on-demand during ReAct rounds.
    fn build_hierarchical_skills(skills: &[SkillLevelDesc]) -> String {
        let mut l1_items = Vec::new();
        let mut l2_sections = Vec::new();

        for skill in skills {
            match skill {
                SkillLevelDesc::L1 { .. } => l1_items.push(skill.to_prompt_text()),
                SkillLevelDesc::L2 { .. } => l2_sections.push(skill.to_prompt_text()),
                SkillLevelDesc::L3 { .. } => {} // L3 not injected by default
            }
        }

        let mut parts = Vec::new();

        if !l1_items.is_empty() {
            parts.push(format!(
                "## 技能目录（L1）\n以下是你可使用的所有技能。如需了解某个技能的详细用法，\
                 参考下方的 L2 摘要；如需完整文档（L3），可在 thought 中说明「需要 skill_id 的详细文档」，\
                 系统会在下一轮追加。\n{}",
                l1_items.join("\n")
            ));
        }

        if !l2_sections.is_empty() {
            parts.push(format!(
                "## 技能摘要（L2）\n{}",
                l2_sections.join("\n\n")
            ));
        }

        parts.join("\n\n")
    }

    #[allow(dead_code)]
    fn filter_memories_by_intent<'a>(
        memories: &'a [MemoryEntry],
        _intent: &crate::intent::UserIntent,
    ) -> Vec<&'a MemoryEntry> {
        memories.iter().take(3).collect()
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Model-specific instruction presets
pub mod model_presets {
    /// Kimi k2.6 specific optimizations
    pub fn kimi_k26() -> &'static str {
        "You are a BeeBotOS AI assistant using Kimi k2.6. Prefer detailed tool parameter \
         descriptions. Use native function calling format. Be concise in reasoning but thorough in \
         tool usage."
    }

    /// GPT-4 specific optimizations
    pub fn gpt4o() -> &'static str {
        "You are a BeeBotOS AI assistant using GPT-4o. Prefer concise tool descriptions. Use \
         native function calling format. Focus on direct answers."
    }

    /// Claude 3 specific optimizations
    pub fn claude3() -> &'static str {
        "You are a BeeBotOS AI assistant using Claude 3. Use XML tags for structured output when \
         helpful. Be thorough but avoid over-explaining."
    }
}

/// 🆕 Unified ReAct system rules appended to every unified-react prompt
fn build_unified_react_rules() -> &'static str {
    r#"## ReAct 工作模式

你通过多轮工具调用自主收集信息、执行操作，最终完成用户交给你的任务。

### 每轮输出格式（严格 JSON）

中间轮次（调用工具）：
```json
{
  "thought": "你的思考过程",
  "action": "call_tool",
  "tool_name": "工具名",
  "arguments": {"参数": "值"},
  "reasoning": "调用该工具的目的"
}
```

最终轮次（输出结果）：
```json
{
  "thought": "数据已足够，任务已完成",
  "action": "final_answer",
  "content": "最终回复内容"
}
```

### 关键规则

1. **自主决策**：不需要调用所有工具。根据任务需要选择性调用。
2. **避免重复**：不要重复调用相同工具（相同参数）。
3. **条件分支**：如果某轮结果已足够做出判断，可以提前终止。
4. **错误处理**：如果工具返回错误，尝试替代方案或跳过，在最终回复中说明。
5. **最多 30 轮**：你可以在 1-30 轮之间的任意时刻终止。
6. **禁止过度思考**：简单问题 1-2 轮即可结束。
7. **需要实时数据时必须调用工具**，不要用 final_answer 伪造已执行的搜索、查询、下单。
8. **遇到天气、行情、账户、持仓、下单等业务能力时**，优先调用 `skill_call`，用 `skill_id` 指定注册技能或 MCP 技能。
9. **BTC/ETH 等加密货币交易任务**必须优先使用 Alpaca MCP 技能。
10. **用户要求搜索/网上查时**必须先调用搜索工具；如果所有工具失败，final_answer 必须明确说明未能完成实时联网验证。
11. **如需某个 skill 的 L3 完整文档**，可在 thought 中说明「需要 {skill_id} 的详细文档」，系统会在下一轮自动追加。
12. **final_answer.content 只能写给用户看的最终答复**，禁止包含 thought、action、工具命令或内部执行过程。
"#}

