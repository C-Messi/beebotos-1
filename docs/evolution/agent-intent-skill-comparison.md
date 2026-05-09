# BeeBotOS 意图分析与 Skill 匹配机制：代码实现 vs POAGENT 设计回顾

> 本文基于 `agent-peng-prompt.md` 中对 POAGENT 意图识别与 Skill 匹配机制的设计描述，逐层对比 BeeBotOS 代码仓库中的实际实现，输出差异、演进和待完善项。

---

## 1. POAGENT 设计回顾（三条路径）

根据 `agent-peng-prompt.md`，POAGENT 的意图识别与 Skill 匹配被设计为**三条独立路径**，没有统一分类器：

### 路径1：普通对话 —— LLM 自主匹配 Description
- Registry 把可用 Skill 压成 `<available_skills>` prompt，仅暴露 `name` / `description` / `location`
- 最多 50 个，按 `usage_count` 排序
- Agent system prompt 明确 instruct LLM：
  - 先扫描 `<description>`
  - 一个明显适用 → 调用 `read_skill`
  - 多个适用 → 选最具体的
  - 没有明显适用 → 不读 Skill
- `read_skill` 被调用后才读取完整 `SKILL.md`（Progressive Disclosure）
- **核心特征**：主路径不是 embedding、不是规则引擎，而是 LLM 读 description 后自主判断

### 路径2：工具检索 —— 关键词搜索辅助
- `list_skills` 支持 `query` / `category`，其中 `query` 走 `registry.search`
- `SkillRegistry::search` 逻辑：
  - `name` / `description` 直接 substring 命中 → 高分
  - 否则把 `query`、`name`、`description`、`capabilities` 拆词
  - 用关键词 overlap 排序
- **定位**：不是主对话自动选择的核心，只是给模型或 API 查 Skill 用

### 路径3：Planning / SkillExecution —— 规则 + 显式参数
- **SkillExecution 类型任务**：不是识别意图，参数里必须有 `skill`，直接 `registry.get`
- **Plan step**：硬编码 domain keyword 映射（`code`、`travel`、`finance`、`security` 等映射到特定 skill id），再 fallback 到 `registry.search` / `tag`

### 当时对 POAGENT 的判断
- 已抓住主流方向：`description` 驱动发现 + 按需读取 `SKILL.md` + 避免上下文爆炸
- 但更像"轻量 skill prompt registry"，还没到成熟 agent skill runtime
- 主要不足：过度依赖 description 文案质量、没有 embedding/reranker/负例训练、没有多 skill 编排、没有 skill 级权限与资源加载协议

---

## 2. BeeBotOS 实际实现详解

BeeBotOS 的代码实现与 POAGENT 设计既有继承关系，也发生了显著演进。下面按代码模块分层解析。

### 2.1 意图分析层：`crates/agents/src/intent/mod.rs`

BeeBotOS 实现了一个**独立的 `IntentEngine`**，这是 POAGENT 设计中没有明确描述的。

| 特性 | 实现细节 |
|------|---------|
| **分类体系** | 六种意图：`DirectAnswer`、`SingleToolCall`、`MultiStepPlanning`、`WorkflowTrigger`、`MetaQuestion`、`Correction` |
| **分类方式** | 纯启发式规则（regex + 关键词 + 否定检测），**不调用 LLM** |
| **触发词检测** | 否定词（"不要"/"别"/"直接"）、元问题（"你会什么"/"what can you do"）、工作流（"/" 开头）、多步骤连接词（"先"/"再"/"然后"/"first"/"then"） |
| **Entity 提取** | `symbol`（BTC/USD、AAPL 等）、`side`（buy/sell）、`qty`（数量+单位） |
| **约束提取** | "不要先查询" → `skip_query_first`，"直接下单" → `direct_order`，"尽快" → `urgent` |
| **Toolset 检测** | 8 组预定义 keywords（account、trading、watchlists、stock-data、crypto-data、options-data、news、weather） |
| **Dual-track** | 设计了 `classify_dual_track`：heuristic 先运行，confidence < threshold（默认 0.7）时返回 LLM classification prompt；**但 LLM fallback 在 `agent_impl.rs` 中实际未调用**（注释 `TODO: If an LLM provider is available...`） |

**与 POAGENT 的对比**：
- POAGENT 没有独立的意图分类层，主路径直接让 LLM 读 description 判断
- BeeBotOS 增加了一个**前置轻量分类器**，用于路由决策（是否注入工具、是否启用 planning、是否跳过 LLM 直接回答）
- 这个设计比 POAGENT 更工程化，但当前实现完全依赖规则，LLM fallback 未实际落地

### 2.2 Skill 发现与注册：`skills/discovery.rs` + `skills/builtin_loader.rs` + `skills/registry.rs`

#### SkillDiscovery
- 扫描 `skills/` 目录，支持两种格式：
  - **目录型**：`skills/{id}/SKILL.md` + 可选 `SKILL.index.md`（L1）+ `SKILL.summary.md`（L2）
  - ** legacy 平铺型**：`.md` 文件
- 解析 YAML front matter（`name`、`description`、`category`、`tags`、`version`）
- 识别三种 `SkillKind`：`Knowledge`（纯文档）、`Code`（+可执行脚本）、`Wasm`（+`skill.wasm`）

#### BuiltinLoader
- 从 `SKILL.md` 的 markdown sections 中提取：
  - `description`
  - `prompt_template`（## Prompt Template）
  - `examples`（## Examples）
  - `capabilities`（## Capabilities 下的 bullet list）
- `build_tags` 基于内容关键词自动打标签（coding、planning、writing、travel 等）
- 注册到 `SkillRegistry`，category 默认为目录名

#### SkillRegistry
- Thread-safe（`RwLock<HashMap>`）
- 核心接口：`register`、`get`、`by_category`、`by_tag`、`search`、`record_usage`
- **Progressive Disclosure 支持**：
  - `SkillDisclosureLevel`：L0（~10 tokens，name only）、L1（~30 tokens，name + one-liner）、L2（~200 tokens，summary）、L3（~2000 tokens，full doc）
  - `get_skill_description(skill_id, level)` 按层级返回内容
- **`search` 算法**（与 POAGENT 设计一致）：
  1. `name` / `description` substring 命中 → score = 100
  2. 否则把 query、name、description、capabilities 拆成 ≥3 字符的词
  3. 计算 `HashSet` intersection 计数作为 score
  4. 按 score 降序排列

**与 POAGENT 的对比**：
- `search` 算法与 POAGENT 描述几乎一致
- Progressive Disclosure 的 **L0/L1/L2/L3 层级定义已经落地**，但实际 prompt 注入时并未严格按层级差异化使用（见 2.4）
- 增加了 `SkillLineage`（版本回溯、rollback）、自动 tag 生成、usage_count 统计

### 2.3 Gateway 层 Skill 匹配：`apps/gateway/src/services/message_processor.rs`

这是 BeeBotOS 相比 POAGENT 设计**变化最大的地方**。

#### `try_match_skill`：已完全禁用
```rust
async fn try_match_skill(&self, _content: &str) -> Option<(String, String, String, String)> {
    None
}
```
注释明确说明：
> "LLM now has full autonomy to choose the appropriate skill from the catalog based on user intent. This avoids keyword-misunderstanding issues where gateway matches a skill that does not fit the user request."

也就是说，BeeBotOS 在 Gateway 层**主动放弃了关键词匹配**，将所有 Skill 选择决策完全下放给 Agent 层的 LLM。

#### Session-level Skill Inheritance
虽然 `try_match_skill` 返回 None，但 Gateway 实现了**会话级 Skill 继承**：
- 如果当前消息未匹配 Skill，但 session metadata 中有 `active_skill`，则检查 domain relevance 后继承
- **Domain Relevance Check**（硬编码规则）：
  - `code_researcher` 不应继承给 crypto/finance 查询（排除 btc、bitcoin、eth、股票、行情等关键词）
  - `mcp:alpaca/*` 不应继承给 code/development 查询（排除 code、编程、开发、debug 等关键词）
  - 其他 Skill 默认继承
- 用户发送退出关键词（"结束"、"退出"、"bye" 等）时清除 `active_skill`
- 新 Skill 匹配成功时更新 session 的 `active_skill`

#### Skill Planning 触发判断
Gateway 还会根据已匹配 Skill 的类型和 query 复杂度决定是否注入 `plan=true`：
- **Analytical Skill**（developer、analyst、advisor 等）：强制启用 planning
- **Generative Skill**（travel、planner、writer 等）：
  - `travel_planner` 跳过 planning（单轮生成足够）
  - 其他 generative skill 仅在 query 高复杂度时启用 planning

**与 POAGENT 的对比**：
- POAGENT 设计在 Gateway/Agent 层保留了 LLM 读 description 自主匹配的能力，BeeBotOS 实际上**更进一步**：Gateway 层完全不干预 Skill 选择，全部由 LLM 自主决策
- BeeBotOS 新增的 Session-level inheritance 是为了解决多轮对话中 Skill 上下文丢失的问题，这是 POAGENT 设计未覆盖的场景
- POAGENT 提到的 planning 中"硬编码 domain keyword 映射到 skill id"在 BeeBotOS 代码中**不存在**（HybridPlanner 只有基于关键词选择 planning strategy 的逻辑，不是 skill 映射）

### 2.4 Agent 层 Skill 自主匹配：`agent_impl.rs` 中的 `skill_catalog` + `inject_skill_catalog`

这是 BeeBotOS 实际承担 Skill 匹配职责的核心路径。

#### Skill Catalog 构建
在 `agent_runtime_impl.rs` 中，启动 agent 时：
1. `SkillDiscovery::scan()` 扫描 `skills/` 目录
2. 合并 `SkillRegistry::list_all()` 中的已注册 Skill（含 MCP skills）
3. 格式化为：`- {id} ({category}): {description}`
4. 缓存到 `skill_catalog: RwLock<Option<String>>`

#### `inject_skill_catalog`
```rust
fn inject_skill_catalog(&self, messages: Vec<Message>) -> Vec<Message> {
    // 注入系统消息，内容包含：
    // 1. 可用 skills 列表（catalog）
    // 2. INSTRUCTION:
    //    - 当 Skill 匹配时，只回复 SKILL:<id>|{"key":"value"}
    //    - 无匹配时直接回答
    //    - NEVER analyze, explain, or think out loud
    //    - 信息缺失时只问一句
    // 3. EXAMPLES（weather、place_crypto_order）
}
```

#### Persona 中的 Skill 指令
在 `handle_llm_task_internal` 中，构建 persona 时：
- 如果 Gateway 已提供 `skill_hint`（通过 session inheritance 或之前轮次）：
  - 追加指令："Gateway 建议使用的 skill 是 '{id}'. 如果该 skill 能直接满足用户请求，请只回复 SKILL:id|{...}..."
- 如果无 `skill_hint` 但 `skill_catalog` 存在：
  - 追加指令："当用户请求与某个 skill 匹配时，请只回复 SKILL:<skill_id>|{...}"

#### Skill 触发解析与执行
当 LLM 回复包含 `SKILL:id|params` 格式时：
- Agent 解析出 `skill_id` 和参数
- 调用 `execute_skill_by_id` → `registry.get(skill_id)` → `execute_registered_skill`
- `execute_registered_skill` 的执行优先级：
  1. **MCP Skill Bridge**：如果 id 格式为 `mcp:{server_name}/{tool_name}`，通过 MCP client 调用外部 tool
  2. **LLM Fallback**：用 `prompt_template` 作为 system prompt，调用 LLM 生成结果（WASM 已注释为 removed，但代码中 sandbox 路径仍保留）

**与 POAGENT 的对比**：
- 与 POAGENT "路径1：LLM 读 description 自主判断"的核心思想完全一致
- BeeBotOS 的 catalog 格式更**指令化**（强制要求 `SKILL:id|params` 格式），而 POAGENT 描述的是调用 `read_skill` 工具
- BeeBotOS 没有实现 `read_skill` 这个显式 tool call，而是通过输出格式约定让 LLM "触发" skill 执行
- **Progressive Disclosure 的实际使用与设计理念有落差**：`skill_catalog` 注入的是统一格式的简单列表（id + category + description），没有严格按 L1/L2/L3 分层暴露。`PromptBuilder::build_skills_section` 虽有分层逻辑（DirectAnswer 不显示、MetaQuestion 只显示 L1、MultiStepPlanning 显示 L2），但当前主流程使用的是 `inject_skill_catalog` 的统一列表注入

### 2.5 显式 Skill 执行：`handle_skill_task`

对应 POAGENT 的"路径3：显式 skill 任务直接指定 skill"。

```rust
async fn handle_skill_task(&self, task: &Task) -> Result<...> {
    let skill_name = task.parameters.get("skill")
        .ok_or_else(|| AgentError::InvalidConfig("Missing 'skill' parameter".into()))?;
    let registered_skill = registry.get(skill_name).await
        .ok_or_else(|| AgentError::SkillNotFound(skill_name.clone()))?;
    let result = self.execute_registered_skill(&registered_skill, &task.input, 
                                               Some(task.parameters.clone())).await?;
    // ...
}
```

**与 POAGENT 的对比**：实现完全一致。通过 `task.parameters["skill"]` 显式指定，直接 `registry.get`。

### 2.6 Planning 中的意图与 Skill 整合：`planning/engine.rs`

#### `IntentAnalyzer`
PlanningEngine 中包含一个 `IntentAnalyzer`，在 `create_plan_with_memory` 中调用：
1. 调用 `IntentEngine::classify_heuristic(query)` 获取意图分类
2. 提取 entities 和 constraints
3. 从 memory 搜索 historical solutions
4. 构造 `IntentResult`（含 suggested_approach）
5. 如果历史方案 keyword overlap > 0.85，直接复用历史 plan

#### PlanContext
- `available_tools`：从 Agent 获取的工具列表
- `constraints`：从 task parameters 或 intent analysis 传递
- `history`：memory 中检索的历史方案

#### Planner 实现
- **ReActPlanner**：推理 → 信息收集 → 决策 → 执行 → 反思 → 验证
- **ChainOfThoughtPlanner**：问题分解 → 多角度考虑 → 选择方案 → 逐步推理 → 验证
- **GoalBasedPlanner**：明确目标 → 定义成功标准 → 识别障碍 → 规划路径 → 执行 → 验证
- **HybridPlanner**：使用 `Decomposer` 分解任务 + 在 Action step 前插入 Reasoning step + 去重 + 硬上限 8 步 + 标记并行安全

**与 POAGENT 的对比**：
- BeeBotOS 的 Planning 体系比 POAGENT 描述的要完整得多，包含四种策略、记忆注入、历史方案复用、ToolTrail 追踪
- POAGENT 提到的"plan step 中硬编码 domain keyword 映射到 skill id"在 BeeBotOS 中**不存在**。Plan step 的 skill 关联通过 `Action::Delegate { skill_hint: Option<String> }` 实现，但无硬编码映射表
- BeeBotOS 的 planning 中的意图分析是 `IntentAnalyzer` 的结果注入，而非 POAGENT 描述的规则路由

### 2.7 PromptBuilder 中的 Progressive Disclosure：`prompt/builder.rs`

`PromptBuilder` 实现了基于意图的差异化 Skill 展示：

| UserIntent | Skill 展示级别 |
|-----------|---------------|
| `DirectAnswer` | 不显示 Skills |
| `MetaQuestion` | 只显示 L1（`[技能目录]`） |
| `SingleToolCall` | 只显示 L1（`[可用技能]`） |
| `MultiStepPlanning` | 显示 L2 + L1（`[技能详情]`） |

同时，`filter_memories_by_intent` 也会基于意图关键词过滤相关记忆：
- `SingleToolCall` → 优先 "tool" / "skill" 相关记忆
- `MultiStepPlanning` → 优先 "plan" / "步骤" / "流程"
- `MetaQuestion` → 优先 "skill" / "capability"

**与 POAGENT 的对比**：
- 设计理念完全一致：不同意图对应不同信息暴露粒度
- 但实际主流程（`inject_skill_catalog`）未完全接入 `PromptBuilder` 的分层逻辑，而是统一注入完整 catalog

---

## 3. 逐项对比表

| 维度 | POAGENT 设计 | BeeBotOS 实际实现 | 差异说明 |
|------|-------------|------------------|---------|
| **意图分类层** | 无独立分类器，直接让 LLM 读 description 判断 | 独立 `IntentEngine`，六种意图，纯启发式规则 | BeeBotOS 增加了前置分类层用于路由，LLM fallback 未落地 |
| **主对话 Skill 匹配** | LLM 读 `<available_skills>` description 自主调用 `read_skill` | LLM 读 `skill_catalog` 后自主输出 `SKILL:id|params` | 核心思想一致，BeeBotOS 用输出格式约定替代 tool call |
| **Gateway 关键词匹配** | 保留（未明确描述禁用） | **`try_match_skill` 已完全禁用，返回 `None`** | BeeBotOS 更进一步，Gateway 完全不干预 Skill 选择 |
| **Session Skill 继承** | 未提及 | 实现 domain relevance check 的 session inheritance | BeeBotOS 新增，解决多轮对话上下文问题 |
| **Registry Search** | name/description substring + 关键词 overlap 排序 | 完全一致：substring → 100 分，否则拆词 overlap | 实现一致 |
| **Progressive Disclosure** | L0/L1/L2/L3 概念 + `read_skill` 按需加载 | L0/L1/L2/L3 已定义，`PromptBuilder` 有分层逻辑 | 代码结构已落地，但主流程 `inject_skill_catalog` 未严格分层 |
| **显式 Skill 执行** | 参数里必须有 `skill`，直接 `registry.get` | `task.parameters["skill"]` → `registry.get` → 执行 | 实现一致 |
| **Planning Skill 路由** | 硬编码 domain keyword → skill id 映射 | **不存在硬编码映射**。Plan 通过 `skill_hint` 可选传递 | POAGENT 描述的特性未在代码中体现；BeeBotOS 使用更通用的 planning 策略 |
| **Skill 执行方式** | LLM fallback（WASM removed） | MCP Bridge → LLM fallback → WASM sandbox（保留） | BeeBotOS 优先 MCP Bridge，LLM fallback 用 `prompt_template` |
| **Skill 反馈收集** | 未提及 | `SkillImprovementEngine` + usage_count + execution_time | BeeBotOS 新增进化基础设施 |
| **Catalog 构建** | 启动时按 usage_count 排序，最多 50 个 | `SkillDiscovery.scan()` + registry 合并，无数量上限，无 usage_count 排序 | BeeBotOS 未对 catalog 做数量裁剪和排序 |
| **多 Skill 组合** | Prompt 明确要求"不要预先读取多个技能，只选一个" | 单轮只匹配一个 Skill（`skill_hint` 单一），无显式组合机制 | 两者都限制为单 Skill，组合能力均较弱 |

---

## 4. 关键差异深度分析

### 4.1 Gateway 层 Skill 匹配的"主动放弃"

BeeBotOS 最大的架构决策是将 Gateway 层的 `try_match_skill` 设为永远返回 `None`。这意味着：
- **所有 Skill 选择完全由 LLM 在 Agent 层自主决定**
- Gateway 不再做关键词匹配，避免了"gateway matches a skill that does not fit the user request"的问题
- 代价是：LLM 的上下文必须包含完整的 `skill_catalog`，增加了每轮 prompt 的固定开销

这与 POAGENT 设计相比是一个**激进的去中心化**决策。POAGENT 保留了"让 LLM 自主判断"的主路径，但并未明确禁止 Gateway 层做预处理匹配。

### 4.2 Intent Engine 的前置价值

BeeBotOS 的 `IntentEngine` 是一个 POAGENT 设计中没有的模块。它的价值在于：
- **Token 节省**：`DirectAnswer` 和 `MetaQuestion` 可以直接跳过 tool/skill 注入，节省 5k-10k tokens
- **路由优化**：`Correction` 可以走专门的处理路径，`MultiStepPlanning` 可以启用 planning engine
- **信息预提取**：entities（symbol、side、qty）和 constraints 可以在进入 LLM 主循环前提取，供后续使用

但当前实现是纯规则的，dual-track 的 LLM fallback 是一个 TODO，这意味着复杂意图的识别准确率受限于规则覆盖度。

### 4.3 MCP Skill Bridge 的引入

BeeBotOS 代码中一个重要的新增路径是 `mcp::skill_bridge`：
- Skill id 格式为 `mcp:{server_name}/{tool_name}` 时，通过 MCP client 调用外部 tool
- 参数会经过 JSON Schema validation
- 这是 POAGENT 设计时没有的架构层，代表了 BeeBotOS 从"纯 prompt template skill"向"可执行 tool skill"的演进

### 4.4 Progressive Disclosure 的"半落地"

代码中 `SkillDisclosureLevel`（L0/L1/L2/L3）和 `PromptBuilder::build_skills_section` 都已经实现，但：
- `agent_runtime_impl.rs` 构建 `skill_catalog` 时，注入的是统一格式的完整列表（id + category + description）
- 没有根据当前意图动态选择 L1/L2/L3 来构建 catalog
- `inject_skill_catalog` 的指令也没有区分"只给 description"和"给 full doc"

也就是说，**Progressive Disclosure 的基础设施已经建好，但主流程尚未接入**。当前每轮对话都会把全部 skills 的 description 塞进 prompt，当 skill 数量增多时会有上下文膨胀问题。

### 4.5 Planning 中的 Skill 关联缺失

POAGENT 设计提到 planning 中有"硬编码 domain keyword 映射到 skill id"，但 BeeBotOS 代码中：
- `HybridPlanner` 只有基于关键词选择 `PlanStrategy` 的逻辑
- Plan step 的 `Action::Delegate` 有 `skill_hint` 字段，但没有自动填充机制
- `IntentAnalyzer` 输出 `suggested_approach`，但没有将其映射到具体 skill

这意味着在 BeeBotOS 中，**Planning 和 Skill 系统是相对独立的**：plan 分解出步骤后，步骤执行时的 skill 选择仍然依赖 LLM 自主判断（通过 `skill_catalog`），而不是由 planner 显式指定。

---

## 5. 结论与建议

### 5.1 已实现的优势

1. **独立 Intent Engine**：前置分类节省了 token，优化了路由
2. **Session Skill Inheritance**：解决了多轮对话的 skill 上下文问题
3. **MCP Skill Bridge**：将外部 tool 统一为 skill 接口，扩展性强
4. **Skill 反馈与进化**：`SkillImprovementEngine` + `ToolTrail` + `EvolutionScheduler` 形成了自我改进闭环
5. **Registry 基础设施完整**：search、progressive disclosure、lineage、rollback 都已实现

### 5.2 与 POAGENT 设计方向的对齐度

BeeBotOS 与 POAGENT 的核心设计理念（description 驱动发现 + 按需加载 + LLM 自主判断）**高度一致**。BeeBotOS 在以下方面甚至更进一步：
- 完全下放 Skill 选择给 LLM（禁用 Gateway 关键词匹配）
- 增加了前置意图分类层
- 引入了 MCP 外部工具桥接

### 5.3 待完善项（按优先级）

| 优先级 | 问题 | 建议 |
|-------|------|------|
| **P0** | `skill_catalog` 未做数量裁剪和 usage_count 排序 | 按 usage_count 排序，限制最多 50 个；根据意图选择 L1/L2/L3 |
| **P0** | Progressive Disclosure 主流程未接入 | `inject_skill_catalog` 应根据 `UserIntent` 动态选择披露层级 |
| **P1** | Intent Engine 的 LLM fallback 是 TODO | 实现真正的 dual-track：低置信度时调用轻量 LLM 做分类 |
| **P1** | Planning 与 Skill 系统未打通 | `IntentAnalyzer` 的结果应映射到 `skill_hint`，planner 应在 step 中显式推荐 skill |
| **P1** | 无 embedding / reranker | 当 skill 数量 > 50 时，引入轻量 embedding 做初筛 |
| **P2** | 无多 Skill 编排 | 当前限制为单 skill；复杂任务需要支持 skill pipeline（sequential / parallel / conditional） |
| **P2** | Skill 级权限边界薄弱 | `ApprovalGate` 已存在，但缺少 per-skill tool permission（如 Claude 的 allowed-tools） |
| **P2** | Skill 资源加载不完整 | `SKILL.md` 可指向 scripts/templates，但当前 `read_skill` 只读主文件，无专门资源分层加载协议 |

### 5.4 如果要对齐 Claude Code / Codex 还需要做什么

根据 `agent-peng-prompt.md` 的判断，BeeBotOS 已经比当时的 POAGENT 更接近主流方向。当前差距主要在：

1. **统一 SKILL.md 标准**：当前 front matter + section 的解析逻辑分散在 `builtin_loader.rs` 和 `discovery.rs` 中，需要统一 schema
2. **Skill activation debug trace**：当前没有结构化日志记录"为什么选了这个 skill"，难以调试误触发/漏触发
3. **可选 embedding/rerank**：当内置 skills 超过 50 个时，纯关键词 overlap 不够
4. **allowed_tools / resources / scripts 分层加载**：MCP Bridge 已解决外部 tool 问题，但内置 skill 的脚本/模板/引用资源还没有分层加载协议

---

*文档生成时间：2026-05-08*
*基于代码 commit：当前工作目录状态*
