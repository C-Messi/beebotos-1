//! Event Sequencer

use crate::error::Result;
use std::collections::VecDeque;

/// Sequenced event
#[derive(Debug)]
pub struct SequencedEvent {
    pub sequence: u64,
    pub event: super::bus::Event,
}

/// Event sequencer
pub struct EventSequencer {
    sequence: u64,
    buffer: VecDeque<SequencedEvent>,
}

impl EventSequencer {
    pub fn new() -> Self {
        Self {
            sequence: 0,
            buffer: VecDeque::new(),
        }
    }

    pub fn sequence(&mut self, event: super::bus::Event) -> SequencedEvent {
        self.sequence += 1;
        let sequenced = SequencedEvent {
            sequence: self.sequence,
            event,
        };
        self.buffer.push_back(sequenced.clone());
        sequenced
    }

    pub fn next(&mut self) -> Option<SequencedEvent> {
        self.buffer.pop_front()
    }

    pub fn current_sequence(&self) -> u64 {
        self.sequence
    }
}

impl Default for EventSequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SequencedEvent {
    fn clone(&self) -> Self {
        unimplemented!("Event clone not implemented")
    }
}

&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&

基于最新的生产日志，已完成三项关键修复。以下是完整的分析和修复总结：

---

## 🚨 问题 1：`kimi-flash` 404 导致 Skill Selection 完全失效

### 现象
```
Provider primary failed: API error: 404 - {"error":{"message":"Not found the model kimi-flash or Permission denied"}}
V2 Skill selection failed ... All providers failed or are unavailable
```

### 根因
上次的 `model: "kimi-flash"` hint 被传递到了 `LlmService`，但当前 Moonshot API key **没有 `kimi-flash` 的访问权限**。每次 skill selection 都返回 404，导致：
1. `FailoverProvider` 将 provider 标记为不健康（consecutive_failures += 1）
2. 连续 3 次失败后，provider 被 circuit breaker 永久跳过
3. 由于没有 fallback provider，所有后续 LLM 调用都失败

### 修复
**移除了 `model: "kimi-flash"` hint**，skill selection 恢复使用默认模型 `kimi-k2.6`。

---

## 🚨 问题 2：FailoverProvider 对 404 客户端错误过度惩罚

### 根因
`FailoverProvider::try_complete` 对**所有**错误都调用 `update_provider_health(index, false)`，包括：
- 404（模型不存在）
- 400（请求参数错误）
- 401（认证失败）

这些是**客户端错误**，provider 本身是正常的。但 circuit breaker 将其视为 provider 故障，导致 provider 被错误地标记为不健康。

### 修复
新增 `is_transient_error()` 辅助函数，区分**瞬态错误**和**非瞬态客户端错误**：

| 错误类型 | 是否瞬态 | 是否影响健康 |
|----------|----------|--------------|
| Network, Timeout | ✅ | ✅ 标记不健康 |
| RateLimit (429) | ✅ | ✅ 标记不健康 |
| Server error (5xx) | ✅ | ✅ 标记不健康 |
| **Not found (404)** | ❌ | ❌ **不标记** |
| **Bad request (400)** | ❌ | ❌ **不标记** |
| **Auth (401)** | ❌ | ❌ **不标记** |
| Context length exceeded | ❌ | ❌ **不标记** |

这样，即使未来传了错误的 model 名，provider 也不会被 circuit breaker 错误地禁用。

---

## 🚨 问题 3：Planning 任务 180s 超时

### 现象
"到成都市5天的旅游规划" 任务触发 planning，生成 5 个 step，每个 step 执行 `Travel Planner` knowledge skill，最终 180s 超时：

```
Task bb855fe6... failed after 188294ms: Plan execution timed out after 180s
```

### 根因深度分析

**`KnowledgeSkillExecutor` 错误地使用了 `ReActExecutor`**：

```rust
// knowledge_executor.rs (旧代码)
let tools = default_tool_set(&tool_root);  // file_list, process_exec, bash_shell...
let executor = ReActExecutor::new(self.llm.clone(), tools);
executor.execute(&system_prompt, user_input).await
```

对于纯知识型 skill（如 Travel Planner），`ReActExecutor` 会：
1. 在 prompt 中注入 5 个工具定义（file_list, bash_shell 等）
2. 让 LLM 在 ReAct loop 中反复调用这些工具
3. LLM 尝试用 `file_list` 和 `bash_shell` 来"研究"旅游信息，完全不合理
4. 每次 ReAct step 都发起一次 LLM 调用，耗时 10-60s
5. 5 个 planning step × 多轮 ReAct = 远超 180s

日志证据：
```
Executing tool 'file_list' at step 1
Executing tool 'process_exec' at step 1
Executing tool 'bash_shell' at step 1
...
✅ Received LLM response: length=5049, latency=63757ms, tokens=2159/1474
```

### 修复
**`KnowledgeSkillExecutor` 改为单轮 LLM 调用**，完全移除 ReAct loop：

```rust
// knowledge_executor.rs (新代码)
let messages = vec![
    CommMessage::new(..., system_prompt),   // skill instructions
    CommMessage::new(..., user_input),      // user query
];
let response = self.llm.call_llm(messages, None).await?;
```

知识型 skill 的本质是"用 LLM 的内置知识回答用户"，**不需要任何工具执行**。ReAct 只会：
- 增加 prompt 噪音（注入无关的工具定义）
- 增加 LLM 调用次数（每轮 ReAct 都调一次 LLM）
- 导致 LLM 幻觉工具调用（用 bash 命令查旅游攻略）

---

## ✅ 所有变更文件

| 文件 | 变更 |
|------|------|
| `crates/agents/src/skill_matching/skill_selector.rs` | 移除 `kimi-flash` model hint |
| `crates/agents/src/llm/failover.rs` | 新增 `is_transient_error()`，404/400 等客户端错误不再影响 provider 健康 |
| `crates/agents/src/skills/knowledge_executor.rs` | **重写**：从 ReAct loop 改为单轮 LLM 调用 |

---

## 📋 预期效果

1. **Skill Selection 恢复正常**：不再因 `kimi-flash` 404 而完全失败
2. **FailoverProvider 更健壮**：客户端错误不会导致 provider 被 circuit breaker 禁用
3. **Knowledge skill 执行大幅提速**：从多轮 ReAct（60-180s）变为单轮 LLM（2-10s）
4. **Planning 任务不再超时**：每个 planning step 从 30-60s 降至 2-5s，5 个 step 总计约 10-25s

%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%

已完成针对 Skill Selection 持续超时的根本性修复。以下是本次变更的核心内容：

---

## 🔧 根本性修复：极简 Ranking Prompt

### 核心问题
之前的 ranking prompt 要求 LLM 输出**完整的嵌套 JSON**，包含 5 个候选的 4 维评分 + reason + selection_reasoning，导致：
- 输出 token 数：500-800+
- 生成时间：15-25s（API 波动时超时）

### 修复方案

**1. 输出格式从 JSON 改为 3 行纯文本**

```
selected_skill: city-weather
needs_planning: no
scores: city-weather:9.5,weather-assistant:7.0,game-ai-player:2.0
```

LLM 只需生成约 **30-50 个 token**，而不是 500-800。

**2. Prompt 结构大幅精简**

| 改动 | 之前 | 之后 |
|------|------|------|
| 候选描述格式 | 多行 markdown（when/desc/caps/examples） | 单行紧凑格式 |
| when_to_use 截断 | 200 字符 | **150 字符** |
| description 截断 | 100 字符 | **80 字符** |
| 正/负示例 | 各保留 1 条 | **完全移除** |
| 评分维度 | Relevance/Specificity/Capability/Negative | **仅保留 overall_score** |
| 输出要求 | 完整 JSON + reasoning | **3 行纯文本** |

**3. 参数调整**

| 参数 | 之前 | 之后 |
|------|------|------|
| `max_candidates` | 5 | **3** |
| `timeout` | 25s | **30s** |
| `max_tokens` | 768 | **256** |

---

## 📋 预期效果

- **输出 token 减少 ~90%**（从 500-800 降至 30-50）
- **生成时间预计减少 ~80%**（从 15-25s 降至 3-8s）
- **TTFB 不变**（由 API 负载决定），但整体请求应能在 10-15s 内完成
- 30s timeout 作为 API 高峰期的安全兜底

---

## 🔥 其他已修复问题（本次迭代）

| 问题 | 修复 |
|------|------|
| `kimi-flash` 404 导致 provider 被禁用 | 移除了 `model` hint |
| 404 错误永久禁用 provider | `FailoverProvider` 新增 `is_transient_error()`，客户端错误不再影响健康状态 |
| Knowledge skill ReAct 导致 Planning 180s 超时 | `KnowledgeSkillExecutor` 改为**单轮 LLM 调用**，不再执行工具循环 |

---

## ⚠️ 仍需注意的执行错误

从日志中看到以下错误与 **Skill Selection 超时无关**，是后续执行阶段的问题：

1. **天气脚本返回 exit code 1** — `city-weather` 脚本需要网络访问（Open-Meteo API），请确认已设置 `BEE_ALLOW_NETWORK=1`
2. **BTC MCP ConnectError** — `mcp-alpaca` 服务连接失败，请检查 MCP server 是否运行
3. **地铁查询匹配到 akshare_stock** — 这是 Skill Selection 超时后的 fallback 路径问题，应在 skill selection 修复后自然改善

---

## 📁 变更文件

- `crates/agents/src/skill_matching/skill_selector.rs` — 极简 prompt + 参数调整
- `crates/agents/src/llm/failover.rs` — 客户端错误不标记 provider 不健康
- `crates/agents/src/skills/knowledge_executor.rs` — 移除 ReAct，改为单轮调用
- `apps/gateway/src/services/llm_service.rs` — model_override 支持（预留）
- `apps/gateway/src/services/agent_runtime_manager.rs` — model hint 传递（预留）


&&&&&&&&&&&&&&&&&&&&&&&&&&&&&


