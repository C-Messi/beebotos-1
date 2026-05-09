//! Prompt Builder
//!
//! Implements Hermes-style dynamic modular assembly and progressive disclosure.

use crate::intent::UserIntent;
use crate::memory::MemoryEntry;

/// Skill description at different disclosure levels
#[derive(Debug, Clone)]
pub enum SkillLevelDesc {
    /// L1: ~30 tokens — name + one-liner
    L1 { id: String, name: String, one_liner: String },
    /// L2: ~200 tokens — summary with key concepts
    L2 { id: String, name: String, summary: String },
    /// L3: ~2000 tokens — full SKILL.md content
    L3 { id: String, name: String, full_doc: String },
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
            SkillLevelDesc::L1 { name, one_liner, .. } => {
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

    /// Build the final prompt string based on intent and model
    pub fn build(self, intent: &UserIntent) -> String {
        let mut parts = Vec::new();
        let c = self.components;

        // 1. Model-specific instructions (first — highest impact)
        if let Some(instr) = c.model_instructions {
            parts.push(instr);
        }

        // 2. Base persona (always loaded, but compressible)
        if let Some(soul) = c.soul {
            parts.push(soul);
        }

        // 3. User profile (L2, always loaded)
        if let Some(profile) = c.user_profile {
            parts.push(format!("[用户偏好]\n{}", profile));
        }

        // 4. Project memory (L1, always loaded)
        if let Some(project) = c.project_memory {
            parts.push(format!("[项目约定]\n{}", project));
        }

        // 5. Dynamic memories (L3, filtered by intent relevance)
        let relevant_memories = Self::filter_memories_by_intent(&c.memories, intent);
        if !relevant_memories.is_empty() {
            let memory_text = relevant_memories
                .iter()
                .map(|m| format!("- {}", m.content))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("[相关记忆]\n{}", memory_text));
        }

        // 5. Skills (progressive loading based on intent)
        let skill_text = Self::build_skills_section(&c.skills, intent);
        if !skill_text.is_empty() {
            parts.push(skill_text);
        }

        // 6. Tool usage guide (on-demand)
        if !matches!(intent, UserIntent::DirectAnswer | UserIntent::MetaQuestion) {
            if !c.tools.is_empty() {
                let tools_text = c.tools
                    .iter()
                    .map(|t| format!("- {}: {}", t.name, t.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                parts.push(format!("[可用工具]\n{}", tools_text));
            }
        }

        // 7. Context files
        for file in c.context_files {
            parts.push(format!("[{}]\n{}", file.name, file.content));
        }

        parts.join("\n\n")
    }

    /// Build prompt with reasoning scratchpad for complex tasks
    pub fn build_with_reasoning(self, intent: &UserIntent) -> String {
        let mut prompt = self.build(intent);
        if matches!(intent, UserIntent::MultiStepPlanning) {
            prompt.push_str("\n\n[推理指南]\n这是一个复杂任务，请按以下步骤思考并在回复中包含 <REASONING_SCRATCHPAD> 标签：\n\
                1. 分析用户目标\n\
                2. 确定需要调用的工具及顺序\n\
                3. 验证每一步的依赖关系\n\
                输出格式：<REASONING_SCRATCHPAD>你的思考过程</REASONING_SCRATCHPAD>\n\
                然后输出实际回答或工具调用。");
        }
        prompt
    }

    fn filter_memories_by_intent<'a>(memories: &'a [MemoryEntry], _intent: &UserIntent) -> Vec<&'a MemoryEntry> {
        // 🆕 SKILL MATCHING V2: Removed hardcoded intent keyword filtering.
        // Memory relevance is now handled by the memory system's own retrieval logic.
        // This function simply returns the top 3 most recent/relevant memories.
        memories.iter().take(3).collect()
    }

    fn build_skills_section(skills: &[SkillLevelDesc], intent: &UserIntent) -> String {
        match intent {
            UserIntent::DirectAnswer => String::new(),
            UserIntent::MetaQuestion => {
                // For meta questions, only show L1 indexes
                let items: Vec<String> = skills
                    .iter()
                    .filter_map(|s| match s {
                        SkillLevelDesc::L1 { .. } => Some(s.to_prompt_text()),
                        _ => None,
                    })
                    .collect();
                if items.is_empty() {
                    String::new()
                } else {
                    format!("[技能目录]\n{}", items.join("\n"))
                }
            }
            UserIntent::SingleToolCall => {
                // For single tool calls, show L1 (name + one-liner)
                let items: Vec<String> = skills
                    .iter()
                    .filter_map(|s| match s {
                        SkillLevelDesc::L1 { .. } => Some(s.to_prompt_text()),
                        _ => None,
                    })
                    .collect();
                if items.is_empty() {
                    String::new()
                } else {
                    format!("[可用技能]\n{}", items.join("\n"))
                }
            }
            UserIntent::MultiStepPlanning => {
                // For complex planning, show L2 (summaries)
                let items: Vec<String> = skills
                    .iter()
                    .filter_map(|s| match s {
                        SkillLevelDesc::L2 { .. } => Some(s.to_prompt_text()),
                        SkillLevelDesc::L1 { .. } => Some(s.to_prompt_text()),
                        _ => None,
                    })
                    .collect();
                if items.is_empty() {
                    String::new()
                } else {
                    format!("[技能详情]\n{}", items.join("\n\n"))
                }
            }
            _ => String::new(),
        }
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
        "You are a BeeBotOS AI assistant using Kimi k2.6. \
        Prefer detailed tool parameter descriptions. \
        Use native function calling format. \
        Be concise in reasoning but thorough in tool usage."
    }

    /// GPT-4 specific optimizations
    pub fn gpt4o() -> &'static str {
        "You are a BeeBotOS AI assistant using GPT-4o. \
        Prefer concise tool descriptions. \
        Use native function calling format. \
        Focus on direct answers."
    }

    /// Claude 3 specific optimizations
    pub fn claude3() -> &'static str {
        "You are a BeeBotOS AI assistant using Claude 3. \
        Use XML tags for structured output when helpful. \
        Be thorough but avoid over-explaining."
    }
}
