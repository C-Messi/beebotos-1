# BeeBotOS 自主进化系统技术设计文档 v1.0

> **文档编号**: BEE-EVOL-TECH-v1  
> **版本**: v1.0  
> **日期**: 2026-05-07  
> **状态**: 技术设计（Tech Design）  
> **适用范围**: `crates/agents` 核心进化模块  

---

## 一、设计目标与核心原则

### 1.1 设计目标

BeeBotOS 自主进化系统旨在构建一套**从经验到本能**的闭环进化基础设施，使 Agent 在持续服务过程中实现三类核心能力的自我增强：

| 进化维度 | 核心目标 | 用户感知 |
|----------|----------|----------|
| **记忆进化** | 越用越懂你 | Agent 记住你的偏好、项目背景、沟通风格 |
| **技能进化** | 越用越会做 | Agent 从成功任务中自动提炼可复用技能 |
| **策略进化** | 从经验到本能 | Agent 的 Prompt 和决策策略通过 RL 持续优化 |

### 1.2 核心原则

1. **单 Agent 进化优先**：当前阶段聚焦于单智能体内部进化闭环，不涉及群智能体协作进化或 A2A 跨智能体知识迁移。
2. **经验驱动，非预设驱动**：进化触发源于真实任务轨迹，而非人工编写的规则库。
3. **渐进式、可回滚**：所有进化操作采用 Patch 优先策略，保留历史版本，支持一键回滚。
4. **安全边界内进化**：记忆/技能/Prompt 的最终产物进入系统提示词，是一等安全边界，必须通过安全扫描。
5. **明确排除项**：
   - **不引入 PAD（Pleasure-Arousal-Dominance）情感标签体系**：情感状态建模增加复杂度且缺乏可验证性。
   - **不引入 OCEAN（Big Five）人格权重体系**：人格维度对工具型 Agent 的决策优化增益有限，且易引入偏见。

---

## 二、总体架构

### 2.1 三层进化架构

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                         BeeBotOS Auto-Evolution Stack                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                   Agent Evolution Layer (策略层)                      │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │   │
│  │  │   CAPO       │  │  Atropos     │  │   DAPO + PAPO            │  │   │
│  │  │  Prompt优化  │──│  异步协调器  │──│   RL 训练引擎            │  │   │
│  │  │  SKILL.md    │  │  轨迹收集    │  │   过程级奖励             │  │   │
│  │  │  SOUL.md     │  │  环境管理    │  │   熵崩溃防护             │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                  Skill Evolution Layer (技能层)                       │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │   │
│  │  │  Auto-       │  │  Skill       │  │  Progressive             │  │   │
│  │  │  Distiller   │──│  Lineage     │──│  Disclosure              │  │   │
│  │  │  自动提炼    │  │  谱系追踪    │  │  L0/L1/L2 三级披露      │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                 Memory Evolution Layer (记忆层)                       │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │   │
│  │  │  Tiered      │  │  Nudge       │  │  Active                  │  │   │
│  │  │  Persistence │──│  Engine      │──│  Consolidation           │  │   │
│  │  │  L0~L3 分层  │  │  主动提醒    │  │  主动压缩与去重          │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 进化闭环数据流

```text
        ┌─────────────┐
        │  用户任务   │
        └──────┬──────┘
               │
               ▼
        ┌─────────────┐     ┌─────────────┐
        │   Planning  │────▶│  ToolTrail  │
        │   & Execute │     │  轨迹记录   │
        └──────┬──────┘     └──────┬──────┘
               │                    │
               ▼                    ▼
        ┌─────────────┐     ┌─────────────┐
        │   Result    │     │   Nudge     │
        │   Feedback  │     │   Engine    │
        └──────┬──────┘     └──────┬──────┘
               │                    │
               └────────┬───────────┘
                        ▼
               ┌─────────────────┐
               │  Evolution      │
               │  Pipeline       │
               │  (CAPO+Atropos) │
               └────────┬────────┘
                        │
           ┌────────────┼────────────┐
           ▼            ▼            ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐
    │  Memory  │ │  Skill   │ │  Prompt  │
    │  Update  │ │  Create  │ │  Optimize│
    │  (L1-L3) │ │  /Patch  │ │  (CAPO)  │
    └──────────┘ └──────────┘ └──────────┘
```

### 2.3 与现有代码模块映射

| 进化子系统 | 新增模块 | 复用/扩展现有模块 |
|-----------|---------|-----------------|
| 记忆进化 | `evolution/memory_nudge.rs` | `memory/search.rs`, `memory/markdown_storage.rs`, `memory/hybrid_search_sqlite.rs` |
| 技能进化 | `evolution/skill_distiller.rs`, `evolution/skill_lineage.rs` | `skills/discovery.rs`, `skills/feedback.rs`, `skills/registry.rs` |
| 策略进化 | `evolution/capo.rs`, `evolution/dapo.rs`, `evolution/papo.rs`, `evolution/atropos.rs` | `planning/tool_trail.rs`, `context/assembler.rs`, `prompt/builder.rs` |

---

## 三、记忆自主进化系统（Memory Auto-Evolution）

### 3.1 设计理念

**"越用越懂你"** 不是通过情感建模实现，而是通过**高频事实的精确沉淀**和**跨会话上下文的主动召回**实现。记忆系统拒绝 PAD/OCEAN 等心理学维度抽象，转而采用**纯行为事实记录**：用户做了什么选择、偏好什么风格、在什么场景下成功/失败。

### 3.2 四层分级持久化架构（L0 ~ L3）

BeeBotOS 在 Hermes 四层模型基础上，针对工程型 Agent 做适应性调整：

#### L0: 热记忆（Hot Memory）—— 会话内上下文
- **载体**: `PlanContext.history` + `ToolTrail`
- **生命周期**: 当前会话活跃，会话结束后自动压缩归档
- **容量**: 受 `ContextAssembler` 的 `context_window` 约束（默认 128K tokens）
- **进化机制**: 
  - 会话结束时，Nudge Engine 评估热记忆中是否有值得长期保留的事实
  - 通过 `SummarizeMiddle` 策略压缩为 L1/L2 可接受的格式

#### L1: 项目记忆（Project Memory）—— `MEMORY.md`
- **载体**: `~/.beebotos/memory/{project_id}/MEMORY.md`
- **内容边界**:
  - 环境事实：技术栈、工具链版本、项目约定
  - 高频操作模式："每次部署前自动跑测试"
  - 已知陷阱："此服务需要先启动 Redis"
- **容量限制**: **严格 ≤ 2200 字符**（约 800 tokens）
- **进化机制**:
  - Nudge Engine 每 10 个用户回合触发一次复盘
  - LLM 评估新事实是否值得写入，执行 `add` / `replace` / `remove`
  - 写入前必须通过**安全扫描**（过滤凭证、后门提示）
- **与现有代码对齐**: 复用 `memory::markdown_storage::MarkdownStorage`

#### L2: 用户画像（User Profile）—— `USER.md`
- **载体**: `~/.beebotos/memory/{user_id}/USER.md`
- **内容边界**:
  - 沟通风格偏好："回答简洁，代码用制表符"
  - 领域知识："熟悉 Rust 异步编程，不熟悉前端"
  - 常用工具链："优先用 cargo，不用 npm"
- **容量限制**: **严格 ≤ 1375 字符**（约 500 tokens）
- **进化机制**:
  - 与 L1 共用 Nudge Engine，但触发条件更严格（需跨项目复用价值）
  - **明确排除**: 不记录 PAD 情感状态、不建模 OCEAN 人格维度
  - 仅记录**可观察的行为事实**，而非心理推断
- **注入策略**: `PromptBuilder` 始终将 L2 注入系统提示词，作为模型自适应的基础

#### L3: 全量历史（Full History）—— SQLite + FTS5
- **载体**: `~/.beebotos/memory/search.db`（HybridSearchSqlite）
- **内容**: 所有对话的完整归档，含工具调用链和结果
- **检索机制**:
  - **FTS5 全文检索**: 毫秒级关键词匹配
  - **Vector 语义检索**: 基于 embedding 的相似度搜索
  - **Cross-Session 检索**: `MemorySearch::search_cross_session()` 已支持 user_id 过滤
- **进化机制**:
  - 自动 TTL 清理：30 天前的低频记录自动归档到冷存储
  - LLM 摘要注入：检索结果不是原始对话，而是压缩后的摘要
- **与现有代码对齐**: 复用 `memory::hybrid_search_sqlite::HybridSearchSqlite`

### 3.3 Nudge Engine —— 主动记忆沉淀

Nudge Engine 是记忆进化的**主动触发器**，不是被动等待写入。

#### 3.3.1 架构设计

```rust
/// 主动记忆沉淀引擎
pub struct NudgeEngine {
    /// 回合计数器，每 N 个用户回合触发一次复盘
    turn_counter: AtomicU64,
    /// 触发阈值配置
    config: NudgeConfig,
    /// 记忆质量评估器
    quality_evaluator: MemoryQualityEvaluator,
    /// 写入执行器
    writer: MemoryWriter,
}

pub struct NudgeConfig {
    /// 每多少个用户回合触发一次 Memory Nudge
    pub memory_nudge_interval: u64,
    /// 每多少个成功技能执行触发一次 Skill Nudge
    pub skill_nudge_interval: u64,
    /// 最小任务复杂度（工具调用次数）才值得沉淀
    pub min_tool_calls_for_memory: usize,
    /// 单条记忆最大字符数
    pub max_memory_entry_chars: usize,
}
```

#### 3.3.2 触发条件与流程

**Memory Nudge 触发条件**（满足任一）：
1. 用户回合数达到 `memory_nudge_interval`（默认 10）
2. Agent 主动调用记忆工具（计数器重置，避免重复）
3. 任务成功且工具调用次数 ≥ `min_tool_calls_for_memory`
4. 用户显式反馈（"记住这个" / "以后都这样"）

**Nudge 执行流程**:
```text
1. 捕获当前会话快照（ToolTrail + 用户反馈）
2. LLM 评估：哪些信息值得长期保留？
   - 是否是稳定事实（非临时状态）？
   - 是否具有跨会话复用价值？
   - 是否已在 L1/L2 中存在（避免重复）？
3. 若通过评估，生成 Patch 提案
4. 安全扫描（过滤凭证、敏感词）
5. 执行写入 / 更新 / 去重
6. 生成记忆摘要，存入 L3（SQLite）
```

#### 3.3.3 记忆质量评估器

```rust
/// 评估新信息是否值得沉淀为长期记忆
pub struct MemoryQualityEvaluator;

impl MemoryQualityEvaluator {
    /// 返回 0.0 ~ 1.0 的质量分，≥0.6 才写入
    pub fn evaluate(&self, candidate: &MemoryCandidate, existing: &[MemoryEntry]) -> f32 {
        let mut score = 0.0;
        
        // 稳定性：非临时状态 +0.3
        if candidate.is_stable_fact { score += 0.3; }
        
        // 复用价值：跨场景适用性 +0.3
        if candidate.cross_session_value { score += 0.3; }
        
        // 新颖性：与现有记忆不重复 +0.2
        if !self.is_redundant(candidate, existing) { score += 0.2; }
        
        // 用户确认：显式或隐式反馈 +0.2
        if candidate.has_user_confirmation { score += 0.2; }
        
        score
    }
    
    fn is_redundant(&self, candidate: &MemoryCandidate, existing: &[MemoryEntry]) -> bool {
        // 使用向量相似度 + BM25 混合判断冗余
        // 若与某条现有记忆相似度 > 0.85，视为冗余
        existing.iter().any(|e| {
            let sim = cosine_similarity(&candidate.embedding, &e.embedding);
            sim > 0.85
        })
    }
}
```

### 3.4 主动压缩与去重（Active Consolidation）

当 L1 (`MEMORY.md`) 或 L2 (`USER.md`) 接近容量上限时，系统自动触发**主动压缩**：

1. **去重**: 使用 Hybrid Search 找到相似记忆，LLM 判断合并为单条
2. **摘要**: 将多条相关事实压缩为更精炼的表达
3. **归档**: 被压缩的原始记录降级到 L3（SQLite），保留可追溯性
4. **驱逐**: 最低频、最低质量的记录标记为冷数据，不进入系统提示词

### 3.5 与 PromptBuilder 的集成

```rust
impl PromptBuilder {
    /// 组装系统提示词时注入记忆
    pub fn build_with_memory(&self, intent: &UserIntent) -> String {
        let mut parts = Vec::new();
        
        // 1. SOUL.md（人格定义，用户自定义）
        if let Some(soul) = &self.soul { parts.push(soul.clone()); }
        
        // 2. L2 用户画像（始终注入）
        if let Some(user_profile) = &self.user_profile { 
            parts.push(format!("[用户偏好]\n{}", user_profile)); 
        }
        
        // 3. L1 项目记忆（始终注入）
        if let Some(project_memory) = &self.project_memory {
            parts.push(format!("[项目约定]\n{}", project_memory));
        }
        
        // 4. L3 情景记忆（按需注入，仅当意图匹配时）
        let relevant_history = self.filter_history_by_intent(intent);
        if !relevant_history.is_empty() {
            parts.push(format!("[历史参考]\n{}", relevant_history.join("\n")));
        }
        
        parts.join("\n\n")
    }
}
```

---

## 四、Skills 技能自主进化系统（Skill Auto-Evolution）

### 4.1 设计理念

**"越用越会做"** 通过三类机制实现：
1. **自动提炼（Auto-Distillation）**：从成功任务轨迹中提取通用操作流程
2. **渐进披露（Progressive Disclosure）**：根据意图复杂度动态加载 Skill 的不同深度
3. **谱系追踪（Lineage Tracking）**：记录 Skill 的演化历史，支持回滚和归因

### 4.2 自动提炼引擎（Skill Distiller）

#### 4.2.1 触发条件

Skill 不是人工编写，而是从**真实任务轨迹**中自动提炼。触发条件（满足任一）：

| 条件 | 阈值 | 说明 |
|------|------|------|
| 工具调用复杂度 | ≥ 5 次工具调用 | 复杂流程值得固化 |
| 自我修复 | 出错并成功恢复 | 踩坑经验是高质量技能素材 |
| 用户显式确认 | "好" / "保存这个流程" | 人工标注的高质量轨迹 |
| 隐式采纳 | 用户未修改直接使用 | 默认接受即认可 |
| 模式新颖性 | 与现有 Skill 相似度 < 0.3 | 全新工作流 |

#### 4.2.2 提炼流程

```text
┌─────────────────────────────────────────────────────────────┐
│                    Skill Distillation Pipeline               │
├─────────────────────────────────────────────────────────────┤
│  Input: ToolTrail (成功任务轨迹)                              │
│                                                              │
│  Step 1: 轨迹清洗                                            │
│    - 去除用户隐私信息（账号、密钥、个人路径）                   │
│    - 去除临时状态（session_id、临时文件名）                    │
│    - 保留通用化后的步骤序列                                    │
│                                                              │
│  Step 2: 关键决策点提取                                       │
│    - LLM 分析轨迹中的分支逻辑                                  │
│    - 提取 "IF ... THEN ... ELSE ..." 决策模式                 │
│    - 记录失败路径和修复方案（Pitfalls）                        │
│                                                              │
│  Step 3: 通用化抽象                                          │
│    - 将具体参数替换为占位符（如 "AAPL" → "{symbol}"）          │
│    - 将具体路径替换为变量（如 "/home/alice" → "{project_root}"）│
│                                                              │
│  Step 4: 验证逻辑生成                                        │
│    - 为每个步骤生成自动验证方法                                │
│    - 如："检查返回 JSON 中 status 字段是否为 'ok'"             │
│                                                              │
│  Step 5: 质量评分                                            │
│    - 0-10 分制，由轻量 LLM（本地小模型）评分                   │
│    - ≥ 6 分才进入下一步，≥ 8 分标记为"高置信度"                │
│                                                              │
│  Step 6: 去重与谱系关联                                      │
│    - Hybrid Search 查找相似 Skill                             │
│    - 相似度 ≥ 0.7 → Patch 更新现有 Skill                      │
│    - 相似度 < 0.3 → 创建新 Skill                              │
│                                                              │
│  Output: SKILL.md (agentskills.io 标准格式)                   │
└─────────────────────────────────────────────────────────────┘
```

#### 4.2.3 SKILL.md 标准格式

生成的 Skill 遵循 `agentskills.io` 开放标准，确保跨 Agent 兼容：

```markdown
# Skill: {skill_id}

## 元信息
- **version**: 1.0.0
- **lineage**: [{parent_id}]
- **confidence**: 8.2
- **created_from**: task_{task_id}
- **auto_generated**: true

## 适用场景
{一句话描述触发条件}

## 执行步骤
1. {步骤1描述}
   - 工具: {tool_id}
   - 参数: {参数模板}
   - 验证: {验证逻辑}
2. {步骤2描述}
   ...

## 关键决策点
- **IF** {条件1} **THEN** {分支A} **ELSE** {分支B}

## 已知陷阱 (Pitfalls)
- {陷阱1}: {避坑方案}
- {陷阱2}: {避坑方案}

## 验证逻辑
```python
# 自动验证脚本（可选）
def validate(result):
    assert result.status == "ok"
```

## 谱系历史
- v1.0.0 (2026-05-07): 自动从 task_xxx 提炼
```

### 4.3 渐进披露系统（Progressive Disclosure）

Skill 采用三级披露模式，根据 `UserIntent` 动态加载，节省 Token：

#### L0: 索引层（Skill List）
- **内容**: Skill ID + 一句话描述 + 标签
- **Token 成本**: ~50 tokens / Skill
- **注入时机**: `UserIntent::MetaQuestion` 或系统初始化
- **载体**: `SKILL.index.md`

#### L1: 摘要层（Skill Summary）
- **内容**: 适用场景 + 执行步骤概要 + 关键决策点
- **Token 成本**: ~200 tokens / Skill
- **注入时机**: `UserIntent::SingleToolCall` 且 Skill 被匹配
- **载体**: `SKILL.summary.md`

#### L2: 完整层（Full Skill）
- **内容**: 完整步骤 + Pitfalls + 验证逻辑 + 谱系历史
- **Token 成本**: ~2000 tokens / Skill
- **注入时机**: `UserIntent::MultiStepPlanning` 且 Skill 是核心路径
- **载体**: `SKILL.md`

```rust
impl SkillRegistry {
    /// 根据意图和上下文动态加载 Skill 级别
    pub async fn load_skill_with_level(
        &self,
        skill_id: &str,
        intent: &UserIntent,
        context_budget: usize,
    ) -> Option<SkillLevel> {
        let skill = self.get(skill_id).await?;
        
        match intent {
            UserIntent::DirectAnswer | UserIntent::MetaQuestion => {
                // 不加载任何 Skill
                None
            }
            UserIntent::SingleToolCall => {
                // 加载 L1 摘要
                skill.l1_summary.clone()
            }
            UserIntent::MultiStepPlanning => {
                // 根据预算决定 L1 或 L2
                if context_budget > 3000 {
                    skill.l2_summary.clone()
                } else {
                    skill.l1_summary.clone()
                }
            }
            _ => skill.l1_summary.clone(),
        }
    }
}
```

### 4.4 谱系追踪系统（Skill Lineage）

每个 Skill 携带完整的演化历史，支持**可追溯的回滚**：

```rust
/// Skill 谱系节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    /// 版本号（语义化）
    pub version: String,
    /// 父版本 ID（空表示初始创建）
    pub parent_ids: Vec<String>,
    /// 生成来源：任务 ID、用户反馈、CAPO 优化等
    pub source: LineageSource,
    /// 变更摘要（由 LLM 生成）
    pub change_summary: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 质量评分（该版本的评分）
    pub quality_score: f32,
    /// 使用统计（执行次数、成功率）
    pub usage_stats: UsageStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineageSource {
    /// 从任务轨迹自动提炼
    AutoDistilled { task_id: String, trail_id: String },
    /// 用户手动编辑
    ManualEdit { user_id: String, reason: String },
    /// CAPO 算法优化
    CapoOptimized { generation: u32, improvement: f32 },
    /// 补丁修复
    PatchFix { issue_id: String, fix_description: String },
}
```

#### 谱系树可视化

```text
skill: flask-k8s-deploy
├── v1.0.0 (AutoDistilled from task_001)
│   └── quality: 6.5
├── v1.1.0 (PatchFix: "livenessProbe 路径修正")
│   ├── parent: v1.0.0
│   └── quality: 7.8
├── v1.2.0 (CapoOptimized, gen=3, +12% success)
│   ├── parent: v1.1.0
│   └── quality: 8.5
└── v2.0.0 (AutoDistilled from task_045, 全新工作流)
    └── quality: 7.2
```

#### 回滚机制

```rust
impl SkillRegistry {
    /// 回滚到指定版本
    pub async fn rollback(&mut self, skill_id: &str, version: &str) -> Result<Skill> {
        let lineage = self.get_lineage(skill_id).await?;
        let target = lineage.find_version(version)
            .ok_or_else(|| SkillError::VersionNotFound)?;
        
        // 创建回滚记录（不回删历史，而是新增一个回滚节点）
        let rollback_node = LineageNode {
            version: format!("{}-rollback-{}", version, Uuid::new_v4()),
            parent_ids: vec![self.current_version(skill_id).to_string()],
            source: LineageSource::Rollback { 
                target_version: version.to_string(),
                reason: "User initiated rollback".to_string(),
            },
            change_summary: format!("Rollback to {}", version),
            created_at: Utc::now(),
            quality_score: target.quality_score,
            usage_stats: UsageStats::default(),
        };
        
        self.apply_lineage_node(skill_id, rollback_node).await
    }
}
```

### 4.5 质量评分与淘汰机制

```rust
/// Skill 质量评估与生命周期管理
pub struct SkillLifecycleManager;

impl SkillLifecycleManager {
    /// 定期评估所有 Skill 的健康度
    pub async fn evaluate_all(&self, registry: &SkillRegistry) -> Vec<SkillHealth> {
        let mut healths = Vec::new();
        
        for skill in registry.all().await {
            let health = SkillHealth {
                skill_id: skill.id.clone(),
                quality_score: skill.current_quality(),
                usage_frequency: self.calculate_frequency(&skill),
                success_rate: skill.usage_stats.success_rate(),
                last_used: skill.usage_stats.last_used,
                recommendation: self.generate_recommendation(&skill),
            };
            healths.push(health);
        }
        
        healths
    }
    
    fn generate_recommendation(&self, skill: &Skill) -> LifecycleAction {
        let success_rate = skill.usage_stats.success_rate();
        let frequency = self.calculate_frequency(skill);
        
        if success_rate < 0.3 && frequency > 0 {
            // 频繁使用但成功率低 → 需要修复
            LifecycleAction::NeedsRepair {
                reason: "Low success rate".to_string(),
            }
        } else if frequency == 0.0 && skill.age_days() > 90 {
            // 90 天未使用 → 归档
            LifecycleAction::Archive {
                reason: "Stale skill".to_string(),
            }
        } else if success_rate > 0.9 && frequency > 0.5 {
            // 高频高成功率 → 推荐升级为"核心技能"
            LifecycleAction::Promote {
                reason: "High performance".to_string(),
            }
        } else {
            LifecycleAction::Keep
        }
    }
}
```

---

## 五、Agent 自主进化系统（Agent Auto-Evolution）

### 5.1 设计理念

**"从经验到本能"** 通过两层机制实现：
1. **CAPO（Context-Aware Prompt Optimization）**：负责 Prompt 和策略的自动化进化，替代 Hermes 的 GEPA 遗传算法
2. **Atropos RL 框架（DAPO + PAPO）**：负责从轨迹数据中进行强化学习训练，实现模型级能力跃迁

**核心区别**：
- **CAPO** 是**符号层进化**（Prompt、Skill 文档、SOUL.md 的文本级优化）
- **Atropos + DAPO/PAPO** 是**参数层进化**（通过 RL 微调模型权重或适配层）

### 5.2 CAPO：上下文感知提示优化算法

#### 5.2.1 为什么替代 GEPA

| 维度 | GEPA（遗传进化） | CAPO（上下文感知优化） |
|------|----------------|---------------------|
| 搜索空间 | 随机变异 + 选择，搜索空间大 | 基于上下文梯度定向优化，搜索空间小 |
| 评估成本 | 需 100-500 次评估 | 仅需 20-50 次评估 |
| 上下文利用 | 不利用任务上下文 | 充分利用 UserIntent、ToolTrail、Memory |
| 收敛速度 | 慢（类遗传算法） | 快（类反向传播） |
| 可解释性 | 低（黑箱选择） | 高（每步优化有明确上下文归因） |

#### 5.2.2 CAPO 核心机制

CAPO 将 Prompt 优化建模为**带上下文约束的编辑问题**，核心思想是：

> **不是随机生成 Prompt 变体，而是基于失败/成功轨迹，定位 Prompt 中的"问题段落"，进行定向编辑。**

```rust
/// CAPO 进化引擎
pub struct CapoEngine {
    /// 评估器：轻量 LLM（本地小模型或 GPT-4o-mini）
    evaluator: Arc<dyn LLMCallInterface>,
    /// 编辑策略集合
    edit_strategies: Vec<EditStrategy>,
    /// 上下文感知评分器
    context_scorer: ContextScorer,
    /// 优化目标配置
    config: CapoConfig,
}

pub struct CapoConfig {
    /// 最大优化轮次
    pub max_iterations: usize,
    /// 性能提升阈值（≥此值才采纳）
    pub improvement_threshold: f32,
    /// 编辑温度（控制创新性 vs 保守性）
    pub edit_temperature: f32,
    /// 回溯深度（保留多少历史版本）
    pub rollback_depth: usize,
}
```

#### 5.2.3 CAPO 执行流程

```text
┌─────────────────────────────────────────────────────────────────┐
│                    CAPO Evolution Pipeline                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Input: (SOUL.md 或 SKILL.md, ToolTrail 集合, 性能指标)          │
│                                                                  │
│  Step 1: 上下文归因分析 (Context Attribution)                    │
│    - 对每条成功/失败轨迹，LLM 分析：                              │
│      "这次成功/失败，与 SOUL.md 中哪个段落最相关？"               │
│    - 输出：段落 → 影响分数映射                                    │
│    - 示例：                                                       │
│      "'你总是先检查环境' 这段 → 成功率 +15%"                     │
│      "'忽略错误继续执行' 这段 → 失败率 +30%"                     │
│                                                                  │
│  Step 2: 问题段落定位 (Problem Localization)                     │
│    - 按影响分数排序，定位 Top-K "问题段落"                        │
│    - 同时定位 Top-K "高价值段落"（不应修改）                      │
│                                                                  │
│  Step 3: 定向编辑 (Directed Editing)                             │
│    - 对问题段落应用编辑策略：                                      │
│      a. 重写 (Rewrite): 完全替换段落                              │
│      b. 增强 (Augment): 添加条件分支或示例                        │
│      c. 删减 (Prune): 删除冗余或有害指令                          │
│      d. 重组 (Reorder): 调整段落优先级                            │
│    - 每次只编辑一个段落，控制变量                                 │
│                                                                  │
│  Step 4: 轻量评估 (Lightweight Evaluation)                       │
│    - 在保留的验证轨迹集上测试新版本                               │
│    - 计算综合得分：成功率权重 60% + Token 效率 20% + 延迟 20%     │
│                                                                  │
│  Step 5: 采纳或回退 (Adoption / Rollback)                        │
│    - 若性能提升 ≥ improvement_threshold → 采纳，生成新谱系节点    │
│    - 否则 → 回退到上一版本，尝试其他编辑策略                      │
│                                                                  │
│  Output: 优化后的 SOUL.md / SKILL.md + 变更摘要                   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

#### 5.2.4 编辑策略实现

```rust
/// 编辑策略枚举
#[derive(Debug, Clone)]
pub enum EditStrategy {
    /// 基于上下文重写段落
    Rewrite {
        context: String,
        instruction: String,
    },
    /// 添加条件分支或边界案例
    Augment {
        target_section: String,
        addition: String,
    },
    /// 删除冗余指令
    Prune {
        target_section: String,
        rationale: String,
    },
    /// 调整段落顺序
    Reorder {
        section_order: Vec<String>,
    },
}

impl CapoEngine {
    /// 生成编辑提案
    pub async fn generate_edits(
        &self,
        document: &str,
        attribution: &ContextAttribution,
    ) -> Vec<EditProposal> {
        let mut proposals = Vec::new();
        
        // 对低分段落生成 Rewrite 提案
        for (section, score) in &attribution.low_score_sections {
            if *score < 0.4 {
                proposals.push(EditProposal {
                    strategy: EditStrategy::Rewrite {
                        context: attribution.success_patterns.join("\n"),
                        instruction: format!(
                            "Rewrite '{}' based on successful patterns",
                            section
                        ),
                    },
                    expected_improvement: 0.15,
                });
            }
        }
        
        // 对边界失败案例生成 Augment 提案
        for failure in &attribution.boundary_failures {
            proposals.push(EditProposal {
                strategy: EditStrategy::Augment {
                    target_section: failure.related_section.clone(),
                    addition: format!(
                        "注意：当 {} 时，应 {}",
                        failure.condition, failure.correct_action
                    ),
                },
                expected_improvement: 0.1,
            });
        }
        
        // 对冗余段落生成 Prune 提案
        for section in &attribution.redundant_sections {
            proposals.push(EditProposal {
                strategy: EditStrategy::Prune {
                    target_section: section.clone(),
                    rationale: "Redundant with other sections".to_string(),
                },
                expected_improvement: 0.05,
            });
        }
        
        proposals
    }
}
```

#### 5.2.5 上下文感知评分器

```rust
/// 基于多维度上下文的评分器
pub struct ContextScorer {
    /// 成功率权重
    success_weight: f32,
    /// Token 效率权重
    token_efficiency_weight: f32,
    /// 用户满意度权重（显式/隐式反馈）
    satisfaction_weight: f32,
    /// 延迟权重
    latency_weight: f32,
}

impl ContextScorer {
    pub fn score(&self, result: &EvaluationResult, context: &ExecutionContext) -> f32 {
        let success_score = if result.success { 1.0 } else { 0.0 };
        
        // Token 效率 = 目标达成度 / 消耗的 tokens
        let token_efficiency = result.goal_achievement / result.tokens_consumed as f32;
        
        // 用户满意度（从反馈中提取）
        let satisfaction = context.user_feedback.as_ref()
            .map(|f| f.satisfaction_score)
            .unwrap_or(0.5);
        
        // 延迟得分（归一化到 0-1）
        let latency_score = (1.0 - (result.latency_ms as f32 / 30000.0)).clamp(0.0, 1.0);
        
        success_score * self.success_weight
            + token_efficiency * self.token_efficiency_weight
            + satisfaction * self.satisfaction_weight
            + latency_score * self.latency_weight
    }
}
```

### 5.3 Atropos 框架：异步环境协调与轨迹收集

#### 5.3.1 架构定位

Atropos 是进化系统的**异步基础设施层**，承担三项职责：
1. **轨迹收集**：从生产环境异步收集 ToolTrail、用户反馈、执行结果
2. **环境协调**：管理 CAPO 评估环境和 RL 训练环境的隔离与资源分配
3. **数据管道**：将原始轨迹清洗、标注、格式化为训练数据

```rust
/// Atropos 异步协调框架
pub struct AtroposFramework {
    /// 轨迹收集器
    trail_collector: TrailCollector,
    /// 环境管理器（CAPO 评估环境 + RL 训练环境）
    environment_manager: EnvironmentManager,
    /// 数据管道
    data_pipeline: DataPipeline,
    /// 配置
    config: AtroposConfig,
}

pub struct AtroposConfig {
    /// 轨迹缓冲区大小
    pub trail_buffer_size: usize,
    /// 评估环境并发数
    pub eval_env_concurrency: usize,
    /// 训练环境隔离级别
    pub training_isolation: IsolationLevel,
    /// 数据保留天数
    pub data_retention_days: u32,
}
```

#### 5.3.2 轨迹收集器（Trail Collector）

```rust
/// 异步轨迹收集与批处理
pub struct TrailCollector {
    /// 内存缓冲区（近期轨迹）
    buffer: Arc<RwLock<VecDeque<ToolTrail>>>,
    /// 持久化存储（SQLite）
    storage: Arc<dyn TrailStorage>,
    /// 批处理触发器
    batch_trigger: BatchTrigger,
}

impl TrailCollector {
    /// 收集单条轨迹（非阻塞）
    pub async fn collect(&self, trail: ToolTrail) {
        let mut buf = self.buffer.write().await;
        buf.push_back(trail);
        
        // 触发批处理条件检查
        if self.batch_trigger.should_flush(&buf) {
            drop(buf); // 释放写锁
            self.flush().await;
        }
    }
    
    /// 批量持久化与标注
    async fn flush(&self) {
        let trails: Vec<ToolTrail> = {
            let mut buf = self.buffer.write().await;
            buf.drain(..).collect()
        };
        
        // 并行标注（成功率、用户满意度、Token 消耗）
        let annotated = futures::future::join_all(
            trails.into_iter().map(|t| self.annotate(t))
        ).await;
        
        // 写入持久化存储
        self.storage.store_batch(&annotated).await;
    }
    
    /// 为轨迹附加元数据标注
    async fn annotate(&self, trail: ToolTrail) -> AnnotatedTrail {
        AnnotatedTrail {
            trail,
            success_rate: self.calculate_success_rate(&trail),
            user_satisfaction: self.extract_user_feedback(&trail),
            token_consumption: self.count_tokens(&trail),
            complexity_score: self.assess_complexity(&trail),
            timestamp: Utc::now(),
        }
    }
}
```

#### 5.3.3 环境管理器

```rust
/// 隔离的评估/训练环境
pub struct EnvironmentManager {
    /// CAPO 评估环境池（轻量、快速启动）
    eval_env_pool: EnvPool,
    /// RL 训练环境（完全隔离，使用历史数据回放）
    training_envs: Vec<TrainingEnvironment>,
    /// 资源限制
    resource_limits: ResourceLimits,
}

impl EnvironmentManager {
    /// 获取 CAPO 评估环境
    pub async fn acquire_eval_env(&self) -> Result<EvalEnvironment> {
        self.eval_env_pool.acquire().await
    }
    
    /// 创建 RL 训练环境
    pub async fn create_training_env(
        &self,
        trajectory_dataset: Vec<AnnotatedTrail>,
    ) -> Result<TrainingEnvironment> {
        // 在 WASM 沙箱中创建隔离环境
        let env = TrainingEnvironment::new(
            trajectory_dataset,
            self.resource_limits.clone(),
        );
        
        // 应用 WASM 隔离
        IsolationLevel::Wasm.apply(|| async {
            env.initialize().await
        }).await?;
        
        Ok(env)
    }
}
```

### 5.4 DAPO：动态采样策略优化

#### 5.4.1 问题背景：熵崩溃

在 RL 训练 Agent 策略时，常见问题是**熵崩溃（Entropy Collapse）**：
- 策略过早收敛到少数几个"安全"动作
- 探索不足，错过更优解
- 特别是在工具调用场景中，Agent 可能反复调用同一个工具，拒绝尝试新工具

传统 PPO/GRPO 使用固定熵奖励，无法自适应应对。

#### 5.4.2 DAPO 核心思想

**DAPO 通过动态调整采样分布的温度参数，维持策略的探索-利用平衡。**

```rust
/// Dynamic Sampling Policy Optimization
pub struct DapoTrainer {
    /// 基础策略模型（Actor）
    actor: Arc<dyn LanguageModel>,
    /// 价值模型（Critic）
    critic: Arc<dyn ValueModel>,
    /// 动态温度调度器
    temperature_scheduler: TemperatureScheduler,
    /// 熵监控器
    entropy_monitor: EntropyMonitor,
    /// 超参数
    config: DapoConfig,
}

pub struct DapoConfig {
    /// 初始温度
    pub initial_temperature: f32,
    /// 温度衰减率
    pub temperature_decay: f32,
    /// 最小温度（防止完全随机）
    pub min_temperature: f32,
    /// 目标熵值（维持的探索水平）
    pub target_entropy: f32,
    /// 熵崩溃检测阈值
    pub entropy_collapse_threshold: f32,
    /// 恢复采样比例（检测到崩溃时增加的探索样本）
    pub recovery_sampling_ratio: f32,
}
```

#### 5.4.3 温度调度器

```rust
/// 基于当前策略熵动态调整采样温度
pub struct TemperatureScheduler {
    config: DapoConfig,
    /// 历史熵值滑动窗口
    entropy_history: VecDeque<f32>,
    /// 当前温度
    current_temperature: f32,
}

impl TemperatureScheduler {
    /// 根据当前策略状态更新温度
    pub fn update(&mut self, current_entropy: f32) -> f32 {
        self.entropy_history.push_back(current_entropy);
        if self.entropy_history.len() > 100 {
            self.entropy_history.pop_front();
        }
        
        let avg_entropy: f32 = self.entropy_history.iter().sum::<f32>() 
            / self.entropy_history.len() as f32;
        
        if avg_entropy < self.config.entropy_collapse_threshold {
            // 检测到熵崩溃 → 提升温度（增加探索）
            let boost = (self.config.target_entropy - avg_entropy) 
                * self.config.recovery_sampling_ratio;
            self.current_temperature = (self.current_temperature + boost)
                .min(2.0); // 上限 2.0
            
            tracing::warn!(
                "DAPO entropy collapse detected: {:.3} → boosting temperature to {:.3}",
                avg_entropy, self.current_temperature
            );
        } else if avg_entropy > self.config.target_entropy * 1.2 {
            // 熵过高（过度探索）→ 降低温度
            self.current_temperature *= self.config.temperature_decay;
            self.current_temperature = self.current_temperature
                .max(self.config.min_temperature);
        }
        
        self.current_temperature
    }
    
    /// 将温度应用于动作采样
    pub fn apply_temperature(&self, logits: &[f32]) -> Vec<f32> {
        logits.iter()
            .map(|&l| l / self.current_temperature)
            .collect()
    }
}
```

#### 5.4.4 DAPO 训练循环

```rust
impl DapoTrainer {
    pub async fn train_step(&mut self, batch: TrajectoryBatch) -> TrainingMetrics {
        // 1. 计算当前策略熵
        let current_entropy = self.compute_entropy(&batch);
        
        // 2. 动态调整温度
        let temperature = self.temperature_scheduler.update(current_entropy);
        
        // 3. 带温度调整的采样
        let sampled_actions = self.sample_with_temperature(&batch, temperature);
        
        // 4. 计算优势（Advantage）
        let advantages = self.compute_advantages(&batch, &sampled_actions);
        
        // 5. 策略更新（CLIP + 动态熵奖励）
        let policy_loss = self.update_policy(&batch, &advantages, current_entropy);
        
        // 6. 价值更新
        let value_loss = self.update_critic(&batch);
        
        TrainingMetrics {
            policy_loss,
            value_loss,
            entropy: current_entropy,
            temperature,
            kl_divergence: self.compute_kl(&batch),
        }
    }
    
    /// 动态熵奖励：当熵低于目标时增加奖励权重
    fn entropy_bonus(&self, entropy: f32) -> f32 {
        let deficit = (self.config.target_entropy - entropy).max(0.0);
        // 赤字越大，熵奖励越高
        self.config.entropy_coefficient * (1.0 + deficit * 2.0)
    }
}
```

### 5.5 PAPO：过程感知策略优化

#### 5.5.1 问题背景：稀疏奖励

在 Agent 工具调用场景中，传统 RL 只在任务结束时给出一个**最终结果奖励**（成功/失败），这导致：
- 信用分配问题：20 步工具调用中，哪一步出了问题？
- 训练效率低：只有完整轨迹才有奖励信号
- 无法区分"几乎成功"和"完全失败"

#### 5.5.2 PAPO 核心思想

**PAPO 为每个中间步骤提供过程级奖励（Process Reward），通过工具调用验证器实现细粒度信用分配。**

```rust
/// Process-Aware Policy Optimization
pub struct PapoTrainer {
    /// 过程奖励模型（PRM）
    process_reward_model: Arc<dyn ProcessRewardModel>,
    /// 工具调用验证器集合
    validators: Vec<Box<dyn ToolCallValidator>>,
    /// 步骤级信用分配器
    credit_assigner: CreditAssigner,
    /// 配置
    config: PapoConfig,
}

pub struct PapoConfig {
    /// 过程奖励权重（vs 最终奖励）
    pub process_reward_weight: f32,
    /// 最终奖励权重
    pub final_reward_weight: f32,
    /// 验证超时（毫秒）
    pub validation_timeout_ms: u64,
    /// 信用分配策略
    pub credit_assignment: CreditAssignmentStrategy,
}
```

#### 5.5.3 工具调用验证器

PAPO 的核心是**领域特定的验证器**，为每个工具调用提供即时反馈：

```rust
/// 工具调用验证器 trait
#[async_trait]
pub trait ToolCallValidator: Send + Sync {
    /// 验证工具名称
    fn tool_name(&self) -> &str;
    
    /// 验证工具调用结果，返回过程奖励 [-1.0, +1.0]
    async fn validate(
        &self,
        params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward;
}

/// 代码执行验证器
pub struct CodeExecutionValidator;

#[async_trait]
impl ToolCallValidator for CodeExecutionValidator {
    fn tool_name(&self) -> &str { "execute_code" }
    
    async fn validate(
        &self,
        _params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        // 检查编译/执行结果
        if let Some(exit_code) = result.get("exit_code").and_then(|v| v.as_i64()) {
            if exit_code == 0 {
                // 检查是否有测试通过标记
                let output = result.get("output").and_then(|v| v.as_str()).unwrap_or("");
                if output.contains("test passed") || output.contains("OK") {
                    ProcessReward::positive(1.0, "Code executed and tests passed")
                } else {
                    ProcessReward::positive(0.5, "Code executed but no tests")
                }
            } else {
                let stderr = result.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                if stderr.contains("syntax error") {
                    ProcessReward::negative(1.0, "Syntax error in code")
                } else {
                    ProcessReward::negative(0.5, "Runtime error")
                }
            }
        } else {
            ProcessReward::neutral("Unknown execution state")
        }
    }
}

/// 链上交易验证器
pub struct ChainTransactionValidator {
    /// 链上 RPC 客户端
    rpc_client: Arc<dyn ChainRpcClient>,
}

#[async_trait]
impl ToolCallValidator for ChainTransactionValidator {
    fn tool_name(&self) -> &str { "send_transaction" }
    
    async fn validate(
        &self,
        params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        let tx_hash = result.get("tx_hash").and_then(|v| v.as_str());
        
        if let Some(hash) = tx_hash {
            // 异步查询链上回执
            match tokio::time::timeout(
                Duration::from_secs(30),
                self.rpc_client.get_receipt(hash)
            ).await {
                Ok(Ok(receipt)) => {
                    if receipt.status == 1 {
                        // 检查 gas 效率
                        let gas_used = receipt.gas_used;
                        let gas_limit = params.get("gas_limit").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
                        let efficiency = 1.0 - (gas_used as f32 / gas_limit as f32);
                        
                        ProcessReward::positive(
                            0.8 + efficiency * 0.2,
                            &format!("Transaction confirmed, gas efficiency {:.1}%", efficiency * 100.0)
                        )
                    } else {
                        ProcessReward::negative(1.0, "Transaction reverted on-chain")
                    }
                }
                _ => ProcessReward::negative(0.3, "Failed to get transaction receipt"),
            }
        } else {
            ProcessReward::negative(0.8, "Transaction submission failed")
        }
    }
}

/// HTTP API 调用验证器
pub struct HttpApiValidator;

#[async_trait]
impl ToolCallValidator for HttpApiValidator {
    fn tool_name(&self) -> &str { "http_request" }
    
    async fn validate(
        &self,
        _params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        let status = result.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
        
        match status {
            200..=299 => {
                // 检查响应体格式
                let body = result.get("body").and_then(|v| v.as_str()).unwrap_or("");
                if serde_json::from_str::<serde_json::Value>(body).is_ok() {
                    ProcessReward::positive(1.0, "Valid JSON response")
                } else {
                    ProcessReward::positive(0.7, "Successful HTTP but non-JSON response")
                }
            }
            400..=499 => ProcessReward::negative(0.6, &format!("Client error: {}", status)),
            500..=599 => ProcessReward::negative(0.4, &format!("Server error: {}", status)),
            _ => ProcessReward::negative(0.5, "Unexpected HTTP status"),
        }
    }
}
```

#### 5.5.4 步骤级信用分配

```rust
/// 信用分配策略
pub enum CreditAssignmentStrategy {
    /// 均匀分配：每步获得相同的过程奖励
    Uniform,
    /// 衰减分配：后期步骤获得更多信用（假设前期步骤是铺垫）
    Decay { decay_factor: f32 },
    /// 差异分配：与平均行为的偏差决定信用
    Advantage,
    /// 基于验证器的显式归因
    ValidatorAttribution,
}

pub struct CreditAssigner {
    strategy: CreditAssignmentStrategy,
}

impl CreditAssigner {
    /// 为轨迹中的每一步分配过程奖励
    pub fn assign(
        &self,
        trajectory: &AnnotatedTrail,
        step_rewards: Vec<ProcessReward>,
    ) -> Vec<f32> {
        let n = step_rewards.len();
        if n == 0 { return Vec::new(); }
        
        match self.strategy {
            CreditAssignmentStrategy::Uniform => {
                // 每步获得相同的过程奖励
                step_rewards.iter()
                    .map(|r| r.score * self.config.process_reward_weight / n as f32)
                    .collect()
            }
            CreditAssignmentStrategy::Decay { decay_factor } => {
                // 后期步骤权重更高
                let total_weight: f32 = (0..n)
                    .map(|i| decay_factor.powi(i as i32))
                    .sum();
                step_rewards.iter().enumerate()
                    .map(|(i, r)| {
                        let weight = decay_factor.powi(i as i32) / total_weight;
                        r.score * self.config.process_reward_weight * weight
                    })
                    .collect()
            }
            CreditAssignmentStrategy::Advantage => {
                // 与基准策略的偏差
                let baseline = step_rewards.iter().map(|r| r.score).sum::<f32>() / n as f32;
                step_rewards.iter()
                    .map(|r| {
                        let advantage = r.score - baseline;
                        advantage * self.config.process_reward_weight
                    })
                    .collect()
            }
            CreditAssignmentStrategy::ValidatorAttribution => {
                // 直接使用验证器的奖励分数
                step_rewards.iter()
                    .map(|r| r.score * self.config.process_reward_weight)
                    .collect()
            }
        }
    }
}
```

#### 5.5.5 PAPO 综合奖励函数

```rust
impl PapoTrainer {
    /// 计算综合奖励 = 过程奖励 + 最终奖励
    pub fn compute_total_reward(
        &self,
        trajectory: &AnnotatedTrail,
        step_rewards: Vec<ProcessReward>,
    ) -> f32 {
        let credits = self.credit_assigner.assign(trajectory, step_rewards);
        let process_total: f32 = credits.iter().sum();
        
        // 最终奖励（任务成功/失败）
        let final_reward = if trajectory.trail.status == TrailStatus::Success {
            self.config.final_reward_weight
        } else {
            -self.config.final_reward_weight
        };
        
        process_total + final_reward
    }
}
```

### 5.6 三层进化的协同机制

#### 5.6.1 触发频率与粒度

| 进化层 | 触发频率 | 数据粒度 | 计算成本 | 用户感知 |
|--------|----------|----------|----------|----------|
| **记忆进化** | 每 10 回合 / 每次成功任务 | 单条事实 | 低（本地 LLM） | 无感知 |
| **技能进化** | 每 5 次复杂任务 / 每日批处理 | 完整轨迹 | 中（轻量 LLM） | 无感知 |
| **CAPO** | 每周 / 每 100 条轨迹 | 文档级 | 中（20-50 次评估） | 无感知 |
| **Atropos RL** | 每月 / 用户主动触发 | 批量轨迹 | 高（GPU 训练） | 需显式确认 |

#### 5.6.2 数据流协同

```text
用户任务执行
    │
    ▼
┌──────────────┐
│  ToolTrail   │──────▶ 记忆进化（实时）
│   生成       │        └─▶ MEMORY.md / USER.md 更新
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Nudge Engine │──────▶ 技能进化（批处理）
│  触发评估    │        └─▶ SKILL.md 创建 / Patch
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Atropos     │──────▶ 轨迹数据集构建
│  轨迹归档    │        └─▶ SQLite + 标注
└──────┬───────┘
       │
       ▼
┌──────────────┐
│   CAPO       │──────▶ SOUL.md / SKILL.md 优化
│  文档进化    │        └─▶ 符号层进化
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Atropos RL  │──────▶ 模型微调（DAPO + PAPO）
│  训练管道    │        └─▶ 参数层进化（需用户确认）
└──────────────┘
```

---

## 六、安全与约束

### 6.1 进化安全边界

所有进化操作必须遵守以下安全约束：

```rust
/// 进化安全沙箱
pub struct EvolutionSandbox;

impl EvolutionSandbox {
    /// 执行前安全检查
    pub fn preflight_check(&self, proposal: &EvolutionProposal) -> Result<(), SafetyViolation> {
        // 1. 内容安全扫描
        if self.contains_credential(&proposal.delta) {
            return Err(SafetyViolation::CredentialLeak);
        }
        
        // 2. 指令注入检测
        if self.contains_instruction_injection(&proposal.delta) {
            return Err(SafetyViolation::InstructionInjection);
        }
        
        // 3. 容量超限检查
        if proposal.result_size > proposal.max_allowed_size {
            return Err(SafetyViolation::CapacityExceeded);
        }
        
        // 4. 回滚可用性检查
        if !self.can_rollback(&proposal.target) {
            return Err(SafetyViolation::NoRollbackPath);
        }
        
        Ok(())
    }
    
    /// 执行后验证
    pub fn post_execution_verify(&self, result: &EvolutionResult) -> Result<(), SafetyViolation> {
        // 验证系统提示词仍可解析
        // 验证无循环依赖
        // 验证技能可加载
        Ok(())
    }
}
```

### 6.2 用户控制层级

| 控制项 | 默认行为 | 用户可配置 |
|--------|----------|-----------|
| 记忆自动写入 | 开启 | 可关闭 / 可审查 |
| 技能自动创建 | 开启 | 可关闭 / 可审查 |
| CAPO 自动优化 | 开启 | 可关闭/可审查 |
| Atropos RL 训练 | 关闭 | 需显式 `beebotos train` |
| 数据上传云端 | 禁止 | 本地存储或云端 |

---

## 七、实现路线图

### Phase 1: 记忆自主进化（4 周）

| 周次 | 任务 | 涉及模块 |
|------|------|----------|
| W1 | Nudge Engine 框架 + 回合计数器 | `evolution/memory_nudge.rs` |
| W2 | L1/L2 容量管理与主动压缩 | `memory/markdown_storage.rs` |
| W3 | 记忆质量评估器 + 去重算法 | `evolution/memory_quality.rs` |
| W4 | PromptBuilder 记忆注入集成 + 测试 | `prompt/builder.rs` |

### Phase 2: 技能自主进化（6 周）

| 周次 | 任务 | 涉及模块 |
|------|------|----------|
| W1-W2 | Skill Distiller 框架 + 轨迹清洗 | `evolution/skill_distiller.rs` |
| W3 | 自动提炼 Pipeline + 质量评分 | `evolution/skill_distiller.rs` |
| W4 | Skill Lineage 谱系追踪系统 | `evolution/skill_lineage.rs` |
| W5 | 渐进披露 L0/L1/L2 动态加载 | `skills/registry.rs`, `skills/discovery.rs` |
| W6 | 回滚机制 + 生命周期管理 | `skills/registry.rs` |

### Phase 3: CAPO 提示优化（4 周）

| 周次 | 任务 | 涉及模块 |
|------|------|----------|
| W1 | 上下文归因分析器 | `evolution/capo/attribution.rs` |
| W2 | 定向编辑策略（Rewrite/Augment/Prune/Reorder） | `evolution/capo/editor.rs` |
| W3 | 轻量评估环境 + 评分器 | `evolution/capo/evaluator.rs` |
| W4 | SOUL.md / SKILL.md 进化闭环集成 | `evolution/capo/engine.rs` |

### Phase 4: Atropos + DAPO + PAPO（8 周）

| 周次 | 任务 | 涉及模块 |
|------|------|----------|
| W1-W2 | Atropos 框架：轨迹收集器 + 数据管道 | `evolution/atropos/collector.rs` |
| W3-W4 | Atropos 环境管理器（WASM 隔离评估环境） | `evolution/atropos/environment.rs` |
| W5-W6 | DAPO 训练器：温度调度 + 熵监控 | `evolution/dapo/trainer.rs` |
| W7 | PAPO 验证器体系：代码/链上/HTTP | `evolution/papo/validators.rs` |
| W8 | 过程奖励模型 + 信用分配 + 训练闭环 | `evolution/papo/trainer.rs` |

### Phase 5: 整合与压测（4 周）

- 三层进化协同测试
- 长期运行稳定性验证（30 天持续进化）
- 安全边界渗透测试
- 性能基准：进化开销 < 5% 任务延迟

---

## 八、附录

### A.1 术语表

| 术语 | 定义 |
|------|------|
| **CAPO** | Context-Aware Prompt Optimization，上下文感知提示优化算法 |
| **DAPO** | Dynamic Sampling Policy Optimization，动态采样策略优化，解决熵崩溃 |
| **PAPO** | Process-Aware Policy Optimization，过程感知策略优化，提供步骤级奖励 |
| **Atropos** | 异步环境协调与轨迹收集框架 |
| **Nudge Engine** | 主动提醒引擎，定期触发记忆/技能复盘 |
| **Skill Lineage** | 技能谱系追踪，记录 Skill 的版本演化历史 |
| **Progressive Disclosure** | 渐进披露，根据意图动态加载 Skill 的不同深度 |
| **ToolTrail** | 工具调用轨迹，记录 Planning 执行过程中的完整工具调用链 |

### A.2 与现有代码的扩展点

| 现有模块 | 扩展方式 |
|----------|----------|
| `memory/search.rs` | 扩展 `MemorySearch` trait，添加 `search_for_distillation` 方法 |
| `memory/markdown_storage.rs` | 扩展 `MarkdownStorage`，支持容量监控和主动压缩 |
| `skills/registry.rs` | 扩展 `SkillRegistry`，添加 `lineage` 和 `rollback` 支持 |
| `skills/feedback.rs` | 复用 `SkillFeedback`，扩展为 `SkillEvolutionFeedback` |
| `planning/tool_trail.rs` | 扩展 `ToolTrail`，添加 `to_training_data` 转换方法 |
| `context/assembler.rs` | 扩展 `Summarizer`，添加 `memory_aware_summarize` |
| `agent_impl.rs` | 添加进化调度器（`EvolutionScheduler`）字段和方法 |

### A.3 设计决策记录

**ADR-001: 为什么用 CAPO 替代 GEPA？**
- GEPA 的遗传算法需要大量评估（100-500 次），成本高
- CAPO 利用上下文归因直接定位问题段落，评估次数减少 80%
- CAPO 的可解释性更强（每步编辑有明确的上下文归因）

**ADR-002: 为什么排除 PAD/OCEAN？**
- PAD 情感标签需要额外的情感识别模型，增加系统复杂度
- OCEAN 人格权重对工具型 Agent 的决策优化增益有限
- 行为事实记录（L1/L2）已足够实现个性化，无需心理推断

**ADR-003: 为什么单 Agent 进化优先？**
- 群智能体进化需要解决知识一致性、冲突消解等问题，复杂度指数级增长
- A2A 跨智能体进化引入网络延迟和安全边界扩散问题
- 单 Agent 内部闭环已能提供显著的进化效果，是性价比最优的路径

**ADR-004: 为什么 DAPO 替代 GRPO？**
- GRPO 的组相对策略优化在工具调用场景中易出现熵崩溃
- DAPO 的动态温度调度能自适应维持探索水平
- DAPO 与 PAPO 的过程奖励天然互补（探索 + 信用分配）

---

*本文档为 BeeBotOS 自主进化系统的技术设计规格书，所有模块接口和数据结构可直接映射到 `crates/agents/src/evolution/` 目录的实现代码。*
