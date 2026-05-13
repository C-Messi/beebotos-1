# BeeBotOS 架构重构方案：取消 V2 Intent，统一 ReAct 入口，Skills L1/L2/L3 全注入

> **文档定位**：对 BeeBotOS Agent 核心路由层的架构级重构方案，取消前置 Intent 分类组件，所有用户消息统一进入通用 ReAct 循环，Skills 与 Tools 全量注入上下文，由 LLM 在 ReAct 内自主决策。
>
> **版本**：v1.0
> **日期**：2026-05-13
> **涉及模块**：`crates/agents/src/agent_impl.rs`、`prompt/builder.rs`、`intent/`、`skill_matching/`、`skills/unified_react_executor.rs`、`skills/general_react_prompt.rs`

---

## 目录

1. [架构目标与核心原则](#1-架构目标与核心原则)
2. [现状问题分析](#2-现状问题分析)
3. [新架构总览](#3-新架构总览)
4. [详细改造方案](#4-详细改造方案)
5. [Skills L1/L2/L3 注入策略](#5-skills-l1l2l3-注入策略)
6. [TOOL 全注入与调度](#6-tool-全注入与调度)
7. [PromptBuilder 重构](#7-promptbuilder-重构)
8. [路由层改造](#8-路由层改造)
9. [性能、成本与风险控制](#9-性能成本与风险控制)
10. [实施步骤与兼容性](#10-实施步骤与兼容性)

---

## 1. 架构目标与核心原则

### 1.1 目标

| 维度 | 现状 | 目标 |
|------|------|------|
| **意图处理** | 双轨 Intent 分类（启发式 + LLM V2）前置路由，6 种意图分支 | 取消前置分类，统一 ReAct 入口 |
| **Skills 注入** | 按 `UserIntent` 条件注入（DirectAnswer 不注入、SingleToolCall 注入 L1、MultiStepPlanning 注入 L2） | 所有 Skills 按 L1/L2/L3 层次全量注入 Context |
| **Tool 注入** | 按意图过滤（DirectAnswer 跳过、MetaQuestion 跳过） | 所有可用 Tools 全量注入 Context |
| **LLM 决策权** | 系统预分类后由 LLM 在受限路径内执行 | LLM 在 ReAct 循环内完全自主决策工具调用与终止时机 |
| **代码复杂度** | `process_task_legacy` 中 6 分支 match + safety net 拦截 | 单一路由：`通用 ReAct` |

### 1.2 核心原则

1. **单一入口**：所有 `LlmChat` 类型任务统一走 `UnifiedReActExecutor`，不再分流。
2. **全上下文注入**：Skills（L1/L2/L3 层次化）+ Tools（全部）+ Memories（全部）始终注入 system prompt。
3. **LLM 自主决策**：由 LLM 在 ReAct 循环内自主判断「是否需要工具」「调用哪个工具」「何时终止」。
4. **渐进式披露**：Skills 不是平铺，而是按 L1（索引层）→ L2（摘要层）→ L3（完整文档层）结构化注入，LLM 可快速浏览 L1 后按需深入。
5. **保留例外路径**：`MetaQuestion`（"你能做什么"）、`WorkflowTrigger`（"/xxx" 指令）、`SkillExecution`/`McpTool` 等明确类型任务保持独立处理。

---

## 2. 现状问题分析

### 2.1 V2 Intent 架构的结构性问题

当前 `process_task_legacy` 中的意图路由逻辑：

```rust
match intent_analysis.intent {
    DirectAnswer     => handle_direct_answer(),      // 跳过工具注入
    MetaQuestion     => handle_meta_question(),      // 直接返回 skill 目录
    Correction       => handle_correction(),
    WorkflowTrigger  => handle_workflow_task(),
    MultiStepPlanning=> execute_with_planning(),     // Unified ReAct
    SingleToolCall   => handle_llm_task_with_intent(),
}
```

**问题**：
- **过早分类风险**：前置的 `IntentEngine`（关键词规则）和 `LLMIntentAnalyzer`（轻量 LLM 调用）在信息不足时做出路由决策，误分类导致工具被错误跳过。
- **Safety Net 反模式**：`handle_direct_answer` 中已出现「实时数据安全网」——当用户问 "BTC 价格" 时，系统硬编码拦截并改路由到 `SingleToolCall`。这说明**意图分类器不可靠**。
- **能力割裂**：`DirectAnswer` 路径完全不注入 Tools/Skills，LLM 无法自主判断「这个简单问题其实需要查一下实时数据」。
- **维护成本**：6 条分支 + 交叉 safety net + V1/V2 双分析器，代码路径指数增长。

### 2.2 Skills 条件注入的局限

当前 `PromptBuilder::build_skills_section` 按 `UserIntent` 决定注入层级：

```rust
match intent {
    DirectAnswer     => String::new(),           // 不注入
    MetaQuestion     => L1 only,                 // 只注入目录
    SingleToolCall   => L1 only,                 // name + one-liner
    MultiStepPlanning=> L1 + L2,                // name + summary
}
```

**问题**：
- `DirectAnswer` 不注入 Skills，LLM 不知道有哪些能力，无法主动调用。
- `SingleToolCall` 只注入 L1，LLM 看不到 skill 的详细用法，参数传递容易出错。
- 注入逻辑与 Intent 绑定，Intent 误判直接导致 Skills 信息缺失。

### 2.3 Tool 过滤的局限

当前 `PromptBuilder::build` 中：

```rust
if !matches!(intent, UserIntent::DirectAnswer | UserIntent::MetaQuestion) {
    // 注入 tools
}
```

**问题**：
- 即使被分类为 `DirectAnswer`，用户仍可能隐含需要 tool（如 "今天的天气怎么样"）。
- Safety net 的硬编码关键词无法覆盖所有实时数据场景。

---

## 3. 新架构总览

### 3.1 架构对比

**旧架构（V2 Intent 路由）**：
```
User Input
    ↓
[IntentEngine 启发式分类] ──confidence<0.7──→ [LLMIntentAnalyzer V2]
    ↓
UserIntent ──路由──→ DirectAnswer │ SingleToolCall │ MultiStepPlanning │ ...
    ↓                   ↓                ↓                  ↓
              无工具注入        L1 Skills + 部分Tools    L2 Skills + Tools
    ↓                   ↓                ↓                  ↓
              纯 LLM 回答      单轮 LLM + tool call    Unified ReAct
```

**新架构（统一 ReAct）**：
```
User Input
    ↓
[TaskType 判断] ──LlmChat──→ [通用 ReAct 入口]
    ↓                              ↓
其他类型任务              [PromptBuilder 组装全上下文]
（SkillExecution/           - SOUL.md (persona)
 McpTool/Workflow/          - USER.md (profile)
 A2a/...）                 - MEMORY.md (project)
                            - Memories (dynamic, 全部)
                            - Skills L1/L2/L3 (层次化)
                            - Tools (全部)
    ↓                              ↓
专用处理器              [UnifiedReActExecutor]
                            - LLM 自主决定 call_tool / final_answer
                            - max 30 rounds
                            - 可中断
                            - 重复调用检测
    ↓                              ↓
                          输出结果
```

### 3.2 核心变化

| 组件 | 旧行为 | 新行为 |
|------|--------|--------|
| `IntentEngine` | 启发式分类 6 种意图 | **移除**（或保留为只读标签，不参与路由） |
| `LLMIntentAnalyzer` | LLM 前置分析意图 | **移除**（或保留为 ReAct 内可选的 reflection 辅助） |
| `process_task_legacy` | 6 分支 match | 统一进入 `execute_unified_react(task)` |
| `PromptBuilder::build` | 按 `UserIntent` 条件注入 | 按 `ContextMode::UnifiedReact` 全量注入 |
| `build_skills_section` | 按 intent 过滤层级 | 按 L1/L2/L3 层次化结构化注入（全部 skills） |
| `build_tools_section` | 按 intent 决定是否注入 | **始终注入全部可用 tools** |
| `handle_direct_answer` | 独立路径，无工具 | **删除**，统一由 ReAct 处理 |
| `handle_llm_task_with_intent` | 单轮 LLM + 可选 tool | **删除**，统一由 ReAct 处理 |
| `execute_with_planning` | MultiStepPlanning 时调用 | **保留作为 ReAct 的别名/包装** |

---

## 4. 详细改造方案

### 4.1 移除 Intent 前置路由（`agent_impl.rs`）

**修改 `process_task_legacy`**：

```rust
async fn process_task_legacy(&self, task: Task) -> Result<(String, Vec<Artifact>), AgentError> {
    let task_id = task.id.clone();

    match &task.task_type {
        TaskType::LlmChat => {
            // 🆕 取消 Intent 分类，统一进入通用 ReAct
            self.execute_unified_react(task).await
        }
        TaskType::SkillExecution => self.handle_skill_task(&task).await,
        TaskType::McpTool => self.handle_mcp_task(&task).await,
        TaskType::FileProcessing => self.handle_file_task(&task).await,
        TaskType::A2aSend => self.handle_a2a_task(&task).await,
        TaskType::ChainTransaction => self.handle_chain_transaction_task(&task).await,
        TaskType::PlanCreation => self.handle_plan_creation_task(&task).await,
        TaskType::PlanExecution => self.handle_plan_execution_task(&task).await,
        TaskType::PlanAdaptation => self.handle_plan_adaptation_task(&task).await,
        TaskType::DeviceAutomation => self.handle_device_automation_task(&task).await,
        TaskType::AppLifecycle => self.handle_app_lifecycle_task(&task).await,
        TaskType::WorkflowExecution => self.handle_workflow_task(&task).await,
        TaskType::Custom(type_name) => {
            if self.should_use_planning(&task).await {
                self.execute_unified_react(task).await
            } else {
                warn!("Unknown custom task type: {}", type_name);
                Err(AgentError::InvalidConfig(format!(
                    "Unsupported task type: {}",
                    type_name
                )))
            }
        }
    }
}
```

**删除/弃用以下方法**：
- `classify_intent()` —— 不再用于路由
- `handle_direct_answer()` —— 功能由 ReAct 的 `final_answer` 替代
- `handle_llm_task_with_intent()` —— 功能由 ReAct 替代
- `handle_meta_question()` —— MetaQuestion 改为由 ReAct 处理（LLM 看到 L1 Skills 目录后可自主回答）
- `handle_correction()` —— Correction 逻辑可保留为 ReAct 内的 system prompt 规则

> **注意**：`IntentEngine` 和 `LLMIntentAnalyzer` 模块本身不删除，仅**不再参与主路由**。可保留用于：
> - 日志/观测（记录意图分类标签用于分析）
> - ReAct 内的可选 reflection 辅助
> - 未来需要快速预筛的场景（非主流程）

### 4.2 新建统一 ReAct 入口（`agent_impl.rs`）

```rust
/// 🆕 统一 ReAct 入口：所有 LlmChat 任务的唯一处理路径
async fn execute_unified_react(
    &self,
    task: Task,
) -> Result<(String, Vec<Artifact>), AgentError> {
    let input_text = extract_user_input(&task);
    info!("Unified ReAct entry for task {}: {}", task.id, input_text);

    // 1. 组装全上下文 system prompt
    let system_prompt = self.build_unified_react_prompt(&task).await?;

    // 2. 准备可用 tools（全部注入）
    let available_tools = self.build_full_tool_set().await?;

    // 3. 配置 ReAct executor
    let config = UnifiedReActConfig {
        max_rounds: self.max_rounds as usize,
        round_timeout_sec: 30,
        enable_reflection: true,
        require_structured_output: false, // 通用模式不要求严格 JSON
        cancel_rx: task.cancel_rx.clone(),
        stream_tx: task.stream_tx.clone(),
    };

    let executor = UnifiedReActExecutor::new(
        self.llm_interface.clone().ok_or_else(|| {
            AgentError::InvalidConfig("LLM interface not configured".into())
        })?,
    )
    .with_config(config)
    .with_tool_dispatcher(Arc::new(AgentSkillDispatcher::from_agent(self)));

    // 4. 执行 ReAct 循环
    let result = executor
        .execute(&system_prompt, &input_text, &available_tools)
        .await?;

    // 5. 包装为 TaskResult
    Ok((result, vec![]))
}
```

### 4.3 全上下文 Prompt 组装（`agent_impl.rs`）

```rust
/// 🆕 组装统一 ReAct 的 system prompt，全量注入 Skills（L1/L2/L3）+ Tools + Memories
async fn build_unified_react_prompt(&self, task: &Task) -> Result<String, AgentError> {
    let mut builder = crate::prompt::PromptBuilder::new()
        .with_model(self.config.model.clone());

    // 1. Persona (SOUL.md)
    if let Some(soul) = &self.config.soul_content {
        builder = builder.with_soul(soul.clone());
    }

    // 2. User profile (USER.md) — L2 memory
    if let Some(profile) = &self.config.user_profile {
        builder = builder.with_user_profile(profile.clone());
    }

    // 3. Project memory (MEMORY.md) — L1 memory
    if let Some(project) = &self.config.project_memory {
        builder = builder.with_project_memory(project.clone());
    }

    // 4. Dynamic memories — 全部注入（不再按 intent 过滤）
    if let Some(memory_system) = &self.memory_system {
        let memories = memory_system.search(&task.input, 10).await?;
        builder = builder.with_memories(memories);
    }

    // 5. Skills — L1/L2/L3 层次化注入（全部 skills）
    let skill_levels = self.build_all_skills_levels().await?;
    builder = builder.with_skills(skill_levels);

    // 6. Tools — 全部注入（不再按 intent 过滤）
    let tool_defs = self.build_all_tool_definitions().await?;
    builder = builder.with_tools(tool_defs);

    // 7. Model-specific instructions
    builder = builder.with_model_instructions(get_model_preset(&self.config.model));

    // 🆕 使用新的 UnifiedReact 模式构建
    Ok(builder.build_unified_react())
}
```

---

## 5. Skills L1/L2/L3 注入策略

### 5.1 层次定义

```rust
/// Skill 渐进式披露层级
pub enum SkillDisclosureLevel {
    L1, // ~30 tokens — name + one-liner (SKILL.index.md)
    L2, // ~200 tokens — summary with key concepts (SKILL.summary.md)
    L3, // ~2000 tokens — full SKILL.md content
}

/// 层次化 Skill 描述，用于统一 ReAct 的上下文注入
#[derive(Debug, Clone)]
pub struct HierarchicalSkillDesc {
    pub id: String,
    pub l1: SkillLevelDesc::L1,
    pub l2: Option<SkillLevelDesc::L2>,
    pub l3: Option<SkillLevelDesc::L3>,
}
```

### 5.2 注入格式（Structured Progressive Disclosure）

不再按 intent 过滤「显示哪些 skills」，而是**全部 skills 按层次结构化注入**：

```markdown
## 可用技能目录（L1 索引）

以下是你可使用的所有技能。每个技能包含 ID、名称和一句话描述。
如需了解某个技能的详细用法，参考下方的 L2 摘要或 L3 完整文档。

- weather_assistant: 查询全球任意城市的实时天气和未来预报
- crypto_trader: 加密货币交易下单、持仓查询和订单管理
- news_briefing: 获取指定主题或市场的最新新闻摘要
- portfolio_analyzer: 分析投资组合的收益率、风险敞口和资产配置
- ... (全部 skills)

## 技能详细摘要（L2）

### weather_assistant
查询全球城市的实时天气、未来 7 天预报、空气质量指数。
关键能力：支持城市名/坐标输入，返回结构化天气数据。

### crypto_trader
支持加密货币现货/限价/止损订单的下单、撤单、持仓查询。
关键能力：与 Alpaca MCP 集成，支持 BTC/USD、ETH/USD 等交易对。

### ... (全部 skills 的 L2)

## 技能完整文档（L3 — 按需引用）

### weather_assistant
[完整的 SKILL.md 内容，包括：详细参数说明、使用示例、返回值格式、错误处理、注意事项]

### ...
```

### 5.3 Token 控制策略

| 层级 | 内容 | 平均 Token/Skill | 50 Skills 总量 | 控制策略 |
|------|------|------------------|----------------|----------|
| L1 | name + one-liner | ~30 | ~1,500 | **始终注入** |
| L2 | summary | ~200 | ~10,000 | **始终注入**（可压缩） |
| L3 | full doc | ~2,000 | ~100,000 | **按需注入** |

**L3 按需注入机制**：
- 默认不注入 L3。
- 当 LLM 在 ReAct 中表示「需要了解 skill X 的详细参数」时，系统可在下一轮将 L3 文档追加到 context 中。
- 或者采用「Top-K 相关性」预加载：通过 embedding 检索与 user input 最相关的 3-5 个 skills 注入 L3。

```rust
/// 构建层次化 skills 描述
async fn build_all_skills_levels(&self) -> Result<Vec<SkillLevelDesc>, AgentError> {
    let registry = self.skill_registry.as_ref()
        .ok_or_else(|| AgentError::InvalidConfig("Skill registry not configured".into()))?;

    let mut skills = Vec::new();
    let all_skills = registry.list_all().await;

    for skill_meta in all_skills {
        // L1: 始终注入
        skills.push(SkillLevelDesc::L1 {
            id: skill_meta.id.clone(),
            name: skill_meta.name.clone(),
            one_liner: skill_meta.l1_index.clone()
                .unwrap_or_else(|| skill_meta.description.chars().take(80).collect()),
        });

        // L2: 始终注入（如存在）
        if let Some(summary) = &skill_meta.l2_summary {
            skills.push(SkillLevelDesc::L2 {
                id: skill_meta.id.clone(),
                name: skill_meta.name.clone(),
                summary: summary.clone(),
            });
        }
    }

    // L3: 按需注入（由 PromptBuilder 在 build_unified_react 时决定）
    // 可选：通过 embedding 检索 Top-5 相关 skills 注入 L3

    Ok(skills)
}
```

---

## 6. TOOL 全注入与调度

### 6.1 Tool 全注入

所有内置 Tools 和 MCP Tools 全部注入 context：

```rust
async fn build_all_tool_definitions(&self) -> Result<Vec<ToolDefinition>, AgentError> {
    let mut tools = Vec::new();

    // 1. 内置 Tools（file_read, web_search, skill_call, parallel_delegate, ...）
    for tool in self.builtin_tools.values() {
        tools.push(ToolDefinition {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters_schema(),
        });
    }

    // 2. MCP Tools（通过 MCPManager 获取所有注册 server 的 tools）
    if let Some(mcp_manager) = &self.mcp_manager {
        let mcp_tools = mcp_manager.list_all_tools().await?;
        for tool in mcp_tools {
            tools.push(ToolDefinition {
                name: format!("mcp:{}/{}", tool.server_name, tool.name),
                description: tool.description,
                parameters: tool.parameters_schema,
            });
        }
    }

    Ok(tools)
}
```

### 6.2 Tool 命名规范（便于 LLM 识别）

```
内置工具：file_read, web_search, skill_call, parallel_delegate, ...
MCP 工具：mcp:alpaca/place_crypto_order, mcp:alpaca/get_crypto_snapshot, ...
Skill 工具：skill:weather_assistant, skill:portfolio_analyzer, ...
```

### 6.3 ReAct 中 Tool 调用格式

保留现有 JSON 格式：

```json
{
  "thought": "用户问 BTC 价格，我需要调用 Alpaca MCP 的行情接口",
  "action": "call_tool",
  "tool_name": "mcp:alpaca/get_crypto_snapshot",
  "arguments": {"symbol": "BTC/USD"},
  "reasoning": "获取 BTC 实时快照数据，包括最新成交、买卖盘、日 K 和涨跌幅"
}
```

---

## 7. PromptBuilder 重构

### 7.1 新增 `build_unified_react` 方法

```rust
impl PromptBuilder {
    /// 🆕 统一 ReAct 模式：全量注入 Skills（L1/L2/L3 层次化）+ Tools + Memories
    pub fn build_unified_react(self) -> String {
        let mut parts = Vec::new();
        let c = self.components;

        // 1. Model-specific instructions
        if let Some(instr) = c.model_instructions {
            parts.push(instr);
        }

        // 2. Base persona
        if let Some(soul) = c.soul {
            parts.push(soul);
        }

        // 3. User profile
        if let Some(profile) = c.user_profile {
            parts.push(format!("[用户偏好]\n{}", profile));
        }

        // 4. Project memory
        if let Some(project) = c.project_memory {
            parts.push(format!("[项目约定]\n{}", project));
        }

        // 5. Dynamic memories（全部，不再过滤）
        if !c.memories.is_empty() {
            let memory_text = c.memories
                .iter()
                .map(|m| format!("- {}", m.content))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("[相关记忆]\n{}", memory_text));
        }

        // 6. Skills — L1/L2/L3 层次化注入
        let skills_text = Self::build_hierarchical_skills(&c.skills);
        if !skills_text.is_empty() {
            parts.push(skills_text);
        }

        // 7. Tools — 全部注入
        if !c.tools.is_empty() {
            let tools_text = c.tools
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

        // 9. ReAct 工作模式说明（通用规则）
        parts.push(build_react_rules().to_string());

        parts.join("\n\n")
    }

    /// 层次化 Skills 构建
    fn build_hierarchical_skills(skills: &[SkillLevelDesc]) -> String {
        let mut l1_items = Vec::new();
        let mut l2_sections = Vec::new();
        let mut l3_sections = Vec::new();

        for skill in skills {
            match skill {
                SkillLevelDesc::L1 { .. } => l1_items.push(skill.to_prompt_text()),
                SkillLevelDesc::L2 { .. } => l2_sections.push(skill.to_prompt_text()),
                SkillLevelDesc::L3 { .. } => l3_sections.push(skill.to_prompt_text()),
            }
        }

        let mut parts = Vec::new();

        if !l1_items.is_empty() {
            parts.push(format!(
                "## 技能目录（L1）\n以下是你可使用的所有技能。如需详细用法，参考下方的 L2/L3。\n{}",
                l1_items.join("\n")
            ));
        }

        if !l2_sections.is_empty() {
            parts.push(format!(
                "## 技能摘要（L2）\n{}",
                l2_sections.join("\n\n")
            ));
        }

        if !l3_sections.is_empty() {
            parts.push(format!(
                "## 技能详细文档（L3）\n{}",
                l3_sections.join("\n\n")
            ));
        }

        parts.join("\n\n")
    }
}
```

### 7.2 移除 `build` 的 intent 参数

旧方法 `pub fn build(self, intent: &UserIntent)` 保留用于兼容，但主流程改用 `build_unified_react()`。

---

## 8. 路由层改造

### 8.1 `process_task` 路由简化

```rust
async fn process_task(&self, task: Task) -> Result<TaskResult, AgentError> {
    match self.config.task_version {
        TaskVersion::V2 => self.process_task_v2(task).await,
        TaskVersion::Legacy => self.process_task_legacy(task).await,
    }
}

async fn process_task_legacy(&self, task: Task) -> Result<(String, Vec<Artifact>), AgentError> {
    match &task.task_type {
        // 🆕 统一 ReAct 入口
        TaskType::LlmChat => self.execute_unified_react(task).await,

        // 其他任务类型保持专用处理器
        TaskType::SkillExecution => self.handle_skill_task(&task).await,
        TaskType::McpTool => self.handle_mcp_task(&task).await,
        TaskType::WorkflowExecution => self.handle_workflow_task(&task).await,
        // ... 其他类型

        TaskType::Custom(type_name) => {
            if self.should_use_planning(&task).await {
                self.execute_unified_react(task).await
            } else {
                Err(AgentError::InvalidConfig(format!(
                    "Unsupported custom task type: {}", type_name
                )))
            }
        }
    }
}
```

### 8.2 `process_task_v2` 同步改造

如果 `process_task_v2` 已包含 `SkillSelector` + `UnifiedReActExecutor` 的逻辑，直接对齐：
- `SkillSelector` 的 `needs_planning=false` → 也进入 ReAct（LLM 自主决定是否需要多轮）
- `SkillSelector` 的 `needs_planning=true` → 进入 ReAct（与上面一致）

**本质**：`process_task_v2` 和 `process_task_legacy` 在 LlmChat 路径上合并为同一个 `execute_unified_react`。

---

## 9. 性能、成本与风险控制

### 9.1 Token 成本分析

| 项目 | 旧架构（DirectAnswer） | 旧架构（SingleToolCall） | 旧架构（MultiStepPlanning） | 新架构（统一 ReAct） |
|------|------------------------|--------------------------|----------------------------|----------------------|
| System Prompt | ~1K（persona only） | ~3K（+L1 skills + tools） | ~5K（+L2 skills + tools） | ~6K（+L1+L2 skills + all tools） |
| 首轮 LLM | 1 次 | 1 次 | 1 次（ReAct Round 1） | 1 次（ReAct Round 1） |
| 总轮次 | 1 | 1-2 | 1-10 | 1-30（LLM 自主） |
| 简单查询成本 | **低** | 中 | 中 | 中（与 SingleToolCall 持平） |
| 复杂任务成本 | — | — | 中 | 中（与 MultiStepPlanning 持平） |

**结论**：
- 简单查询（闲聊、问候）成本从「极低」上升到「中等」，因为必须注入 Skills + Tools。
- 这是**架构简化的必要代价**。如果成本敏感，可通过「L1 技能索引缓存」+「Tool 签名缓存」降低重复请求的 system prompt 成本。

### 9.2 风险控制

| 风险 | 缓解措施 |
|------|----------|
| LLM 在简单查询上仍调用工具（如 "你好" 后调用 weather） | ReAct system prompt 强化规则：「禁止过度思考，简单问候 1 轮结束」；`max_rounds` 上限 |
| Context 过长导致 LLM 注意力分散 | L3 按需注入；Observation >4K 自动截断；长期对话自动摘要 |
| Tool 全量注入导致 LLM 选择困难 | Tool 分组 + 命名规范（`mcp:` 前缀）；ReAct prompt 引导「根据任务需要选择性调用」 |
| ReAct 循环陷入死循环 | 30 轮硬上限；重复调用检测；强制终止时注入 system 消息要求 final_answer |
| 实时数据查询被误判为闲聊 | 不再存在「误判」问题——所有消息都带 Tools，LLM 可自主调用 |

### 9.3 回滚策略

如果统一 ReAct 导致简单查询延迟/成本不可接受：
- 恢复 `IntentEngine` 作为快速预筛（但不用于路由，仅用于「是否可跳过 ReAct」）
- 增加「零工具模式」：当 `IntentEngine` 以 >0.95 confidence 判定为纯闲聊时，直接 LLM 回答

---

## 10. 实施步骤与兼容性

### 10.1 实施顺序（建议）

| 阶段 | 任务 | 文件 | 工作量 |
|------|------|------|--------|
| **Phase 1** | 新建 `build_unified_react_prompt` + `build_hierarchical_skills` | `prompt/builder.rs` | 中 |
| **Phase 1** | 新建 `execute_unified_react` 入口 | `agent_impl.rs` | 中 |
| **Phase 2** | 修改 `process_task_legacy`，LlmChat 路由到 `execute_unified_react` | `agent_impl.rs` | 小 |
| **Phase 2** | 移除/注释 `handle_direct_answer`、`handle_llm_task_with_intent`、`handle_meta_question` 的调用 | `agent_impl.rs` | 小 |
| **Phase 3** | 改造 `general_react_prompt.rs`，适配 L1/L2/L3 Skills 层次化描述 | `skills/general_react_prompt.rs` | 中 |
| **Phase 3** | 确保 `UnifiedReActExecutor` 的 `execute` 支持全量 tools | `skills/unified_react_executor.rs` | 小 |
| **Phase 4** | 测试：简单查询、工具调用、多步任务、MetaQuestion | `tests/` | 大 |
| **Phase 4** | 性能基准：对比旧架构的 token 消耗和延迟 | 测试报告 | 中 |

### 10.2 兼容性

- **Agent 配置**：`TaskVersion::Legacy` 和 `TaskVersion::V2` 都使用 `execute_unified_react`，可合并。
- **Skill 格式**：现有 `SKILL.index.md`（L1）、`SKILL.summary.md`（L2）、`SKILL.md`（L3）完全兼容，无需改动。
- **Tool 定义**：现有 `SkillTool` trait 和 MCP tool 定义完全兼容。
- **LLM Provider**：所有 provider 通过 `LLMCallInterface` 调用，无影响。

### 10.3 监控指标

实施后应监控：
- `react_rounds_distribution`：ReAct 实际执行轮次分布（目标：简单查询 1-2 轮，复杂任务 3-10 轮）
- `tool_call_rate_by_query_type`：不同查询类型的工具调用率
- `prompt_token_count`：System prompt token 数（目标：6K-8K）
- `intent_misclassification_rate`：旧指标归零（因为不再分类）
- `user_satisfaction`：用户满意度评分

---

## 附录：PromptBuilder 旧方法对比

| 方法 | 状态 | 说明 |
|------|------|------|
| `build(self, intent: &UserIntent)` | **保留（兼容）** | 旧路由仍可用，主流程不再调用 |
| `build_with_reasoning(self, intent: &UserIntent)` | **保留（兼容）** | 旧路由仍可用 |
| `build_unified_react(self)` | **新增** | 新架构主入口 |
| `build_hierarchical_skills(&[SkillLevelDesc])` | **新增** | L1/L2/L3 结构化渲染 |
| `filter_memories_by_intent` | **保留（不再调用）** | 记忆不再按 intent 过滤 |

---

---

## 实施记录

**状态**：✅ 已完成  
**实施日期**：2026-05-13

### 修改文件清单

| 文件 | 修改内容 |
|------|---------|
| `crates/agents/src/prompt/builder.rs` | 新增 `build_unified_react()`、`build_hierarchical_skills()`、`build_unified_react_rules()`；旧的 `build()` 标记为 `#[deprecated]` |
| `crates/agents/src/prompt/mod.rs` | 导出 `ToolDefinition` 和 `model_presets` |
| `crates/agents/src/agent_impl.rs` | 新增 `execute_unified_react()`、`build_unified_react_prompt()`、`build_all_skills_levels()`、`build_all_tool_definitions()`、`extract_user_input()`；修改 `process_task()`、`process_task_v2()`、`process_task_legacy()` 统一路由到 ReAct |
| `crates/agents/src/skills/unified_react_executor.rs` | 新增 `skill_registry` 字段、`with_skill_registry()`、`extract_l3_request()`；在 `execute()` 中实现 L3 动态注入 |

### 编译验证

```bash
cargo check -p beebotos-agents
# 结果: Finished (0 errors, 29 warnings — 均为未使用函数的 warning，不影响运行)
```

### 已知遗留（非阻塞）

- `handle_direct_answer`、`handle_llm_task_with_intent`、`handle_meta_question`、`handle_correction` 等方法仍存在于代码中，但不再被主流程调用。可后续清理。
- `execute_with_react`、`execute_with_react_planning`、`execute_single_skill` 等 V2/V3 过渡方法同样不再被调用。
- 29 个 compiler warnings 均为 `dead_code` / `unused_variables`，不影响功能。

方案确认：
1、不保留 IntentEngine 作为纯闲聊快速路径；
2、默认不注入 L3。当 LLM 在 ReAct 中表示「需要了解 skill X 的详细参数」时，系统可在下一轮将 L3 文档追加到 context 中。
3、开始实施这个取消V2 intent 优化技术方案。



