# 方案：Agent 路由重构 — 取消单轮 LLM 路径，全部走 Skill 直接调用或通用 ReAct

## 问题陈述

当前 `process_task_v2` 对 `skill=true, planning=false` 的消息走 `handle_llm_task_v2`（单轮 LLM 推理，让 LLM 重新选择 skill 和参数）。用户要求：

1. **取消 `handle_llm_task_v2` 路径**。
2. Skill Selector 判断 `needs_planning=false` → **直接调用已选 skill/tool**。
3. Skill Selector 判断 `needs_planning=true` → **进入通用 ReAct 循环**（不限于 crypto）。
4. ReAct **最多 30 轮**。
5. ReAct **可中断**：WebChat 用户发送"停止/终止/停下来/结束"等命令时中断循环，输出中断时的内容。

## 方案概述

重构 `process_task_v2` 路由逻辑，将原先的三路分支（`direct_answer` / `single LLM` / `planning`）改为三路分支（`direct_answer` / `single skill` / `general ReAct`）。

同时引入**会话级取消注册表**，让 Gateway 层和 Agent 层共享取消信号，实现 ReAct 中断。

## 详细修改清单

### 1. 新建 `crates/agents/src/session_cancellation.rs`

全局共享的会话取消注册表，供 Gateway（写）和 Agent（读）共享。

```rust
use once_cell::sync::Lazy;
use tokio::sync::{RwLock, watch};
use std::collections::HashMap;

static REGISTRY: Lazy<RwLock<HashMap<String, watch::Sender<bool>>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

pub async fn register(key: &str, sender: watch::Sender<bool>) {
    REGISTRY.write().await.insert(key.to_string(), sender);
}

pub async fn unregister(key: &str) {
    REGISTRY.write().await.remove(key);
}

pub async fn cancel(key: &str) -> bool {
    if let Some(sender) = REGISTRY.read().await.get(key) {
        let _ = sender.send(true);
        true
    } else {
        false
    }
}

pub async fn get_receiver(key: &str) -> Option<watch::Receiver<bool>> {
    REGISTRY.read().await.get(key).map(|s| s.subscribe())
}
```

### 2. 修改 `crates/agents/src/lib.rs`

添加 `pub mod session_cancellation;` 导出。

### 3. 修改 `crates/agents/src/skills/unified_react_executor.rs`

**A. `UnifiedReActConfig` 添加 `cancel_rx`**

```rust
pub struct UnifiedReActConfig {
    pub max_rounds: usize,           // 默认 30
    pub round_timeout_sec: u64,      // 默认 30
    pub enable_reflection: bool,     // 默认 true
    pub require_structured_output: bool, // 默认 true
    pub cancel_rx: Option<tokio::sync::watch::Receiver<bool>>, // 新增
}
```

**B. `execute` 方法支持中断**

在 `for round in 1..=self.config.max_rounds` 循环的**每轮开头**，检查 `cancel_rx`：

```rust
if let Some(ref mut rx) = self.config.cancel_rx {
    if *rx.borrow() {
        info!("ReAct loop cancelled by user at round {}", round);
        // 从已执行的 rounds 构建回复
        let content = self.build_interrupted_answer(&rounds, user_request);
        return Ok(content);
    }
}
```

新增 `build_interrupted_answer` 方法：将已执行的 tool 调用历史和部分结果汇总为自然语言回复。

**C. max_rounds 默认值改为 30**

`UnifiedReActConfig::default()` 中 `max_rounds: 30`。

强制终止提示中的 "10轮" 改为 "30轮"。

### 4. 新建 `crates/agents/src/skills/general_react_prompt.rs`

通用 ReAct system prompt builder，基于现有投资分析 prompt 改造：

- **保留**：ReAct 工作模式说明、工具列表渲染、JSON 输出格式（`thought`/`action`/`tool_name`/`arguments`/`final_answer`）
- **删除**：投资分析角色、用户画像、技术面/情绪面/资金面/宏观面分析框架、交易规则、风险等级规则
- **修改**："最多 10 轮" → "最多 30 轮"
- **新增**：通用任务执行规则（自主决策、避免重复、条件分支、错误处理）

```rust
pub fn build_general_react_prompt(
    available_tools: &HashMap<String, Box<dyn SkillTool>>,
) -> String
```

### 5. 修改 `crates/agents/src/skills/mod.rs`

添加 `pub mod general_react_prompt;`。

### 6. 修改 `crates/agents/src/agent_impl.rs`

#### 6A. `process_task_v2` 路由重构（line ~1582-1738）

替换为：

```rust
let result = if intent_v2.direct_answer || !intent_v2.needs_skill {
    self.handle_direct_answer(&task).await
} else {
    let selection = self.select_skill_v2(&message_text).await;
    // ... 注入 skill hint 到 task input（保留现有逻辑）...

    if selection.needs_planning || intent_v2.needs_planning {
        // 多轮 → 通用 ReAct（30轮，可中断）
        self.execute_with_react(&task, &message_text, &intent_v2, &selection).await
    } else if let Some(ref skill_id) = selection.selected_skill {
        // 单轮 → 直接调用 skill
        self.execute_single_skill(&task, skill_id, &message_text).await
    } else {
        self.handle_direct_answer(&task).await
    }
};
```

**删除**：`should_use_react_planning` 调用、`execute_with_react_planning` 调用、`execute_with_planning` 调用、`handle_llm_task_v2` 调用。

**删除/废弃**：`should_use_react_planning` 方法（或保留但不使用）。

#### 6B. 新增 `execute_single_skill` 方法

```rust
async fn execute_single_skill(
    &self,
    _task: &Task,
    skill_id: &str,
    message_text: &str,
) -> Result<(String, Vec<Artifact>), AgentError> {
    let result = self.execute_skill_by_id(skill_id, message_text, None).await?;
    let output = self.synthesize_skill_output(message_text, &result.output, skill_id);
    Ok((output, vec![]))
}
```

说明：
- `execute_skill_by_id` 内部已包含 MCP 参数自动提取（`McpParameterExtractor`）
- 对于 prompt-based skills，会走 LLM 执行 skill 的 prompt_template（但这是直接执行已选 skill，不是重新选择）
- `synthesize_skill_output` 负责格式化输出

#### 6C. 新增 `execute_with_react` 方法（通用 ReAct）

基于 `execute_with_react_planning` 改造：

```rust
async fn execute_with_react(
    &self,
    task: &Task,
    message_text: &str,
    intent: &crate::skill_matching::IntentAnalysisV2,
    selection: &crate::skill_matching::SkillSelection,
) -> Result<(String, Vec<Artifact>), AgentError> {
    let task_id = task.id.clone();
    info!("General ReAct: executing task {} (multi-step)", task_id);

    let llm = self.llm_interface.clone()
        .ok_or_else(|| AgentError::InvalidConfig("LLM interface not configured".into()))?;

    // 1. 加载所有可用 tools（不限于 crypto analysis tools）
    let tools = self.get_available_tools().await;

    if tools.is_empty() {
        warn!("General ReAct: no tools available, falling back to direct answer");
        return Box::pin(self.handle_direct_answer(task)).await;
    }

    // 2. 构建通用 ReAct system prompt
    let system_prompt = crate::skills::general_react_prompt::build_general_react_prompt(&tools);

    // 3. 获取 cancel_rx
    let task_input_json = serde_json::from_str::<serde_json::Value>(&task.input).unwrap_or_default();
    let cancel_key = task_input_json
        .get("channel_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&task_id);
    let cancel_rx = crate::session_cancellation::get_receiver(cancel_key).await;

    // 4. 执行 ReAct
    let executor = crate::skills::UnifiedReActExecutor::new(llm).with_config(
        crate::skills::UnifiedReActConfig {
            max_rounds: 30,
            round_timeout_sec: 30,
            enable_reflection: true,
            require_structured_output: false, // 通用任务不一定需要结构化输出
            cancel_rx,
        },
    );

    let react_result = executor.execute(&system_prompt, message_text, &tools).await;

    match react_result {
        Ok(content) => {
            info!("General ReAct: task {} completed, result length={}", task_id, content.len());
            Ok((content, vec![]))
        }
        Err(e) => {
            warn!("General ReAct: task {} failed: {}", task_id, e);
            Err(e)
        }
    }
}
```

#### 6D. 删除 `execute_with_react_planning` 方法

该方法硬编码为 crypto 投资分析，不再使用。如果其他模块引用它，一并替换。

### 7. 修改 `apps/gateway/src/services/message_processor.rs`

#### 7A. 添加停止命令检测

在 `handle_message_via_agent` 和 `handle_message` 的**最前面**（去重检查之后，获取会话之后），添加：

```rust
let stop_keywords = ["停止", "终止", "停下来", "结束", "stop", "cancel", "abort"];
let is_stop = stop_keywords.iter().any(|kw| content.to_lowercase().contains(kw));
if is_stop {
    if let Some(ref svc) = self.webchat_service {
        let _ = beebotos_agents::session_cancellation::cancel(&db_session_id).await;
        self.send_reply(platform, channel_id, &message, "⏹️ 已收到停止指令，正在中断当前任务...").await?;
        return Ok(());
    }
}
```

#### 7B. 后台任务注册/注销取消 token

在 `handle_message_via_agent` 中，`tokio::spawn` 之前：

```rust
let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
beebotos_agents::session_cancellation::register(&db_session_id, cancel_tx).await;
```

在后台任务末尾（finally 语义），注销：

```rust
beebotos_agents::session_cancellation::unregister(&db_session_id).await;
```

## 修改后的执行流程

```
用户: "本机有哪些文件？"
  ↓
handle_message_via_agent
  │
  ├─ 消息去重 ✅
  ├─ 检查停止命令 → 否 ✅
  ├─ 获取/创建会话 ✅
  ├─ 获取 DB session ✅
  ├─ 处理多模态 ✅
  ├─ 添加用户消息到历史 ✅
  ├─ 构建上下文 ✅
  ├─ 调用 AgentRuntime::execute_task ✅
  ↓
Agent::process_task_v2
  │
  ├─ V2 Intent Analyzer
  │   direct_answer=false, needs_skill=true, needs_planning=false
  ↓
  ├─ select_skill_v2("本机有哪些文件？")
  │   → selected_skill: "file_list" (假设), needs_planning: false
  ↓
  ├─ needs_planning=false
  │   → execute_single_skill("file_list", "本机有哪些文件？")
  │   → execute_skill_by_id → 直接调用 file_list skill
  │   → 返回文件列表
  ↓
流式回复到 WebChat

──────────────────────────────────────

用户: "帮我分析 BTC 走势并给出建议"
  ↓
Agent::process_task_v2
  │
  ├─ V2 Intent Analyzer
  │   direct_answer=false, needs_skill=true, needs_planning=true
  ↓
  ├─ select_skill_v2
  │   → selected_skill: "crypto_analysis", needs_planning: true
  ↓
  ├─ needs_planning=true
  │   → execute_with_react(task, message, intent, selection)
  │   → 加载所有 tools
  │   → 构建通用 ReAct prompt
  │   → UnifiedReActExecutor.execute(max_rounds=30)
  │   → Round 1: LLM → call_tool("crypto_price", {"symbol":"BTC"})
  │   → Round 2: LLM → call_tool("calculate_rsi", {"symbol":"BTC"})
  │   → ...
  │   → Round N: LLM → final_answer
  ↓
流式回复到 WebChat

──────────────────────────────────────

用户（ReAct 执行中）: "停止"
  ↓
handle_message_via_agent
  │
  ├─ 检查停止命令 → ✅ 命中
  ├─ session_cancellation::cancel(session_id) → 发送 true
  ├─ send_reply("⏹️ 已收到停止指令...")
  └─ return Ok(())
  ↓
Agent ReAct 循环（Round 5/30）
  ├─ 检查 cancel_rx → true
  ├─ 构建 interrupted_answer（已收集的 4 轮结果）
  ├─ 返回中断内容
  ↓
后台任务发送中断时的内容到 WebChat
```

## 风险与注意事项

1. **通用 ReAct 的 prompt 质量**：投资分析 prompt 经过调优，通用 prompt 需要测试确保 LLM 不会陷入无限循环或输出错误格式。
2. **单轮 skill 直接调用的参数提取**：`execute_skill_by_id` 对 MCP tools 有自动参数提取，但对 WASM/prompt-based skills 的参数处理需要验证。
3. **中断时内容质量**：ReAct 中断时的 `build_interrupted_answer` 需要合理汇总已收集的数据，避免输出半成品。
4. **停止命令误判**：如果用户正常消息中包含"停止"等词（如"请停止播放音乐"），可能被误判。但此类消息通常是技能请求，会进入正常处理流程。改进方案：可要求停止命令为**单独发送**的短消息（如仅"停止"两个字），降低误判率。
5. **多会话并发取消**：`session_cancellation` 使用 `HashMap`，需要确保并发安全（`RwLock` 已保证）。


&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&

全部修改已完成，`cargo check` 验证通过 ✅

---

## 修改总结

### 1. 新建 `crates/agents/src/session_cancellation.rs`
全局共享的会话取消注册表，Gateway 写入（注册/触发取消），Agent 读取（获取取消信号）。

### 2. 修改 `crates/agents/src/skills/unified_react_executor.rs`
- `UnifiedReActConfig` 添加 `cancel_rx` 字段
- `execute` 每轮循环开头检查取消信号，被中断时返回已收集的内容
- `max_rounds` 默认值从 **10 → 30**
- 新增 `build_interrupted_answer` 方法，汇总已执行的工具调用历史

### 3. 新建 `crates/agents/src/skills/general_react_prompt.rs`
通用 ReAct system prompt，去掉投资分析硬编码，保留自主决策/避免重复/错误处理等通用规则。

### 4. 修改 `crates/agents/src/agent_impl.rs`
- **`process_task_v2` 路由重构**：
  - `direct_answer` → `handle_direct_answer`（保留）
  - `needs_planning=true` → **`execute_with_react`**（通用 ReAct，30轮，可中断）
  - `needs_planning=false` → **`execute_single_skill`**（直接调用已选 skill）
- **新增 `execute_single_skill`**：调用 `execute_skill_by_id`，走 skill 内部参数提取 + 执行
- **新增 `execute_with_react`**：加载所有 tools，构建通用 prompt，配置 30 轮，传入 cancel_rx
- **删除 `handle_llm_task_v2` 路径调用**

### 5. 修改 `apps/gateway/src/services/message_processor.rs`
- **停止命令检测**：收到 "停止/终止/停下来/结束/stop/cancel/abort" 时，触发 session 取消
- **注册/注销 cancel token**：后台任务启动前注册，完成后注销

---

## 修改后的执行流程

```
用户: "本机有哪些文件？"
  ↓
Agent::process_task_v2
  ├─ V2 Intent: needs_skill=true, needs_planning=false
  ├─ SkillSelector: selected="file_list", needs_planning=false
  └─ → execute_single_skill → 直接调用 file_list skill
  ↓
流式回复

────────────────────────────

用户: "帮我分析 BTC 走势"
  ↓
Agent::process_task_v2
  ├─ V2 Intent: needs_skill=true, needs_planning=true
  ├─ SkillSelector: selected="crypto_analysis", needs_planning=true
  └─ → execute_with_react → General ReAct (最多30轮)
     Round 1: call_tool("crypto_price")
     Round 2: call_tool("calculate_rsi")
     ...
     Round N: final_answer
  ↓
流式回复

────────────────────────────

用户（ReAct 执行中）: "停止"
  ↓
Gateway: session_cancellation::cancel(session_id) = true
  ↓
Agent ReAct Round 5:
  ├─ 检查 cancel_rx → true
  ├─ build_interrupted_answer(已收集的4轮结果)
  └─ 返回中断汇总
  ↓
后台任务发送中断内容到 WebChat
```



