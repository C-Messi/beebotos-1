# BeeBotOS Agent 系统优化改进方案

> 基于 Hermes Agent 系统设计理念（`docs/hermes/intent-prompt-rule1.md`）对 BeeBotOS 现有架构的深度优化方案。
>
> 版本：v1.0  
> 日期：2026-05-07

---

## 一、执行摘要

本方案基于对 BeeBotOS 实际代码的深度分析（`crates/agents/src/`、`apps/gateway/src/services/` 等核心模块），对照 Hermes Agent 系统的设计理念，识别出 **7 大优化方向**、**23 项具体改进措施**。核心目标是：提升用户意图识别精准度、降低 Planning 盲目性、控制 Prompt Token 消耗、强化工具调度安全性，最终实现从"单步反应式 Agent"到"显式规划型 Agent"的跃迁。

| 优化维度 | 现状问题 | 目标状态 |
|----------|----------|----------|
| **意图识别** | 硬编码关键词映射，误识别率高 | 轻量级 Intent Engine，分类精确到 5 大类 |
| **Prompt 组装** | 静态拼接，Token 浪费严重 | 动态模块化 + 渐进式披露 + 模型适配 |
| **工具调度** | 简单关键词过滤 Top-20 | Toolsets 分层 + 交易意图增强 + 审批链 |
| **Planning** | 阈值触发，缺少意图解析前置 | 显式规划循环：意图→分解→分配→监控→固化 |
| **Skills** | 全量加载，无自改进 | L1/L2/L3 渐进加载 + Feedback 循环 |
| **记忆系统** | 检索后简单拼接 | 记忆增强 Planning + 历史方案复用 |
| **安全成本** | 缺少输出截断、轮次限制 | 分层防御 + 成本天花板 |

---

## 二、现状与差距深度分析

### 2.1 意图识别：从"关键词猜测"到"语义理解"

**现状代码位置**：`crates/agents/src/agent_impl.rs` (line ~2548)

```rust
// 当前实现：硬编码 domain_keywords 映射
let domain_keywords: [(&[&str], &str); 12] = [
    (&["天气", "weather", "temperature"], "weather_assistant"),
    (&["新闻", "news", "头条"], "news_briefing"),
    // ... 共 12 组
];
```

**问题**：
1. **覆盖率低**：只有 12 组硬编码映射，无法覆盖 100+ 工具。
2. **语义丢失**：用户说"根据当前的市场形势，下单购买 btc/usd"，关键词同时命中 `crypto` 和 `trade`，但系统无法区分"查询意图"和"交易意图"。
3. **无负向识别**：无法识别"不要查询，直接下单"这类否定指令。
4. **多意图冲突**：一句话包含多个动作时（"查价格然后下单"），系统只能匹配一个。

**Hermes 差距**：Hermes 在第 5 节明确设计了"意图解析"作为 Planning 的第一步，通过 LLM 提取目标与约束，而非简单关键词匹配。

### 2.2 System Prompt：从"静态模板"到"动态组装"

**现状代码位置**：`apps/gateway/src/services/message_processor.rs` (line ~1444)、`crates/agents/src/agent_impl.rs` (line ~392)

**问题**：
1. **全量加载**：`USER.md` + `SOUL.md` + 动态记忆 + 技能提示，全部塞进 System Prompt，无差别对待简单查询和复杂任务。
2. **无模型适配**：Kimi k2.6、GPT-4、Claude 3 的 prompt 优化策略不同，但系统使用同一套模板。
3. **技能目录暴力注入**：`inject_skill_catalog` 注入完整的 100+ 技能描述（即使已做 Top-20 过滤），对于不需要工具调用的对话是严重 Token 浪费。
4. **无渐进式加载**：技能一旦匹配就全量加载 SKILL.md，没有 L1（索引）→ L2（摘要）→ L3（完整）的分级披露。

**Hermes 差距**：Hermes 第 2 节设计了 7 大组件的动态模块化组装；第 4 节设计了 Skills 的 3 级渐进披露，目标是将单次 Prompt Token 控制在合理范围。

### 2.3 Function Calling：从"Top-20 过滤"到"意图驱动调度"

**现状代码位置**：`crates/agents/src/agent_impl.rs` (line ~1116)

**问题**：
1. **预过滤逻辑脆弱**：基于关键词重叠的打分机制，同分工具排序不稳定，导致交易类工具被数据查询类工具挤出 Top-20。
2. **无 Toolsets 概念**：101 个工具平铺，LLM 面对长列表选择困难。没有按 `account`/`trading`/`stock-data` 等域分组。
3. **描述重复冗长**：`manifest.description` 和 `function.description` 拼接，导致 `place_crypto_order` 的描述超过 500 字，反而降低 LLM 对核心信息的捕捉。
4. **缺少审批链**：破坏性操作（如 `place_crypto_order` 的 `destructiveHint=true`）没有二次确认机制。

**Hermes 差距**：Hermes 第 3 节设计了 Toolsets 分层防御（注册/执行/安全/输入四层），以及 Agent 级工具的多步管道压缩。

### 2.4 Planning：从"阈值触发"到"显式规划循环"

**现状代码位置**：`crates/agents/src/planning/`、`crates/agents/src/agent_impl.rs` (line ~698)

**问题**：
1. **缺少意图解析前置**：Planning 由字数阈值触发（中文 > 50 字），但"字数多"不等于"需要规划"。真正的触发条件应该是"意图复杂度"。
2. **资源分配隐性**：`Decomposer` 分解任务后，`search_skills_for_step` 仍使用关键词匹配，没有显式的"为子任务匹配 Skill/Tool/MCP Server"步骤。
3. **无记忆增强**：Planning 时没有注入 MEMORY.md 中的历史解决方案，导致同类问题重复推理。
4. **缺少执行可视化**：没有 `ToolTrail` 或 `<REASONING_SCRATCHPAD>` 标签，开发者和用户都无法追踪 Planning 过程。
5. **经验固化缺失**：Plan 执行成功后，没有自动将成功路径写入 Skill 或 Memory。

**Hermes 差距**：Hermes 第 5 节设计了完整的 7 步规划循环，特别强调"意图解析"前置和"经验固化"闭环。

### 2.5 Skills 系统：从"全量注册"到"渐进式自改进"

**现状代码位置**：`crates/agents/src/skills/`、`crates/agents/src/mcp/skill_bridge.rs`

**问题**：
1. **无 SKILL.md 规范**：技能描述存储在 `SkillManifest.description` 中，是程序内嵌字符串，没有独立的 `SKILL.md` 程序式指南。
2. **无渐进式加载**：技能一旦注册，全量信息进入 Registry，没有 L1/L2/L3 分级。
3. **无自改进机制**：任务完成后没有 `skill_manage` 工具来固化经验。

**Hermes 差距**：Hermes 第 4 节将 Skills 定义为"杀手级特性"，强调程序式指南、渐进披露和自改进。

### 2.6 记忆系统：从"检索后拼接"到"上下文感知增强"

**现状代码位置**：`crates/agents/src/memory/`、`crates/agents/src/agent_impl.rs`

**问题**：
1. **Planning 无记忆注入**：`PlanningEngine::create_plan` 没有接收 Memory 上下文。
2. **无历史方案复用**：`HybridSearchSqlite` 检索到的是碎片化的对话记录，没有"历史同类问题的解决方案摘要"。
3. **SessionDB 未充分使用**：`memory_search.db` 有 FTS5 能力，但 `message_processor.rs` 的 `build_memory_context` 只做了简单的 hybrid search，没有利用 FTS5 做跨会话的全文检索。

**Hermes 差距**：Hermes 第 5.4 节强调 Planning 过程可利用 MEMORY.md、SessionDB (FTS5)、Honcho 用户建模三层记忆。

### 2.7 安全与成本控制：从"沙箱兜底"到"分层防御"

**现状代码位置**：`crates/agents/src/security/`、`crates/agents/src/kernel_integration.rs`

**问题**：
1. **无 Prompt 缓存**：每次请求都重新组装完整的 System Prompt，重复 Token 消耗大。
2. **无上下文压缩**：长对话没有自动摘要机制，超过窗口时直接截断。
3. **无工具输出截断**：MCP 工具返回大型 JSON 时直接注入上下文，可能撑爆窗口。
4. **无 max_rounds**：Agent 与 LLM 的交互轮次没有硬上限，存在死循环风险。
5. **沙箱集成 TODO**：`session_isolation.rs` 中 `IsolationLevel::Wasm` 标记为 TODO。

**Hermes 差距**：Hermes 第 7 节设计了完整的 Prompt 缓存、上下文压缩、工具输出截断、max_rounds 等机制。

---

## 三、详细优化方案

### 3.1 意图识别引擎（Intent Engine）

**目标**：在消息进入 LLM 主循环前，增加一个轻量级但精确的意图分类层，指导后续处理路径的选择。

#### 3.1.1 引入 Intent 分类器

**新增模块**：`crates/agents/src/intent/`（或集成到 `agent_impl.rs` 中）

```rust
/// 用户意图分类
pub enum UserIntent {
    /// 闲聊/问候/简单问答，无需工具
    DirectAnswer,
    /// 单步工具调用（查天气、查股价等）
    SingleToolCall,
    /// 多步复杂任务（需要 Planning）
    MultiStepPlanning,
    /// 触发预定义 Workflow
    WorkflowTrigger,
    /// 关于系统本身的元问题（"你会什么"）
    MetaQuestion,
    /// 否定/修正指令（"不要查询，直接下单"）
    Correction,
}

pub struct IntentAnalysis {
    pub intent: UserIntent,
    /// 提取的实体（如 symbol=BTC/USD, side=buy）
    pub entities: HashMap<String, String>,
    /// 用户明确指定的约束（如"不要先查询"）
    pub constraints: Vec<String>,
    /// 置信度 0.0-1.0
    pub confidence: f32,
}
```

**实现策略（双轨制）**：

| 策略 | 适用场景 | 实现方式 |
|------|----------|----------|
| **规则引擎（轻量）** | 高频、模式固定的意图 | 正则 + 关键词 + 否定词检测，无需 LLM |
| **LLM 分类器（精准）** | 复杂、模糊的意图 | 调用小型模型（或主模型的快速分类模式） |

**规则引擎示例**：
```rust
fn classify_intent_heuristic(query: &str) -> IntentAnalysis {
    let lower = query.to_lowercase();
    
    // 否定/修正检测
    if lower.contains("不要") || lower.contains("别") || lower.contains("直接") {
        return IntentAnalysis::new(UserIntent::Correction, ...);
    }
    
    // 元问题检测
    if lower.contains("你会什么") || lower.contains("有哪些技能") {
        return IntentAnalysis::new(UserIntent::MetaQuestion, ...);
    }
    
    // Workflow 触发检测（以 "/" 开头或匹配 workflow 名称）
    if query.starts_with('/') || matches_workflow_name(&lower) {
        return IntentAnalysis::new(UserIntent::WorkflowTrigger, ...);
    }
    
    // 多步规划检测（顺序词 + 多个动作）
    let step_keywords = ["先", "再", "然后", "接着", "最后", "第一步"];
    let action_count = count_distinct_actions(&lower);
    if step_keywords.iter().any(|k| lower.contains(k)) || action_count >= 2 {
        return IntentAnalysis::new(UserIntent::MultiStepPlanning, ...);
    }
    
    // 默认：单步工具调用或直连回答
    if has_tool_keyword(&lower) {
        IntentAnalysis::new(UserIntent::SingleToolCall, ...)
    } else {
        IntentAnalysis::new(UserIntent::DirectAnswer, ...)
    }
}
```

**LLM 分类器 Prompt 模板**：
```
分析以下用户输入，输出 JSON：
{
  "intent": "DirectAnswer|SingleToolCall|MultiStepPlanning|WorkflowTrigger|MetaQuestion|Correction",
  "entities": {"symbol": "...", "side": "..."},
  "constraints": ["不要先查询价格"],
  "confidence": 0.95
}

规则：
- 包含"下单/购买/buy/sell/place order" → SingleToolCall（除非有"先...再..."）
- 包含"先...再...然后..." → MultiStepPlanning
- 以"/"开头 → WorkflowTrigger
- "你会什么/有哪些技能" → MetaQuestion
- "不要/别/直接" → Correction

用户输入：{query}
```

#### 3.1.2 基于意图的路由分发

**修改位置**：`crates/agents/src/agent_impl.rs::process_task()`

```rust
async fn process_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
    // 🆕 FIX: Intent Engine 前置
    let intent_analysis = self.classify_intent(&task.input).await;
    
    match intent_analysis.intent {
        UserIntent::DirectAnswer => {
            // 跳过工具注入，直接 LLM 回答，节省 Token
            self.handle_direct_answer(task).await
        }
        UserIntent::SingleToolCall => {
            // 精准注入 Top-10 工具（而非 Top-20），减少干扰
            self.handle_single_tool_task(task, &intent_analysis).await
        }
        UserIntent::MultiStepPlanning => {
            // 触发完整 Planning 流程
            self.execute_with_planning(task, &intent_analysis).await
        }
        UserIntent::WorkflowTrigger => {
            // 直接路由到 Workflow Engine
            self.handle_workflow_task(task).await
        }
        UserIntent::MetaQuestion => {
            // 跳过 LLM，直接 Registry 查询返回
            self.handle_meta_question(task).await
        }
        UserIntent::Correction => {
            // 修正上一轮行为
            self.handle_correction(task, &intent_analysis).await
        }
    }
}
```

**收益**：
- **DirectAnswer** 场景下，不注入任何工具描述，单次请求节省 5k-10k Token。
- **SingleToolCall** 场景下，只注入最相关的 5-10 个工具，降低 LLM 选择困难。
- **Correction** 场景下，可以撤销上一轮错误操作（如"不要查询了，直接下单"）。

---

### 3.2 System Prompt 动态组装（PromptBuilder）

**目标**：实现 Hermes 的"动态模块化组装"和"渐进式披露"，将 Prompt Token 消耗降低 30-50%。

#### 3.2.1 引入 PromptBuilder 模块

**新增模块**：`crates/agents/src/prompt/builder.rs`

```rust
pub struct PromptBuilder {
    /// 基础人格（SOUL.md）
    soul: Option<String>,
    /// 用户画像（USER.md）
    user_profile: Option<String>,
    /// 动态记忆（按相关度排序）
    memories: Vec<MemorySnippet>,
    /// 技能目录（L1/L2/L3）
    skills: Vec<SkillLevel>,
    /// 可用工具（按意图过滤）
    tools: Vec<ToolDefinition>,
    /// 模型特定指令
    model_instructions: Option<String>,
    /// 上下文文件
    context_files: Vec<ContextFile>,
}

pub enum SkillLevel {
    L1 { id: String, name: String, one_liner: String },      // ~30 tokens
    L2 { id: String, name: String, summary: String },        // ~200 tokens
    L3 { id: String, name: String, full_doc: String },       // ~2000 tokens
}
```

**组装策略**：

```rust
impl PromptBuilder {
    pub fn build(self, intent: &UserIntent, model: &str) -> String {
        let mut parts = Vec::new();
        
        // 1. 模型特定指令（最前面，影响最大）
        if let Some(instr) = self.model_instructions {
            parts.push(instr);
        }
        
        // 2. 基础人格（始终加载，但可压缩）
        if let Some(soul) = self.soul {
            parts.push(soul);
        }
        
        // 3. 用户画像（始终加载）
        if let Some(profile) = self.user_profile {
            parts.push(profile);
        }
        
        // 4. 动态记忆（按意图筛选相关度）
        let relevant_memories = self.filter_memories_by_intent(intent);
        for m in relevant_memories {
            parts.push(m.content);
        }
        
        // 5. 技能（渐进式加载）
        match intent {
            UserIntent::DirectAnswer => {
                // 不加载任何技能描述
            }
            UserIntent::SingleToolCall => {
                // 只加载 L1（名称+一句话）
                for skill in self.skills {
                    if let SkillLevel::L1 { id, name, one_liner } = skill {
                        parts.push(format!("- {}: {}", name, one_liner));
                    }
                }
            }
            UserIntent::MultiStepPlanning => {
                // 加载 L2（关键概念+触发条件）
                for skill in self.filter_relevant_skills() {
                    parts.push(skill.l2_summary());
                }
            }
            _ => {}
        }
        
        // 6. 工具使用指南（按需）
        if !matches!(intent, UserIntent::DirectAnswer | UserIntent::MetaQuestion) {
            parts.push(self.build_tools_guide());
        }
        
        // 7. 上下文文件
        for file in self.context_files {
            parts.push(file.content);
        }
        
        parts.join("\n\n")
    }
}
```

#### 3.2.2 模型特定指令适配

**新增配置**：`config/beebotos.toml` 中添加 `[models.kimi]` 的子项

```toml
[models.kimi]
base_url = "https://api.moonshot.cn/v1"
model = "kimi-k2.6"
temperature = 1.0
# 🆕 FIX: 模型特定 prompt 优化
system_prompt_suffix = ""
prefers_react_format = false          # Kimi 原生支持 function calling
prefers_detailed_tools = true         # Kimi 对详细参数描述响应更好
max_tools_per_request = 20            # Kimi 建议的工具数量上限
reasoning_hint = "请直接输出工具调用，不要做过多分析。"

[models.gpt4]
model = "gpt-4o"
prefers_react_format = false
prefers_detailed_tools = false        # GPT-4 对简洁描述响应更好
max_tools_per_request = 128
reasoning_hint = ""
```

**实现**：在 `llm_service.rs` 或 `agent_impl.rs` 中，根据当前 LLM provider 加载对应的 `model_instructions`。

#### 3.2.3 引入 `<REASONING_SCRATCHPAD>` 标签

**目标**：让 LLM 在复杂任务中输出显式推理过程，便于调试和训练数据提取。

**修改位置**：`crates/agents/src/agent_impl.rs::handle_llm_task()`

```rust
// 当意图为 MultiStepPlanning 时，在 system prompt 中附加推理标签
if matches!(intent, UserIntent::MultiStepPlanning) {
    let reasoning_hint = format!(
        "这是一个复杂任务，请按以下步骤思考并在回复中包含 <REASONING_SCRATCHPAD> 标签：\n\
         1. 分析用户目标\n\
         2. 确定需要调用的工具及顺序\n\
         3. 验证每一步的依赖关系\n\
         输出格式：<REASONING_SCRATCHPAD>你的思考过程</REASONING_SCRATCHPAD>\n\
         然后输出实际回答或工具调用。"
    );
    extra_params.insert("system_prompt_suffix".to_string(), reasoning_hint);
}
```

**后续处理**：在 `cleanup_thinking_process()` 中，如果检测到 `<REASONING_SCRATCHPAD>`，提取并存储到 `ToolTrail`，不返回给用户。

---

### 3.3 Function Calling 与工具调度优化

**目标**：解决"查询工具替代交易工具"问题，实现 Hermes 的 Toolsets 分层防御。

#### 3.3.1 Toolsets 分组与分层过滤

**现状**：101 个工具平铺，关键词过滤后取 Top-20。

**改进**：引入 Toolsets 概念，按功能域分组，意图匹配时优先从相关 Toolset 中选择。

**修改位置**：`crates/agents/src/agent_impl.rs`

```rust
/// Toolset 定义
pub struct Toolset {
    pub name: String,           // e.g., "trading", "crypto-data"
    pub description: String,
    pub tool_ids: Vec<String>,
    /// 触发该 toolset 的关键词
    pub trigger_keywords: Vec<String>,
}

/// 预定义 Toolsets（与 Alpaca MCP manifest 对齐）
const DEFAULT_TOOLSETS: &[(&str, &[&str])] = &[
    ("account", &["账户", "account", "余额", "balance", "portfolio"]),
    ("trading", &["下单", "购买", "买入", "卖出", "order", "buy", "sell", "place", "交易"]),
    ("watchlists", &["自选", "watchlist", "关注"]),
    ("stock-data", &["股票", "股价", "stock", "AAPL", "TSLA"]),
    ("crypto-data", &["比特币", "BTC", "以太坊", "ETH", "crypto", "加密货币"]),
    ("options-data", &["期权", "option", "call", "put"]),
    ("news", &["新闻", "news", "头条"]),
];

/// 基于意图的 Toolset 预筛选
fn filter_tools_by_intent_and_toolsets(
    all_skills: &[RegisteredSkill],
    intent: &UserIntent,
    query: &str,
    top_n: usize,
) -> Vec<&RegisteredSkill> {
    let query_lower = query.to_lowercase();
    
    // 1. 确定激活的 Toolsets
    let mut active_toolsets: HashSet<&str> = HashSet::new();
    for (name, keywords) in DEFAULT_TOOLSETS {
        if keywords.iter().any(|k| query_lower.contains(k)) {
            active_toolsets.insert(name);
        }
    }
    
    // 2. 根据意图强制包含/排除 Toolsets
    match intent {
        UserIntent::SingleToolCall if active_toolsets.contains("trading") => {
            // 交易意图下，强制激活 trading + account（查余额可能需要）
            active_toolsets.insert("account");
        }
        UserIntent::DirectAnswer => {
            // 直接回答不需要任何工具
            return vec![];
        }
        _ => {}
    }
    
    // 3. 从激活的 Toolsets 中筛选工具
    let mut candidates: Vec<(usize, &RegisteredSkill)> = all_skills
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| {
            // 检查该技能是否属于激活的 toolset
            let belongs_to_active = active_toolsets.iter().any(|ts| {
                s.tags.contains(&ts.to_string()) || 
                s.skill.id.to_lowercase().contains(ts)
            });
            
            if !belongs_to_active && !active_toolsets.is_empty() {
                return None;
            }
            
            let score = compute_relevance_score(s, query);
            Some((score, s))
        })
        .collect();
    
    // 4. 排序并取 Top-N
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().take(top_n).map(|(_, s)| s).collect()
}
```

#### 3.3.2 破坏性操作审批链

**目标**：对于 `destructiveHint = true` 的工具（如下单、删除、发送），增加二次确认。

**修改位置**：`crates/agents/src/agent_impl.rs::execute_registered_skill()` 或新增 `security/approval.rs`

```rust
pub struct ApprovalGate {
    /// 需要审批的操作类型
    pub requires_approval: Vec<String>,
    /// 审批方式：同步阻塞 / 异步回调
    pub mode: ApprovalMode,
}

pub enum ApprovalMode {
    /// 阻塞等待用户确认（适合聊天场景）
    SyncConfirm { timeout_secs: u64 },
    /// 基于规则的自动审批（如 paper trading 免确认）
    RuleBased(Vec<ApprovalRule>),
    /// 需要管理员密钥签名
    AdminSignature,
}

pub struct ApprovalRule {
    pub condition: Box<dyn Fn(&SkillExecutionContext) -> bool + Send + Sync>,
    pub auto_approve: bool,
}

/// 在 execute_registered_skill 中集成
async fn execute_with_approval(
    &self,
    skill_id: &str,
    params: &serde_json::Value,
    context: &SkillExecutionContext,
) -> Result<String, AgentError> {
    // 1. 检查是否需要审批
    if self.approval_gate.requires_approval_for(skill_id, params) {
        // 2. 生成审批请求
        let request = ApprovalRequest {
            skill_id: skill_id.to_string(),
            params: params.clone(),
            risk_level: self.assess_risk(skill_id, params),
            description: self.describe_action(skill_id, params),
        };
        
        // 3. 发送审批请求（通过当前会话的 channel 回传）
        let approved = self.request_approval(request).await?;
        if !approved {
            return Ok("操作已取消（用户未批准）".to_string());
        }
    }
    
    // 4. 执行原操作
    self.execute_skill_inner(skill_id, params).await
}
```

**示例规则**：
```rust
// Paper trading 自动通过
ApprovalRule {
    condition: Box::new(|ctx| {
        ctx.skill_id.contains("alpaca") && 
        ctx.env.get("ALPACA_PAPER_TRADE") == Some("true")
    }),
    auto_approve: true,
}

// 真实交易需要确认
ApprovalRule {
    condition: Box::new(|ctx| {
        ctx.skill_id.contains("place_") && 
        ctx.skill_id.contains("_order") &&
        ctx.env.get("ALPACA_PAPER_TRADE") != Some("true")
    }),
    auto_approve: false,
}
```

#### 3.3.3 工具链压缩（Tool Chain Compression）

**目标**：将 LLM 多步推理压缩为单次调用，减少往返延迟。

**场景**：用户说"查一下 AAPL 的最新价格，如果高于 180 就买入 10 股"。

**现状**：LLM 需要 2-3 轮交互（查价格 → 判断 → 下单）。

**改进**：引入"条件工具"概念，允许 LLM 在一次 function calling 中表达条件逻辑。

```rust
/// 条件工具调用（扩展 ToolCall 结构）
pub struct ConditionalToolCall {
    pub condition: String,           // "latest_price > 180"
    pub if_true: Vec<ToolCall>,      // 条件满足时执行
    pub if_false: Vec<ToolCall>,     // 条件不满足时执行
    pub required_info: Vec<String>,  // 需要预先获取的信息
}
```

**实现方式（渐进式）**：
1. **短期**：在 System Prompt 中教 LLM 使用复合调用格式：
   ```
   STEP 1: get_stock_latest_quote|{"symbols":"AAPL"}
   IF result.price > 180 THEN
   STEP 2: place_stock_order|{"symbol":"AAPL","side":"buy","qty":"10"}
   ELSE
   STEP 2: notify|{"message":"价格未达预期"}
   ```

2. **中期**：在 `agent_impl.rs` 中增加复合调用解析器，自动执行多步链。

---

### 3.4 Planning 与推理机制升级

**目标**：从"字数阈值触发"升级为"显式规划循环"，提升复杂任务成功率。

#### 3.4.1 引入 Intent Analyzer 前置模块

**修改位置**：`crates/agents/src/planning/engine.rs`

在 `PlanningEngine::create_plan` 之前，增加 `IntentAnalyzer`：

```rust
pub struct IntentAnalyzer;

impl IntentAnalyzer {
    /// 解析用户意图，提取目标、约束、隐含需求
    pub async fn analyze(&self, query: &str, memory: &dyn MemorySearch) -> IntentResult {
        // 1. 实体提取（symbol, side, qty, price_threshold 等）
        let entities = self.extract_entities(query);
        
        // 2. 约束提取（"不要先查询"、"必须在今天完成"等）
        let constraints = self.extract_constraints(query);
        
        // 3. 历史方案检索（MEMORY.md + SessionDB）
        let historical_solutions = memory.search(
            &format!("如何解决: {}", query),
            3,
        ).await;
        
        // 4. 组装分析结果供 Decomposer 使用
        IntentResult {
            goal: self.summarize_goal(query),
            entities,
            constraints,
            historical_solutions,
            suggested_approach: self.infer_approach(&entities, &constraints),
        }
    }
}
```

#### 3.4.2 显式资源分配步骤

**修改位置**：`crates/agents/src/planning/decomposer.rs`

在分解任务后，增加"资源分配"阶段：

```rust
pub struct ResourceAllocation {
    pub step_index: usize,
    pub assigned_skills: Vec<String>,
    pub assigned_tools: Vec<String>,
    pub assigned_mcp_servers: Vec<String>,
    pub estimated_tokens: u32,
    pub estimated_time_secs: u32,
}

impl Decomposer {
    pub async fn decompose_with_allocation(
        &self,
        goal: &str,
        intent_result: &IntentResult,
        registry: &SkillRegistry,
    ) -> Result<(Plan, Vec<ResourceAllocation>), PlanningError> {
        // 1. 先分解为原子步骤
        let mut plan = self.decompose(goal).await?;
        
        // 2. 为每个步骤分配资源
        let mut allocations = Vec::new();
        for (i, step) in plan.steps.iter_mut().enumerate() {
            let allocation = self.allocate_resources(step, intent_result, registry).await?;
            
            // 将分配结果写入 step 的 metadata
            step.metadata.insert("skills".to_string(), json!(allocation.assigned_skills));
            step.metadata.insert("tools".to_string(), json!(allocation.assigned_tools));
            
            allocations.push(allocation);
        }
        
        Ok((plan, allocations))
    }
    
    async fn allocate_resources(
        &self,
        step: &PlanStep,
        intent: &IntentResult,
        registry: &SkillRegistry,
    ) -> Result<ResourceAllocation, PlanningError> {
        // 基于步骤描述，从 Registry 中搜索最匹配的技能
        let skills = registry.search(&step.description).await;
        
        // 基于意图中的实体，匹配 MCP 工具
        let mut tools = Vec::new();
        if step.description.contains("下单") || step.description.contains("order") {
            if intent.entities.get("symbol").map(|s| s.contains("BTC")).unwrap_or(false) {
                tools.push("mcp:alpaca/place_crypto_order".to_string());
            } else {
                tools.push("mcp:alpaca/place_stock_order".to_string());
            }
        }
        
        Ok(ResourceAllocation {
            step_index: 0,
            assigned_skills: skills.into_iter().map(|s| s.skill.id).collect(),
            assigned_tools: tools,
            assigned_mcp_servers: vec!["alpaca".to_string()],
            estimated_tokens: 1000,
            estimated_time_secs: 30,
        })
    }
}
```

#### 3.4.3 ToolTrail 执行可视化

**目标**：让 Planning 的执行过程可追踪、可调试。

**新增模块**：`crates/agents/src/planning/tool_trail.rs`

```rust
pub struct ToolTrail {
    pub plan_id: String,
    pub steps: Vec<TrailStep>,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub status: TrailStatus,
}

pub struct TrailStep {
    pub step_number: usize,
    pub description: String,
    pub tool_calls: Vec<ToolCallRecord>,
    pub reasoning: Option<String>,          // <REASONING_SCRATCHPAD> 内容
    pub status: StepStatus,
    pub duration_ms: u64,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

pub struct ToolCallRecord {
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub result_summary: String,             // 截断后的结果摘要
    pub success: bool,
}
```

**集成到 Executor**：
```rust
// 在 PlanExecutor::execute_step 中
async fn execute_step(
    &self,
    step: &PlanStep,
    trail: &mut ToolTrail,
) -> Result<StepResult, ExecutionError> {
    let step_start = Instant::now();
    
    // 1. 记录推理过程（如果有）
    if let Some(reasoning) = step.reasoning {
        trail.steps.last_mut().unwrap().reasoning = Some(reasoning.to_string());
    }
    
    // 2. 执行工具调用
    for tool_call in &step.tool_calls {
        let result = self.execute_tool(tool_call).await;
        
        trail.steps.last_mut().unwrap().tool_calls.push(ToolCallRecord {
            tool_name: tool_call.name.clone(),
            parameters: tool_call.params.clone(),
            result_summary: truncate(&result, 200),
            success: result.is_ok(),
        });
    }
    
    trail.steps.last_mut().unwrap().duration_ms = step_start.elapsed().as_millis() as u64;
    
    Ok(StepResult::Success)
}
```

**输出方式**：
- **日志**：JSON 格式输出到 `tracing`，便于 ELK 收集。
- **WebSocket**：通过 `AgentEventBus` 实时推送到前端，显示执行进度。
- **持久化**：Plan 完成后，ToolTrail 写入 SQLite，支持历史回放。

#### 3.4.4 经验固化（Skill/Memory 自改进）

**目标**：Planning 成功后，将路径固化为可复用资产。

**修改位置**：`crates/agents/src/planning/engine.rs`（Plan 完成后）

```rust
/// 在 PlanningEngine::execute_plan 成功后调用
async fn solidify_experience(
    &self,
    plan: &Plan,
    trail: &ToolTrail,
    original_query: &str,
) -> Result<(), PlanningError> {
    // 1. 生成经验摘要
    let experience = self.summarize_success_path(plan, trail).await?;
    
    // 2. 写入 MEMORY.md（高价值经验）
    if experience.importance_score >= 0.8 {
        self.memory.store(MemoryEntry {
            content: format!(
                "## 问题类型: {}\n\n### 用户查询\n{}\n\n### 解决路径\n{}\n\n### 关键工具\n{}",
                experience.category,
                original_query,
                experience.steps_markdown,
                experience.key_tools.join(", ")
            ),
            category: "solution".to_string(),
            importance: experience.importance_score,
        }).await?;
    }
    
    // 3. 更新或创建 Skill（如果该模式值得复用）
    if experience.reusability_score >= 0.7 {
        self.skill_manager.create_or_update_skill(
            &experience.skill_name,
            &experience.skill_markdown,
        ).await?;
    }
    
    Ok(())
}
```

---

### 3.5 Skills 系统升级

#### 3.5.1 引入 SKILL.md 规范与渐进式加载

**目标**：实现 Hermes 的 L1/L2/L3 渐进披露。

**文件结构规范**：
```
skills/
├── crypto-trading/
│   ├── SKILL.md          # L3: 完整程序式指南
│   ├── SKILL.summary.md  # L2: 200 字摘要
│   └── SKILL.index.md    # L1: 一句话描述
```

**修改位置**：`crates/agents/src/skills/discovery.rs`

```rust
pub struct DiscoveredSkill {
    pub loaded_skill: LoadedSkill,
    pub l1_index: String,      // 一句话
    pub l2_summary: String,    // 200 字摘要
    pub l3_full_doc: String,   // 完整 SKILL.md
}

impl SkillDiscovery {
    pub async fn discover_with_levels(&self, path: &Path) -> Result<Vec<DiscoveredSkill>, SkillError> {
        let mut skills = Vec::new();
        
        for entry in fs::read_dir(path)? {
            let dir = entry?.path();
            let skill_md = dir.join("SKILL.md");
            let summary_md = dir.join("SKILL.summary.md");
            let index_md = dir.join("SKILL.index.md");
            
            if skill_md.exists() {
                let l3 = fs::read_to_string(&skill_md)?;
                let l2 = if summary_md.exists() {
                    fs::read_to_string(&summary_md)?
                } else {
                    extract_first_paragraph(&l3, 200)
                };
                let l1 = if index_md.exists() {
                    fs::read_to_string(&index_md)?
                } else {
                    extract_first_sentence(&l3)
                };
                
                skills.push(DiscoveredSkill {
                    loaded_skill: self.parse_skill_md(&skill_md).await?,
                    l1_index: l1,
                    l2_summary: l2,
                    l3_full_doc: l3,
                });
            }
        }
        
        Ok(skills)
    }
}
```

**Registry 加载策略**：
```rust
impl SkillRegistry {
    /// 按级别加载技能描述
    pub async fn get_skill_description(&self, skill_id: &str, level: SkillLevel) -> Option<String> {
        let skill = self.get(skill_id).await?;
        
        match level {
            SkillLevel::L1 => skill.l1_index.clone(),
            SkillLevel::L2 => skill.l2_summary.clone(),
            SkillLevel::L3 => skill.l3_full_doc.clone(),
        }
    }
}
```

#### 3.5.2 Skill Feedback 自改进循环

**目标**：任务完成后，Agent 自动评估 Skill 的有效性并提出改进建议。

**新增模块**：`crates/agents/src/skills/feedback.rs`

```rust
pub struct SkillFeedback {
    pub skill_id: String,
    pub execution_success: bool,
    pub user_satisfaction: Option<f32>,      // 用户反馈（👍/👎）
    pub llm_self_evaluation: String,         // LLM 自评：哪里做得好/不好
    pub suggested_improvements: Vec<String>,
    pub execution_time_ms: u64,
    pub token_cost: u32,
}

pub struct SkillImprovementEngine;

impl SkillImprovementEngine {
    /// 在任务完成后调用
    pub async fn collect_feedback(
        &self,
        skill_id: &str,
        trail: &ToolTrail,
        user_response: &str,
    ) -> Result<SkillFeedback, SkillError> {
        // 1. 基于 ToolTrail 判断执行是否成功
        let success = trail.steps.iter().all(|s| s.status == StepStatus::Success);
        
        // 2. LLM 自评（调用轻量级 LLM 或本地规则）
        let evaluation = self.llm_evaluate(skill_id, trail, user_response).await?;
        
        Ok(SkillFeedback {
            skill_id: skill_id.to_string(),
            execution_success: success,
            user_satisfaction: None,  // 需要用户显式反馈
            llm_self_evaluation: evaluation.summary,
            suggested_improvements: evaluation.suggestions,
            execution_time_ms: trail.duration_ms(),
            token_cost: trail.total_tokens(),
        })
    }
    
    /// 定期将 Feedback 聚合为 Skill 更新建议
    pub async fn generate_skill_updates(&self) -> Result<Vec<SkillUpdate>, SkillError> {
        // 按月聚合 feedback，生成改进报告
        todo!()
    }
}
```

---

### 3.6 记忆系统增强

#### 3.6.1 Planning 记忆注入

**修改位置**：`crates/agents/src/planning/engine.rs::create_plan()`

```rust
pub async fn create_plan_with_memory(
    &self,
    goal: &str,
    context: &PlanContext,
) -> Result<Plan, PlanningError> {
    // 🆕 FIX: 在创建 Plan 前，检索历史解决方案
    let historical_solutions = if let Some(ref memory) = self.memory_system {
        memory.search(&format!("如何完成: {}", goal), 3).await
    } else {
        vec![]
    };
    
    // 如果有高度相关的历史方案（相似度 > 0.85），直接复用或微调
    if let Some(best_match) = historical_solutions.first() {
        if best_match.similarity > 0.85 {
            tracing::info!("Plan cache hit: reusing historical solution for '{}'", goal);
            return self.adapt_historical_plan(best_match, context).await;
        }
    }
    
    // 否则走正常分解流程
    let mut plan = self.decomposer.decompose(goal).await?;
    
    // 将历史方案作为参考注入 Plan 的 metadata
    plan.metadata.insert("historical_references".to_string(), 
        json!(historical_solutions.iter().map(|s| s.content.clone()).collect::<Vec<_>>()));
    
    Ok(plan)
}
```

#### 3.6.2 SessionDB FTS5 跨会话检索

**现状**：`memory_search.db` 已使用 SQLite FTS5，但 `build_memory_context` 中没有充分利用。

**改进**：在 `message_processor.rs` 中增加跨会话检索：

```rust
async fn search_cross_session_history(
    &self,
    query: &str,
    user_id: &str,
    limit: usize,
) -> Vec<MemoryEntry> {
    // 使用 FTS5 搜索该用户所有历史会话
    let sql = r#"
        SELECT session_id, content, timestamp, rank 
        FROM session_fts 
        WHERE session_fts MATCH ? AND user_id = ?
        ORDER BY rank LIMIT ?
    "#;
    
    // 执行搜索并返回结果
    // ...
}
```

---

### 3.7 安全与成本控制机制

#### 3.7.1 Prompt 缓存与复用

**目标**：相同 System Prompt 的重复请求，降低 40-60% Token 消耗。

**实现**：在 `message_processor.rs` 或 `agent_impl.rs` 中引入 Prompt 哈希缓存。

```rust
pub struct PromptCache {
    cache: RwLock<HashMap<String, (String, Instant)>>,  // hash -> (assembled_prompt, last_used)
    ttl: Duration,
}

impl PromptCache {
    pub fn get_or_build<F>(&self, components: &PromptComponents, builder: F) -> String
    where
        F: FnOnce(&PromptComponents) -> String,
    {
        let hash = self.hash_components(components);
        
        {
            let cache = self.cache.read().unwrap();
            if let Some((prompt, last_used)) = cache.get(&hash) {
                if last_used.elapsed() < self.ttl {
                    tracing::debug!("Prompt cache hit: {}", hash);
                    return prompt.clone();
                }
            }
        }
        
        let prompt = builder(components);
        
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(hash, (prompt.clone(), Instant::now()));
        }
        
        prompt
    }
    
    fn hash_components(&self, components: &PromptComponents) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(components.soul.as_bytes());
        hasher.update(components.user_profile.as_bytes());
        hasher.update(components.active_skills.join(",").as_bytes());
        hasher.update(components.model.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}
```

#### 3.7.2 上下文压缩（Conversation Summarization）

**目标**：长对话超过窗口时，自动压缩历史消息。

**实现**：在 `context/assembler.rs` 中增加压缩策略。

```rust
pub enum HistoryStrategy {
    /// 保留最近 N 条（当前默认）
    RecentN(usize),
    /// 保留开头 + 最近 N 条 + 中间摘要
    SummarizeMiddle {
        keep_head: usize,
        keep_tail: usize,
        summary_tokens: usize,
    },
    /// 逐轮摘要（每轮对话压缩为一句话）
    ProgressiveSummary,
}

impl ContextAssembler {
    pub async fn assemble_with_compression(
        &self,
        messages: &[Message],
        budget: usize,
    ) -> Vec<Message> {
        let estimated_tokens = self.estimate_tokens(messages);
        
        if estimated_tokens <= budget {
            return messages.to_vec();
        }
        
        // 需要压缩
        match self.history_strategy {
            HistoryStrategy::SummarizeMiddle { keep_head, keep_tail, summary_tokens } => {
                let head = &messages[..keep_head.min(messages.len())];
                let tail = &messages[messages.len().saturating_sub(keep_tail)..];
                let middle = &messages[keep_head..messages.len().saturating_sub(keep_tail)];
                
                // 对中间部分调用 LLM 生成摘要
                let summary = self.summarize_messages(middle).await;
                
                let mut result = Vec::new();
                result.extend_from_slice(head);
                result.push(Message::system(format!("[历史对话摘要] {}", summary)));
                result.extend_from_slice(tail);
                
                result
            }
            _ => messages.to_vec(),
        }
    }
}
```

#### 3.7.3 工具输出截断

**目标**：防止 MCP 工具返回的大型 JSON 撑爆上下文。

**修改位置**：`crates/agents/src/mcp/skill_bridge.rs` 或 `agent_impl.rs`

```rust
pub const MAX_TOOL_OUTPUT_CHARS: usize = 4000;

fn truncate_tool_output(output: &str, max_chars: usize) -> String {
    if output.len() <= max_chars {
        return output.to_string();
    }
    
    // 智能截断：尝试保留 JSON 结构
    if output.trim_start().starts_with('{') || output.trim_start().starts_with('[') {
        match serde_json::from_str::<serde_json::Value>(output) {
            Ok(json) => truncate_json_value(&json, max_chars),
            Err(_) => format!("{}...[truncated, {} chars total]", 
                &output[..max_chars], output.len()),
        }
    } else {
        format!("{}...[truncated, {} chars total]", 
            &output[..max_chars], output.len())
    }
}

/// 递归截断 JSON，优先保留关键字段
fn truncate_json_value(value: &serde_json::Value, max_chars: usize) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_len = 2; // {}
            
            for (key, val) in map {
                if key == "error" || key == "status" || key == "symbol" {
                    // 关键字段始终保留
                    truncated.insert(key.clone(), val.clone());
                    current_len += key.len() + val.to_string().len();
                } else if current_len < max_chars * 3 / 4 {
                    // 保留部分非关键字段
                    let val_str = val.to_string();
                    if current_len + key.len() + val_str.len() < max_chars {
                        truncated.insert(key.clone(), val.clone());
                        current_len += key.len() + val_str.len();
                    }
                }
            }
            
            serde_json::Value::Object(truncated).to_string()
        }
        serde_json::Value::Array(arr) => {
            if arr.len() > 5 {
                let mut truncated: Vec<serde_json::Value> = arr.iter().take(3).cloned().collect();
                truncated.push(serde_json::json!(format!("... and {} more items", arr.len() - 3)));
                serde_json::Value::Array(truncated).to_string()
            } else {
                value.to_string()
            }
        }
        _ => value.to_string(),
    }
}
```

#### 3.7.4 Max Rounds 限制

**目标**：防止 Agent 与 LLM 陷入死循环。

**修改位置**：`crates/agents/src/agent_impl.rs`

```rust
pub const DEFAULT_MAX_ROUNDS: u32 = 10;

pub struct AgentConfig {
    // ... existing fields
    pub max_rounds: u32,
}

impl Agent {
    pub async fn process_task_with_round_limit(
        &self,
        task: &Task,
    ) -> Result<(String, Vec<Artifact>), AgentError> {
        let mut rounds = 0;
        let mut current_task = task.clone();
        
        loop {
            rounds += 1;
            if rounds > self.config.max_rounds {
                return Err(AgentError::Execution(
                    format!("达到最大交互轮次限制 ({} 轮)，任务未完成。请简化需求或分步执行。", 
                        self.config.max_rounds)
                ));
            }
            
            let (response, artifacts) = self.process_single_round(&current_task).await?;
            
            // 检查是否完成任务
            if self.is_task_complete(&response) {
                return Ok((response, artifacts));
            }
            
            // 否则进入下一轮（将上一轮结果作为新任务的输入）
            current_task.input = response;
        }
    }
}
```

---

## 四、实施路线图

### Phase 1：意图识别与工具调度优化（2-3 周）

**高优先级、立竿见影的改进：**

| 序号 | 任务 | 涉及文件 | 预估工时 |
|------|------|----------|----------|
| 1.1 | 引入 `UserIntent` 枚举和轻量级规则分类器 | `agent_impl.rs` | 2d |
| 1.2 | 基于意图的路由分发（DirectAnswer 跳过工具注入） | `agent_impl.rs` | 1d |
| 1.3 | 交易意图检测 + `place_*_order` 强制加分（已部分实现） | `agent_impl.rs` | 0.5d |
| 1.4 | Toolsets 分组与分层过滤 | `agent_impl.rs`, `registry.rs` | 3d |
| 1.5 | 破坏性操作审批链（Paper Trading 自动通过） | `security/approval.rs` | 3d |
| 1.6 | ToolDefinition 描述去重（已部分实现） | `agent_impl.rs` | 0.5d |

**预期收益**：
- "查询代替交易"类错误减少 80% 以上。
- DirectAnswer 场景 Token 消耗降低 30%。

### Phase 2：System Prompt 与记忆增强（2-3 周）

| 序号 | 任务 | 涉及文件 | 预估工时 |
|------|------|----------|----------|
| 2.1 | PromptBuilder 模块化组装框架 | `prompt/builder.rs` | 3d |
| 2.2 | 模型特定指令适配层（Kimi/GPT/Claude） | `llm_service.rs`, `config` | 2d |
| 2.3 | Skills L1/L2/L3 渐进加载 | `skills/discovery.rs`, `registry.rs` | 3d |
| 2.4 | `<REASONING_SCRATCHPAD>` 标签支持 | `agent_impl.rs` | 1d |
| 2.5 | Planning 记忆注入（历史方案复用） | `planning/engine.rs` | 2d |
| 2.6 | SessionDB FTS5 跨会话检索 | `memory/`, `message_processor.rs` | 2d |

**预期收益**：
- 单次 Prompt Token 降低 20-40%。
- 同类问题 Planning 时间减少 50%（通过历史方案复用）。

### Phase 3：Planning 与推理升级（3-4 周）

| 序号 | 任务 | 涉及文件 | 预估工时 |
|------|------|----------|----------|
| 3.1 | Intent Analyzer 前置模块 | `planning/engine.rs`, `intent/` | 3d |
| 3.2 | Decomposer 资源分配步骤 | `planning/decomposer.rs` | 3d |
| 3.3 | ToolTrail 执行可视化 | `planning/tool_trail.rs`, `executor.rs` | 4d |
| 3.4 | 经验固化（Memory/Skill 自改进） | `planning/engine.rs`, `skills/feedback.rs` | 4d |
| 3.5 | 工具链压缩（条件工具调用） | `agent_impl.rs` | 3d |

**预期收益**：
- 复杂任务成功率提升 30%。
- Planning 过程可观测、可调试。

### Phase 4：安全与成本兜底（1-2 周）

| 序号 | 任务 | 涉及文件 | 预估工时 |
|------|------|----------|----------|
| 4.1 | Prompt 缓存与复用 | `prompt/builder.rs` | 2d |
| 4.2 | 上下文压缩（SummarizeMiddle） | `context/assembler.rs` | 2d |
| 4.3 | 工具输出智能截断 | `mcp/skill_bridge.rs`, `agent_impl.rs` | 2d |
| 4.4 | Max Rounds 限制 | `agent_impl.rs`, `config` | 1d |
| 4.5 | WASM 沙箱集成（补 TODO） | `security/session_isolation.rs` | 3d |

**预期收益**：
- 长对话场景 Token 消耗降低 40%。
- 杜绝死循环和上下文溢出。

---

## 五、预期收益总结

| 指标 | 当前基线 | 优化后目标 | 提升幅度 |
|------|----------|------------|----------|
| **交易意图准确率** | ~60%（查询误替代） | > 95% | +58% |
| **单次请求 Token** | ~12k（含 Top-20 工具） | ~7k（意图精准过滤） | -42% |
| **复杂任务成功率** | 估计 ~50% | > 80% | +60% |
| **同类问题复用率** | 0%（无历史方案复用） | > 40% | 从无到有 |
| **死循环/失控率** | 偶发（无 max_rounds） | 0% | 完全杜绝 |
| **平均响应延迟** | ~8s（LLM 往返多轮） | ~5s（工具链压缩+缓存） | -38% |

---

## 六、附录：关键代码修改对照表

| 优化点 | 当前代码位置 | 建议修改方式 |
|--------|-------------|-------------|
| 意图分类 | `agent_impl.rs:2548` `domain_keywords` | 替换为 `IntentEngine` 模块 |
| Prompt 组装 | `message_processor.rs:1444` `build_memory_context` | 引入 `PromptBuilder` |
| 工具预过滤 | `agent_impl.rs:1166` `scored_skills` | 增加 Toolsets + 意图加分 |
| Planning 触发 | `agent_impl.rs:698` 字数阈值 | 增加 `IntentAnalyzer` 前置 |
| 技能加载 | `skills/discovery.rs` | 增加 L1/L2/L3 分级 |
| 记忆注入 | `planning/engine.rs` | `create_plan` 前检索历史方案 |
| 输出截断 | `mcp/skill_bridge.rs` | `truncate_tool_output` 函数 |
| 轮次限制 | `agent_impl.rs` | `process_task_with_round_limit` |

---

*本方案基于对 BeeBotOS v1.0.0 实际代码的深度分析，所有改进措施均可直接映射到具体文件和函数，具备可执行性。*
