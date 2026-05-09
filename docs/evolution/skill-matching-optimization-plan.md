# BeeBotOS Skill 精准匹配优化技术方案

> 基于 `agent-intent-skill-comparison.md` 的对比分析，本方案旨在将 BeeBotOS 的意图分析与 Skill 匹配系统全面升级为**纯 LLM 驱动**的架构，彻底移除所有硬编码关键词规则与分类过滤器，建立以 LLM 精准匹配 Skill 为核心的下一代 Skill Runtime。

---

## 1. 设计目标与核心原则

### 1.1 设计目标

| 目标 | 说明 |
|------|------|
| **G0: 零硬编码规则** | 删除所有基于关键词、regex、domain keyword 的硬编码映射与分类逻辑 |
| **G1: 纯 LLM 意图理解** | 用户输入的所有语义理解（是否需要 Skill、需要哪个 Skill、是否需要 Planning）全部由 LLM 处理 |
| **G2: LLM 精准匹配 Skill** | 建立"召回 → 精排 → 选择 → 验证"的四层匹配机制，让 LLM 在可控上下文内做出精准的 Skill 选择 |
| **G3: Planning 与 Skill 原生整合** | Planner 能够根据任务语义自动选择 Skill，无需硬编码映射表 |
| **G4: Progressive Disclosure 完全落地** | L1/L2/L3 分层在匹配流程中真正发挥作用，避免上下文膨胀 |
| **G5: 可观测与自优化** | 每次 Skill 激活都有完整 Trace，支持基于反馈的自动优化 |

### 1.2 核心原则

1. **模型即规则**：所有决策点（意图判断、Skill 选择、Planning 触发、多轮继承）由 LLM 通过结构化 Prompt 完成，不在代码中埋任何业务规则。
2. **上下文受控**：通过 Embedding 初筛将 LLM 需要评估的 Skill 数量限制在 5-10 个，确保精准度与成本可控。
3. **拒绝优于误匹配**：当 LLM 判断无 Skill 适用时，系统必须能够明确拒绝匹配，直接走通用对话路径。
4. **正例 + 负例驱动**：每个 Skill 必须提供 activation_examples（正例）和 activation_negative_examples（负例），LLM 据此学习边界。
5. **渐进式加载**：L1 用于发现，L2 用于评估，L3 用于执行，严格控制每阶段的 Token 消耗。

---

## 2. 总体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        User Query (任意自然语言输入)                          │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  LAYER 0: Retrieval Layer (非 LLM，轻量快速)                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                        │
│  │ Session      │  │ Embedding    │  │ Registry     │                        │
│  │ Inheritance  │  │ Recall       │  │ Search       │                        │
│  │ Check        │  │ (Top-8)      │  │ (Fallback)   │                        │
│  └──────────────┘  └──────────────┘  └──────────────┘                        │
│       │                   │                   │                              │
│       └───────────────────┴───────────────────┘                              │
│                           │                                                  │
│                           ▼                                                  │
│              Candidate Skills Pool (≤ 8 skills, L1 only)                     │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  LAYER 1: Intent Understanding Layer (LLM Call #1，轻量模型即可)              │
│                                                                             │
│  Prompt: "Analyze user query → output JSON:                                  │
│    { direct_answer: bool, needs_skill: bool, needs_planning: bool,           │
│      entities: {...}, constraints: [...], query_summary: "..." }"           │
│                                                                             │
│  Output: IntentAnalysis (纯 LLM 生成，无规则后处理)                          │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                    ┌─────────────────┴─────────────────┐
                    ▼                                   ▼
         direct_answer=true                   needs_skill=true
                    │                                   │
                    ▼                                   ▼
         Skip Skill Layer               ┌──────────────────────────────┐
         (通用对话)                      │  LAYER 2: Skill Ranking      │
                                       │  Layer (LLM Call #2)         │
                                       │                              │
                                       │  Input: Query + Candidates   │
                                       │    (L1 description +         │
                                       │     activation_examples)     │
                                       │                              │
                                       │  Prompt: "Score each skill   │
                                       │    0-10. Select best or      │
                                       │    reject all."              │
                                       │                              │
                                       │  Output: SkillSelection {    │
                                       │    selected: Option<skill_id>│
                                       │    scores: [...],            │
                                       │    reasoning: "..." }        │
                                       └──────────────────────────────┘
                                                          │
                                    ┌─────────────────────┴─────────────────────┐
                                    ▼                                           ▼
                           selected=None                               selected=Some(id)
                                    │                                           │
                                    ▼                                           ▼
                           直接回答（无Skill）                    ┌──────────────────────────┐
                                                                  │ Load L2/L3 on demand     │
                                                                  │ Execute Skill            │
                                                                  └──────────────────────────┘
                                                                               │
                                                                               ▼
                                                                  ┌──────────────────────────┐
                                                                  │ LAYER 3: Execution Layer │
                                                                  │                          │
                                                                  │ • MCP Bridge             │
                                                                  │ • LLM Fallback           │
                                                                  │ • Multi-Skill Pipeline   │
                                                                  └──────────────────────────┘
                                                                               │
                                                                               ▼
                                                                  ┌──────────────────────────┐
                                                                  │ LAYER 4: Trace & Evolve  │
                                                                  │                          │
                                                                  │ • Activation Trace       │
                                                                  │ • Feedback Collection    │
                                                                  │ • Description Auto-tune  │
                                                                  └──────────────────────────┘
```

---

## 3. Phase 1: 纯 LLM 意图理解层

### 3.1 现状问题

当前 `IntentEngine` 包含大量硬编码规则：
- `DEFAULT_TOOLSETS`：8 组预定义关键词映射（account、trading、weather 等）
- `is_correction`：硬编码否定词列表
- `is_meta_question`：硬编码元问题模式
- `has_multi_step_keywords`：硬编码连接词检测
- `count_distinct_actions`：硬编码动作分组
- Gateway 的 `domain relevance check`：硬编码排除关键词（btc、code 等）
- Gateway 的 `exit_keywords`：硬编码退出词
- Gateway 的 skill planning 触发：硬编码 skill 名称判断（travel、planner、analyst 等）
- `PromptBuilder::filter_memories_by_intent`：基于意图关键词的记忆过滤
- `HybridPlanner::select_strategy`：基于关键词选择 planning strategy

### 3.2 改造方案：IntentEngine → LLMIntentAnalyzer

**文件变更**：`crates/agents/src/intent/mod.rs` 重写

**新架构**：
```rust
/// LLM 驱动的意图分析器
pub struct LLMIntentAnalyzer {
    /// 用于意图分析的轻量 LLM（可以是主模型的 fast/low-cost 模式）
    llm: Arc<dyn LLMCallInterface>,
    /// 可选：embedding 模型用于 query 向量化
    embedding: Option<Arc<dyn EmbeddingModel>>,
}

/// 意图分析结果（保留原数据结构，但所有字段由 LLM 填充）
pub struct IntentAnalysis {
    pub intent: UserIntent,
    pub entities: HashMap<String, String>,
    pub constraints: Vec<String>,
    pub confidence: f32,
    pub direct_answer: bool,       // ← 新增：LLM 判断是否可直接回答
    pub needs_planning: bool,      // ← 新增：LLM 判断是否需要规划
    pub query_summary: String,     // ← 新增：LLM 生成的 query 摘要（用于 embedding 检索）
}
```

**关键设计**：LLM 只做**理解**，不做**匹配**。Skill 是否匹配由下一层的 Skill Ranking Layer 决定。

**Prompt 模板**（见第 10 节）要求 LLM 输出结构化 JSON：
```json
{
  "direct_answer": false,
  "needs_planning": true,
  "needs_skill": true,
  "intent": "MultiStepPlanning",
  "entities": {"city": "北京", "days": "5"},
  "constraints": ["预算控制在5000元以内"],
  "query_summary": "用户想要一个北京五天的旅游计划，预算5000元",
  "confidence": 0.92
}
```

**与旧代码的区别**：
- 删除所有 `&[(&str, &[&str])]` 形式的硬编码映射
- 删除所有 `contains()` 关键词检测
- `UserIntent` 枚举保留作为分类标签，但由 LLM 语义判断生成
- 增加 `query_summary` 字段，用于下游 Embedding 检索（比原始 query 更标准化）

### 3.3 Gateway 层硬编码规则清理

**文件变更**：`apps/gateway/src/services/message_processor.rs`

**删除内容**：
1. `exit_keywords` 硬编码列表 → 由 LLM 判断用户是否想退出当前 Skill 上下文
2. `domain relevance check`（code_researcher 排除 btc，alpaca 排除 code 等）→ 由 LLM 或 Embedding 相似度判断
3. Skill planning 触发中的硬编码 skill 名称判断（analytical vs generative）→ 由 LLM 在 Intent Analysis 中输出 `needs_planning`
4. `build_memory_context` 中的 `skip_profiles` 硬编码（travel_planner、weather 跳过 user profile）→ 由 LLM 判断是否需要 user profile

**Session Inheritance 的新逻辑**：
```rust
// 旧逻辑：硬编码 domain relevance check
// 新逻辑：LLM 判断当前 query 是否与 active_skill 相关
async fn should_inherit_skill(
    &self,
    query: &str,
    active_skill_id: &str,
    active_skill_description: &str,
) -> bool {
    // 轻量 LLM prompt："Given user query and skill description, 
    // is the user still referring to this skill's domain?"
    // 或：embedding 相似度（query_embedding · skill_description_embedding > threshold）
}
```

---

## 4. Phase 2: LLM 精准 Skill 匹配（核心）

这是整个方案最关键的部分。当前 BeeBotOS 的 Skill 匹配存在三个核心问题：
1. `skill_catalog` 注入所有 skills，无数量限制，LLM 容易"看花眼"
2. 无 embedding 初筛，候选集过大
3. Skill description 质量参差不齐，缺乏正负例指导
4. 无明确的"拒绝匹配"机制，容易误触发

### 4.1 四层匹配机制

#### Layer 2a: Embedding Recall（非 LLM，< 50ms）

**目标**：从 N 个 skills 中快速召回 Top-K 候选（K=8）。

**实现**：
```rust
pub struct SkillEmbeddingIndex {
    /// skill_id -> embedding vector
    vectors: HashMap<String, Vec<f32>>,
    /// 用于增量更新
    dirty: bool,
}

impl SkillEmbeddingIndex {
    /// 为每个 skill 构建索引向量：
    /// vector = concat[
    ///   embedding(skill.name + ": " + skill.description),
    ///   embedding(skill.capabilities.join(", ")),
    ///   embedding(skill.activation_examples.join("\n")),
    /// ]
    /// 实际实现：取三部分的平均向量或加权拼接
    pub async fn index_skill(&mut self, skill: &RegisteredSkill) {
        let text = format!(
            "{}\n{}\nCapabilities: {}\nExamples: {}",
            skill.skill.name,
            skill.skill.manifest.description,
            skill.skill.manifest.capabilities.join(", "),
            skill.skill.manifest.examples,  // ← 需要新增字段
        );
        let embedding = self.embedding_model.encode(&text).await;
        self.vectors.insert(skill.skill.id.clone(), embedding);
        self.dirty = true;
    }

    /// Query 召回
    pub async fn recall(&self, query: &str, top_k: usize) -> Vec<(String, f32)> {
        let query_vec = self.embedding_model.encode(query).await;
        let mut scored: Vec<(String, f32)> = self.vectors.iter()
            .map(|(id, vec)| {
                let score = cosine_similarity(&query_vec, vec);
                (id.clone(), score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.into_iter().take(top_k).collect()
    }
}
```

**Embedding 模型选择**：
- 初期：使用主 LLM 的 embedding API（如 OpenAI text-embedding-3-small、Kimi embedding）
- 远期：本地轻量模型（如 BGE-small、GTE-base），降低延迟和成本

**Fallback 机制**：当 Embedding 服务不可用时，降级为 `Registry::search`（关键词 overlap）。

#### Layer 2b: Skill 数据结构增强（正例 + 负例）

**文件变更**：`crates/agents/src/skills/loader.rs` 和 `crates/agents/src/skills/registry.rs`

```rust
pub struct SkillManifest {
    // ... 现有字段 ...
    
    /// 🆕 REQUIRED: 何时应该激活此 Skill 的正例（至少 3 个）
    #[serde(default)]
    pub activation_examples: Vec<String>,
    
    /// 🆕 REQUIRED: 何时不应该激活此 Skill 的负例（至少 2 个）
    #[serde(default)]
    pub activation_negative_examples: Vec<String>,
    
    /// 🆕 REQUIRED: 一句话描述"何时使用此 Skill"
    /// 类似 Claude Skills 的 "Use this skill when..."
    pub when_to_use: String,
    
    /// 🆕 可选：此 Skill 不处理什么（边界声明）
    pub when_not_to_use: Option<String>,
    
    /// 🆕 可选：此 Skill 依赖的其他 Skill ID
    pub dependencies: Vec<String>,
}
```

**SKILL.md Schema 统一**：

```markdown
---
name: travel_planner
version: 1.0.0
description: 为用户制定详细的旅行计划，包括行程、住宿、交通和预算
when_to_use: 当用户提到旅行、旅游、行程安排、攻略、去某地玩等需求时
---

## Capabilities
- 根据目的地和天数生成详细行程
- 推荐住宿和餐饮
- 估算交通费用
- 提供预算分配建议

## Activation Examples（正例）
- "我想去云南旅游7天"
- "帮我做个东京五日游攻略"
- "五一假期去哪里玩比较好"
- "带父母去成都，怎么安排"

## Activation Negative Examples（负例）
- "今天天气怎么样"（应使用 weather_assistant）
- "帮我写一封邮件"（应使用 email_writer）
- "AAPL 股价多少"（应使用 stock_query）
- "如何学习 Python"（应使用 code_researcher）

## Prompt Template
...
```

**为什么需要负例**：
- LLM 在面临模糊 query 时，正例只能告诉它"这是什么"
- 负例告诉它"这不是什么"，显著降低误触发率
- 负例中的 "应使用 xxx" 信息还能帮助 LLM 在选错时自我修正

#### Layer 2c: LLM Skill Ranking（LLM Call #2）

**目标**：让 LLM 对 Top-K 候选做精细评估，输出匹配度评分和选择理由。

**输入**：
- 用户原始 query
- `IntentAnalysis.query_summary`
- 最多 8 个候选 Skill 的 L1 信息：name、description、when_to_use、capabilities（前3条）、正例（前2条）、负例（前2条）

**Prompt 核心结构**（完整版见第 10 节）：

```
You are a Skill Matching Judge. Your job is to select the MOST APPROPRIATE skill 
for the user query, or explicitly reject if none match.

## User Query
{query}

## Candidate Skills
[Skill A] {l1_info}
[Skill B] {l1_info}
...

## Rules
1. Score EACH candidate 0-10 based on relevance
2. A score >= 7 means "this skill can handle the query"
3. If NO skill scores >= 7, output selected: null
4. If multiple skills score >= 7, pick the MOST SPECIFIC one
5. Consider negative examples as strong signals of non-match

## Output Format
```json
{
  "selected_skill": "skill_id_or_null",
  "scores": [
    {"skill_id": "A", "score": 8.5, "reason": "..."},
    {"skill_id": "B", "score": 3.0, "reason": "..."}
  ],
  "needs_planning": true,
  "confidence": 0.88,
  "reasoning": "..."
}
```
```

**关键设计决策**：
- **为什么限制 8 个候选**：实验表明，当候选 > 10 时 LLM 的精确度显著下降；8 个是精准度与覆盖度的平衡点
- **为什么要求显式评分**：评分数据可用于后续训练（哪些 skill 在什么 query 下得高分）
- **为什么允许 "null" 选择**：必须给 LLM "拒绝" 的能力，否则它会被迫选一个不太匹配的 skill

#### Layer 2d: Skill Activation Trace（可观测层）

```rust
pub struct SkillActivationTrace {
    pub trace_id: String,
    pub timestamp: DateTime<Utc>,
    pub user_query: String,
    pub intent_analysis: IntentAnalysis,
    /// Retrieval 层结果
    pub retrieval: RetrievalTrace {
        pub method: String,           // "embedding" | "registry_search" | "session_inherit"
        pub candidate_skills: Vec<String>,
        pub recall_scores: Vec<(String, f32)>,
    },
    /// Ranking 层结果
    pub ranking: RankingTrace {
        pub llm_model: String,
        pub scores: Vec<SkillScore>,
        pub selected_skill: Option<String>,
        pub reasoning: String,
        pub confidence: f32,
    },
    /// Execution 层结果
    pub execution: Option<ExecutionTrace>,
    /// 用户反馈（可选）
    pub feedback: Option<UserFeedback>,
}
```

**Trace 存储**：写入 `SkillActivationTrace` 到 memory system 或专用 trace storage，用于：
- 人工审核误匹配
- 自动发现 description 质量问题
- 负例训练数据收集

### 4.2 与现有代码的整合点

**文件**：`crates/agents/src/agent_impl.rs`

**变更点**：
1. `process_task` 中的 intent routing 逻辑简化：
   ```rust
   // 旧：基于硬编码 intent 枚举做 match 分支
   // 新：基于 LLM 输出的 bool 字段做判断
   if intent_analysis.direct_answer {
       self.handle_direct_answer(&task).await
   } else if let Some(skill_id) = intent_analysis.selected_skill {
       if intent_analysis.needs_planning {
           self.execute_with_planning(task).await
       } else {
           self.execute_skill_by_id(&skill_id, &task.input, None).await
       }
   }
   ```

2. `inject_skill_catalog` 重构：
   - 不再注入完整 catalog（所有 skills）
   - 改为注入 "LLM 已选择的 Skill 的 L3 内容" + "少量候选 Skill 的 L1 内容"
   - 或：完全移除 `skill_catalog` 注入，改为显式的 `SkillRanking` 调用

3. `handle_llm_task_internal` 中的 persona 构建：
   - 移除基于 `skill_hint` 的硬编码指令
   - persona 中只包含：base persona + selected skill 的 L3 prompt_template

---

## 5. Phase 3: Planning 与 Skill 无硬编码整合

### 5.1 现状问题

- `HybridPlanner::select_strategy` 基于硬编码关键词选择 planning strategy
- `IntentAnalyzer` 在 planning 中调用 `classify_heuristic`
- Plan step 的 `skill_hint` 无自动填充机制
- Planning 和 Skill 执行是独立的两个阶段

### 5.2 改造方案

#### 5.2.1 Planner 的 LLM 化 Strategy 选择

**删除**：`HybridPlanner::select_strategy` 中的硬编码规则

**新实现**：Strategy 选择由 LLM 在 `IntentAnalysis` 阶段完成（`needs_planning` + `planning_strategy_hint`），或作为 Planning 的输入参数。

```rust
pub struct IntentAnalysis {
    // ...
    pub planning_strategy_hint: Option<PlanningStrategyHint>,
}

pub enum PlanningStrategyHint {
    SingleShot,      // 单轮完成，不需要复杂规划
    ReAct,           // 需要推理-行动循环
    Decompose,       // 需要任务分解
    MultiSkill,      // 需要多个 Skill 协作
}
```

**Prompt 设计**：在 Intent Analysis Prompt 中增加：
```
Also determine the best execution strategy:
- "single_shot": Simple task, can be done in one LLM call
- "react": Task requires reasoning and tool use in sequence  
- "decompose": Task needs to be broken into sub-tasks
- "multi_skill": Task requires combining multiple skills
```

#### 5.2.2 Plan Step 的 Skill 自动绑定

**目标**：Planner 在分解任务时，自动为每个 step 推荐合适的 Skill。

**实现**：
```rust
impl PlanningEngine {
    pub async fn create_plan_with_skills(
        &self,
        goal: &str,
        context: &PlanContext,
        skill_ranking: &SkillRankingResult,  // ← 传入 Skill 匹配结果
    ) -> PlanningResult<Plan> {
        // 1. 先由 LLM 做任务分解（保持现有 Decomposer）
        let mut plan = self.decomposer.decompose(goal, &context)?;
        
        // 2. 为每个 step 绑定 Skill
        for step in &mut plan.steps {
            if let Action::ToolUse { ref mut tool_name, .. } = step.actions.first_mut() {
                // 让 LLM 判断此 step 应该使用哪个 skill
                let step_skill = self.match_skill_for_step(
                    &step.description,
                    &skill_ranking.candidates,
                ).await?;
                if let Some(skill_id) = step_skill {
                    *tool_name = format!("skill:{}", skill_id);
                }
            }
        }
        
        Ok(plan)
    }
}
```

**关键**：`match_skill_for_step` 也是 LLM 驱动，输入是 step description + 可用 skills 列表，输出是 skill_id 或 null。

#### 5.2.3 Multi-Skill Plan 支持

**新增 Action 类型**（`planning/plan.rs`）：
```rust
pub enum Action {
    // ... 现有类型 ...
    
    /// 🆕 顺序执行多个 Skill（Pipeline）
    SkillPipeline {
        steps: Vec<SkillPipelineStep>,
    },
    
    /// 🆕 并行执行多个 Skill
    ParallelSkills {
        skills: Vec<String>,
        merge_strategy: MergeStrategy,
    },
    
    /// 🆕 条件 Skill（如果满足条件则执行）
    ConditionalSkill {
        condition: String,
        if_skill: String,
        else_skill: Option<String>,
    },
}

pub struct SkillPipelineStep {
    pub skill_id: String,
    pub input_transform: String,  // 如何将上一步的输出转换为下一步的输入
}
```

**执行器扩展**（`planning/executor.rs`）：
- `SkillPipeline`：顺序执行，每一步的输出经过 `input_transform`（可以是简单的 JSONPath 提取，也可以是轻量 LLM 转换）后作为下一步输入
- `ParallelSkills`：并发调用多个 Skill，结果按 `MergeStrategy` 合并
- `ConditionalSkill`：由 LLM Judge 判断条件是否满足

---

## 6. Phase 4: Progressive Disclosure 完全落地

### 6.1 当前问题

- `skill_catalog` 注入统一格式的完整列表，无 L1/L2/L3 区分
- `PromptBuilder::build_skills_section` 虽有分层逻辑，但主流程未接入
- `SkillRegistry::get_skill_description` 已实现，但调用方很少使用

### 6.2 改造方案

#### 6.2.1 匹配流程中的 Disclosure 策略

```
┌─────────────────────────────────────────────────────────────┐
│                    Progressive Disclosure Flow               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Step 1: Retrieval (L0/L1 only)                              │
│    • Embedding recall 使用 L1 (name + when_to_use + caps)    │
│    • 每个候选 skill 仅占用 ~50 tokens                        │
│    • 8 个候选 = ~400 tokens                                  │
│                                                              │
│  Step 2: Ranking (L1 + examples)                             │
│    • LLM Ranking Prompt 中注入：                             │
│      - name, description, when_to_use                        │
│      - 前 2 个正例 + 前 2 个负例                             │
│    • 每个候选 ~150 tokens                                    │
│    • 8 个候选 = ~1200 tokens                                 │
│                                                              │
│  Step 3: Execution (L3 full)                                 │
│    • Skill 被选中后，才加载 L3：                             │
│      - prompt_template                                       │
│      - examples                                              │
│      - 关联的 scripts/templates                              │
│    • 仅在执行该 skill 的 LLM call 中注入                     │
│    • 不影响其他对话上下文                                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

#### 6.2.2 代码变更

**`agent_runtime_impl.rs`**：构建 catalog 时按层级区分
```rust
// 旧：统一构建完整 catalog
// 新：只构建 L1 index（用于 embedding 和 ranking）
async fn build_skill_l1_index(&self) -> Vec<SkillL1Index> {
    let mut index = Vec::new();
    if let Some(ref registry) = self.skill_registry {
        for skill in registry.list_enabled().await {
            index.push(SkillL1Index {
                id: skill.skill.id.clone(),
                name: skill.skill.name.clone(),
                when_to_use: skill.skill.manifest.when_to_use.clone(),
                capabilities: skill.skill.manifest.capabilities.iter().take(3).cloned().collect(),
                activation_examples: skill.skill.manifest.activation_examples.iter().take(2).cloned().collect(),
                activation_negative_examples: skill.skill.manifest.activation_negative_examples.iter().take(2).cloned().collect(),
            });
        }
    }
    index
}
```

**`agent_impl.rs`**：
```rust
// 移除 inject_skill_catalog（注入所有 skills）
// 替换为：
async fn prepare_skill_context(
    &self,
    selected_skill: &Option<SkillSelection>,
) -> Vec<Message> {
    match selected_skill {
        None => vec![],  // 无 skill 匹配，不注入任何 skill 上下文
        Some(sel) => {
            // 只注入被选中的 skill 的 L3 内容
            let registry = self.skill_registry.as_ref()?;
            let skill = registry.get(&sel.skill_id).await?;
            let l3 = registry.get_skill_description(&sel.skill_id, SkillDisclosureLevel::L3).await;
            vec![Message::new(
                uuid::Uuid::new_v4(),
                PlatformType::Custom,
                format!("[Skill Context] You are now using the '{}' skill.\n{}", 
                    skill.skill.name, l3.unwrap_or_default()),
            )]
        }
    }
}
```

---

## 7. Phase 5: 多 Skill 编排与执行

### 7.1 设计目标

当前系统限制为单 Skill 匹配，复杂任务需要支持：
- **Sequential**：Skill A 的输出 → Skill B 的输入（如：搜索 → 分析 → 生成报告）
- **Parallel**：多个 Skill 同时执行，结果合并（如：同时查天气和汇率）
- **Conditional**：根据中间结果决定下一步 Skill（如：如果股价 > X 则卖出，否则持有）

### 7.2 实现方案

#### 7.2.1 Skill Composition DSL

在 SKILL.md 中支持声明式编排：

```markdown
## Composition
```yaml
pipeline:
  - skill: web_search
    output_as: search_results
  - skill: content_summarizer
    input: "${search_results}"
    output_as: summary
  - skill: report_generator
    input: "${summary}"
```
```

或在 Plan 中由 LLM 动态生成（推荐）：

```rust
// Plan 执行时，LLM 判断是否需要多 Skill
async fn execute_step_with_skill_routing(
    &self,
    step: &PlanStep,
    context: &ExecutionContext,
) -> Result<StepResult> {
    // 1. 检查 step 是否涉及多个能力域
    let routing_decision = self.llm_skill_router.route(
        &step.description,
        &context.available_skills,
    ).await?;
    
    match routing_decision.strategy {
        SingleSkill(skill_id) => {
            self.execute_skill(&skill_id, &step.description).await
        }
        SequentialPipeline(steps) => {
            let mut last_output = String::new();
            for pipe_step in steps {
                last_output = self.execute_skill(
                    &pipe_step.skill_id, 
                    &pipe_step.format_input(&last_output)
                ).await?;
            }
            Ok(StepResult::new(last_output))
        }
        ParallelBatch(skills) => {
            let futures: Vec<_> = skills.into_iter()
                .map(|s| self.execute_skill(&s, &step.description))
                .collect();
            let results = futures::future::join_all(futures).await;
            let merged = self.merge_parallel_results(results, &routing_decision.merge_strategy).await?;
            Ok(StepResult::new(merged))
        }
    }
}
```

#### 7.2.2 Skill Router Prompt

```
You are a Skill Router. Given a task description and available skills, 
determine the optimal execution strategy.

## Task
{step_description}

## Available Skills
{skills_l1}

## Rules
- "single": One skill can handle the entire task
- "sequential": Task must be done in stages, output of stage N is input to stage N+1
- "parallel": Multiple independent sub-tasks can be executed simultaneously
- "conditional": Different skills based on intermediate results

## Output
```json
{
  "strategy": "single|sequential|parallel|conditional",
  "skills": ["skill_id_1", "skill_id_2"],
  "reasoning": "...",
  "input_transforms": {...}  // for sequential: how to transform outputs
}
```
```

---

## 8. Phase 6: Skill Activation Trace 与自优化

### 8.1 Trace 系统

每次 Skill 匹配全流程记录：

```rust
// 写入 trace storage（SQLite / 专用表）
pub async fn record_activation_trace(&self, trace: &SkillActivationTrace) {
    // 结构化存储，便于查询：
    // SELECT * FROM skill_traces 
    // WHERE selected_skill = 'travel_planner' 
    // AND ranking_confidence < 0.7
}
```

### 8.2 自动发现质量问题

**定时任务**（每天运行一次）：
```rust
async fn analyze_skill_quality(&self) -> Vec<QualityIssue> {
    let issues = vec![];
    
    // 1. 低置信度匹配：同一 skill 多次被低置信度选中
    let low_confidence = self.traces
        .filter(|t| t.ranking.confidence < 0.6)
        .group_by(|t| t.ranking.selected_skill.clone())
        .filter(|(_, group)| group.len() > 5)
        .collect();
    
    // 2. 高误触发率：选中后用户反馈"不对"/"不是这个"
    let false_activations = self.traces
        .filter(|t| t.feedback.as_ref().map(|f| f.was_correct == false).unwrap_or(false))
        .group_by(|t| t.ranking.selected_skill.clone())
        .collect();
    
    // 3. 漏触发：用户明确提到某能力，但系统选择了 direct_answer
    let missed = self.traces
        .filter(|t| t.ranking.selected_skill.is_none() && t.user_query.contains("skill_keyword"))
        .collect();
    
    issues
}
```

### 8.3 Description 自动优化

基于 Trace 数据，定期生成优化建议：

```rust
async fn generate_description_improvement(&self, skill_id: &str) -> Option<String> {
    let traces = self.get_traces_for_skill(skill_id).await;
    
    // 收集误匹配案例
    let false_positives: Vec<String> = traces
        .iter()
        .filter(|t| t.is_false_positive())
        .map(|t| t.user_query.clone())
        .collect();
    
    // 收集漏匹配案例  
    let false_negatives: Vec<String> = traces
        .iter()
        .filter(|t| t.is_false_negative())
        .map(|t| t.user_query.clone())
        .collect();
    
    // 让 LLM 生成优化后的 description + when_to_use + 负例
    let prompt = format!(
        "Given a skill and its misclassification cases, improve its description.\n\n\
         Skill: {}\n\
         Current description: {}\n\
         False positives (wrongly triggered): {:?}\n\
         False negatives (missed): {:?}\n\n\
         Output improved: description, when_to_use, activation_examples, activation_negative_examples",
        skill_id, current_desc, false_positives, false_negatives
    );
    
    self.llm.call(prompt).await.ok()
}
```

---

## 9. 数据模型变更汇总

### 9.1 新增结构

```rust
// crates/agents/src/intent/mod.rs
pub struct LLMIntentAnalyzer { ... }
pub struct IntentAnalysis {
    pub intent: UserIntent,
    pub entities: HashMap<String, String>,
    pub constraints: Vec<String>,
    pub confidence: f32,
    pub direct_answer: bool,              // NEW
    pub needs_planning: bool,             // NEW
    pub planning_strategy_hint: Option<PlanningStrategyHint>, // NEW
    pub query_summary: String,            // NEW
    pub selected_skill: Option<String>,   // NEW (可以是 Layer 2 的输出)
}

// crates/agents/src/skills/loader.rs  
pub struct SkillManifest {
    // ... existing fields ...
    pub when_to_use: String,                          // NEW (REQUIRED)
    pub when_not_to_use: Option<String>,              // NEW
    pub activation_examples: Vec<String>,             // NEW (REQUIRED, min 3)
    pub activation_negative_examples: Vec<String>,    // NEW (REQUIRED, min 2)
    pub dependencies: Vec<String>,                    // NEW
}

// crates/agents/src/skills/registry.rs
pub struct SkillEmbeddingIndex { ... }                // NEW

// crates/agents/src/skills/activation.rs (NEW FILE)
pub struct SkillSelection {
    pub selected_skill: Option<String>,
    pub scores: Vec<SkillScore>,
    pub confidence: f32,
    pub reasoning: String,
    pub needs_planning: bool,
}
pub struct SkillActivationTrace { ... }               // NEW
```

### 9.2 删除/废弃

- `IntentEngine::classify_heuristic` → 删除，替换为 `LLMIntentAnalyzer::analyze`
- `IntentEngine::classify_dual_track` → 删除
- `DEFAULT_TOOLSETS` → 删除
- `detect_toolsets` → 删除
- `extract_symbols` / `extract_quantity` / `extract_constraints` → 删除（由 LLM 统一提取）
- Gateway `domain relevance check` 硬编码 → 删除
- Gateway `exit_keywords` → 删除
- Gateway skill planning 硬编码触发 → 删除
- `HybridPlanner::select_strategy` 硬编码 → 删除

---

## 10. Prompt 设计

### 10.1 Prompt 1: LLM Intent Analysis

```
You are an Intent Analysis Engine. Analyze the user message and output 
a structured JSON response. Do not output any text outside the JSON.

## User Message
{user_message}

## Conversation History (last 3 turns)
{history}

## Instructions
1. Determine if the user is asking for a direct conversational response 
   (greeting, chit-chat, simple factual question, opinion) OR if they 
   need a specialized skill/tool.
2. If a skill is needed, extract any entities mentioned (locations, dates, 
   amounts, symbols, etc.).
3. Identify any constraints (budget limits, time restrictions, preferences).
4. Determine if the task requires multi-step planning.
5. Generate a concise summary of the query for retrieval purposes.

## Output Format
```json
{
  "direct_answer": true/false,
  "needs_skill": true/false,
  "needs_planning": true/false,
  "planning_strategy_hint": "single_shot|react|decompose|multi_skill|null",
  "intent": "DirectAnswer|SingleToolCall|MultiStepPlanning|WorkflowTrigger|MetaQuestion|Correction",
  "entities": {"key": "value"},
  "constraints": ["..."],
  "query_summary": "concise summary for embedding search",
  "confidence": 0.0-1.0
}
```

## Rules
- "direct_answer": true ONLY for greetings, small talk, simple Q&A, 
  meta-questions about capabilities, or when no specialized skill applies.
- "needs_planning": true when the task has multiple steps, dependencies, 
  or requires sequential tool use.
- "query_summary": Should be a normalized, search-friendly description 
  (e.g., "travel plan Beijing 5 days budget 5000").
- Set "confidence" based on how clear the user intent is.
```

### 10.2 Prompt 2: Skill Ranking

```
You are a Skill Matching Judge. Your task is to evaluate which skill, 
if any, best matches the user's query.

## User Query
{user_query}

## Query Summary
{query_summary}

## Candidate Skills
{for each candidate}
### [{index}] {skill_name} (id: {skill_id})
- When to use: {when_to_use}
- Description: {description}
- Capabilities: {capabilities}
- Examples of correct usage:
{activation_examples}
- Examples of INCORRECT usage (do NOT match these):
{activation_negative_examples}
{end for}

## Evaluation Criteria
1. Relevance (0-10): How well does the skill's purpose align with the query?
2. Specificity (0-10): Is this the MOST specific skill for the task, 
   or is it too general?
3. Capability match (0-10): Does the skill actually have the capabilities 
   needed?
4. Negative example check: If the query resembles a negative example, 
   score must be <= 3.

## Rules
- A skill is "selected" only if its overall score >= 7.0
- If multiple skills score >= 7.0, select the one with highest specificity
- If NO skill scores >= 7.0, selected_skill must be null
- NEVER select a skill just because it's the "closest" match — 
  if nothing truly fits, reject all

## Output Format
```json
{
  "selected_skill": "skill_id_or_null",
  "needs_planning": true/false,
  "confidence": 0.0-1.0,
  "scores": [
    {
      "skill_id": "...",
      "relevance": 0-10,
      "specificity": 0-10,
      "capability_match": 0-10,
      "overall_score": 0-10,
      "reason": "..."
    }
  ],
  "selection_reasoning": "Detailed explanation of why this skill was selected or why all were rejected"
}
```
```

### 10.3 Prompt 3: Session Skill Inheritance

```
Determine if the current user message is still related to the active skill.

## Active Skill
Name: {skill_name}
Description: {skill_description}
When to use: {when_to_use}

## User Message
{user_message}

## Instructions
- If the user is continuing a conversation about the skill's domain → "inherit"
- If the user has clearly switched topics → "switch"
- If the user is ending the conversation (goodbye, thanks, done) → "exit"

## Output
```json
{"decision": "inherit|switch|exit", "confidence": 0.0-1.0, "reason": "..."}
```
```

---

## 11. 实施路线图

### Stage 1: 基础设施（Week 1-2）
- [ ] 统一 SKILL.md schema，增加 `when_to_use`、`activation_examples`、`activation_negative_examples`
- [ ] 重写 `builtin_loader.rs` 解析新字段
- [ ] 建立 `SkillActivationTrace` 数据表
- [ ] 实现 `SkillEmbeddingIndex`（先用 Registry search 做 fallback）

### Stage 2: 意图层重构（Week 3）
- [ ] 实现 `LLMIntentAnalyzer`（Prompt 1）
- [ ] 删除 `IntentEngine` 所有硬编码规则
- [ ] 在 `agent_impl.rs` 中接入新 Intent Analyzer
- [ ] Gateway 层删除硬编码 relevance check / exit keywords / planning trigger

### Stage 3: Skill 匹配核心（Week 4-5）
- [ ] 实现 Embedding Recall Layer
- [ ] 实现 LLM Skill Ranking（Prompt 2）
- [ ] 实现 Skill Selection 的 "拒绝" 路径
- [ ] 接入 `SkillActivationTrace`
- [ ] A/B 测试：新系统 vs 旧系统的匹配准确率

### Stage 4: Progressive Disclosure（Week 6）
- [ ] 改造 `inject_skill_catalog` → 只注入 L1 用于 Ranking，L3 用于 Execution
- [ ] 按 `usage_count` 排序 + 限制数量
- [ ] 删除 `PromptBuilder` 中的硬编码意图关键词过滤

### Stage 5: Planning 整合（Week 7-8）
- [ ] 删除 `HybridPlanner::select_strategy` 硬编码
- [ ] Planner 接入 `SkillRankingResult`
- [ ] Plan Step 自动绑定 Skill
- [ ] 支持 Multi-Skill Pipeline（Sequential）

### Stage 6: 多 Skill 编排与优化（Week 9-10）
- [ ] Parallel Skill 执行
- [ ] Conditional Skill 路由
- [ ] Skill Router Prompt（Prompt 4）
- [ ] 基于 Trace 的 description 自动优化建议

---

## 12. 风险与回退策略

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| LLM Skill Ranking 延迟增加（+1 次 LLM call） | 响应时间增加 200-500ms | 使用轻量模型（如 GPT-3.5/Kimi-lite）做 ranking；异步预加载候选 skill |
| Embedding 服务不可用 | 无法召回候选 | Fallback 到 Registry::search；本地轻量 embedding 模型 |
| LLM 误匹配率高于旧规则系统 | 用户体验下降 | A/B 测试验证；保留 "拒绝" 机制；渐进式灰度 rollout |
| Skill description 质量不足 | 匹配精度低 | 强制 schema 校验（正例/负例必填）；CI 检查 |
| Token 成本上升 | 运营成本增加 | Embedding 初筛减少候选数；L1/L2/L3 分层控制上下文大小 |

**回退策略**：
- 每个阶段都保留 `feature flag`，可通过配置切回旧逻辑
- `IntentAnalysis` 保留 `confidence` 字段，低置信度时降级到直接回答
- 建立 "Skill 匹配质量看板"，实时监控误匹配率、拒绝率、平均置信度

---

## 13. 预期效果

| 指标 | 当前 | 目标 |
|------|------|------|
| Skill 误触发率 | ~15-20%（估算，基于硬编码规则局限） | < 5% |
| Skill 漏触发率 | ~10-15%（复杂意图无法被规则覆盖） | < 5% |
| 平均 Skill 匹配置信度 | 无统一指标 | > 0.85 |
| 每轮对话 Skill 上下文 Token | 全部 skills 的 description | 仅 8 个 L1 候选 (~400 tokens) + 1 个 L3 执行 (~2000 tokens) |
| Planning 硬编码规则数量 | ~20 条（关键词映射） | 0 |
| 可观测性 | 无结构化 trace | 100% 匹配有完整 trace |

---

*文档版本：v1.0*
*生成时间：2026-05-08*
*依赖文档：`agent-peng-prompt.md`、`agent-intent-skill-comparison.md`*
