# 智能体运行时

> **Layer 3: Agent 生命周期与通信**

---

## 运行时架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Agent Runtime Layer                       │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │   A2A    │  │   MCP    │  │  Browser │  │ Workflow │    │
│  │  协议栈  │  │  工具集  │  │  自动化  │  │  引擎    │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘    │
│       │             │             │             │            │
│       └─────────────┴─────────────┴─────────────┘            │
│                              │                               │
│  ┌───────────────────────────┴───────────────────────────┐   │
│  │                    Agent Session                      │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐      │   │
│  │  │   State    │  │   Skills   │  │   Memory   │      │   │
│  │  │  状态管理  │  │  技能管理  │  │  会话记忆  │      │   │
│  │  └────────────┘  └────────────┘  └────────────┘      │   │
│  └───────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## A2A 协议

### 协议分层

```
应用层: 商业逻辑 (询价、报价、结算)
会话层: 对话管理 (多轮协商)
消息层: 消息格式 (意图、载荷)
传输层: libp2p / WebSocket
安全层: TLS + 签名
```

### 消息类型

| 消息 | 方向 | 说明 |
|------|------|------|
| Discover | C→S | 发现服务 |
| Advertise | S→C | 广播服务 |
| Propose | C→S | 发起提议 |
| Negotiate | 双向 | 协商 |
| Accept | 双向 | 接受 |
| Settle | C→S | 结算 |

---

## MCP (Model Context Protocol)

### 工具注册

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name(), tool);
    }
    
    pub async fn execute(&self, name: &str, args: Value) -> Result<Value> {
        self.tools.get(name)?.execute(args).await
    }
}
```

### 内置工具

- web_search
- file_read
- code_execute
- browser_navigate

---

## 会话管理

### Session 生命周期

```
Created → Active → Paused → Resumed → Terminated
              ↓
            Error → Recovery
```

### 状态管理

```rust
pub struct Session {
    id: SessionId,
    agent_id: AgentId,
    state: SessionState,
    context: Context,
    memory: SessionMemory,
}
```

---

## 统一 ReAct 任务处理（2026-05-13 更新）

### 架构演进

旧架构采用 **Intent 前置分类** → 多分支路由：
```
User Input → IntentEngine (启发式) / LLMIntentAnalyzer V2 (LLM)
                ↓
        DirectAnswer │ SingleToolCall │ MultiStepPlanning │ MetaQuestion │ Correction
                ↓
        不同路径：跳过工具 / 单轮 LLM / ReAct / 直接返回目录 / 约束处理
```

**问题**：过早分类导致误路由（如 "BTC 价格" 被误判为闲聊），需要大量 safety net 拦截。

新架构采用 **统一 ReAct 入口**：
```
User Input ──→ process_task ──→ execute_unified_react
                                      ↓
                           PromptBuilder 组装全上下文
                           - Persona (SOUL.md)
                           - Memories (全部，无过滤)
                           - Skills L1/L2 (全部，层次化)
                           - Tools (全部)
                           - ReAct 规则
                                      ↓
                           UnifiedReActExecutor
                           - LLM 自主决定 call_tool / final_answer
                           - 最多 30 轮
                           - L3 按需注入
```

### Skills 上下文注入（L1/L2/L3 渐进式披露）

| 层级 | 内容 | Token 估算 | 注入策略 |
|------|------|-----------|---------|
| **L1** | skill_id + name + one-liner | ~30/skill | **始终注入** |
| **L2** | skill summary（关键能力、参数说明） | ~200/skill | **始终注入** |
| **L3** | 完整 SKILL.md（详细示例、返回值、错误处理） | ~2000/skill | **按需注入** |

**L3 按需注入机制**：
- LLM 在 ReAct thought 中说「需要 `{skill_id}` 的详细文档」
- 系统在下一轮自动从 `SkillRegistry` 获取 L3 文档追加到 context

```markdown
## 技能目录（L1）
以下是你可使用的所有技能。如需了解某个技能的详细用法，参考下方的 L2 摘要；
如需完整文档（L3），可在 thought 中说明「需要 skill_id 的详细文档」。

- weather_assistant: 查询全球任意城市的实时天气和未来预报
- crypto_trader: 加密货币交易下单、持仓查询和订单管理
- ...

## 技能摘要（L2）
### weather_assistant
查询全球城市的实时天气、未来 7 天预报、空气质量指数。
关键能力：支持城市名/坐标输入，返回结构化天气数据。

### crypto_trader
支持加密货币现货/限价/止损订单的下单、撤单、持仓查询。
关键能力：与 Alpaca MCP 集成，支持 BTC/USD、ETH/USD 等交易对。
```

### Tools 全注入

所有可用工具始终注入 context，不再按意图过滤：

```
[可用工具]
- file_read: 读取文件内容
- web_search: 搜索网页
- skill_call: 调用已注册的技能或 MCP 技能
- parallel_delegate: 并行执行多个独立子任务
- mcp:alpaca/place_crypto_order: Alpaca 加密货币下单
- mcp:alpaca/get_crypto_snapshot: 获取加密货币行情快照
```

LLM 在 ReAct 循环内自主决定：
- **是否需要工具**：简单问候可直接 `final_answer`
- **调用哪个工具**：根据任务需要选择性调用
- **何时终止**：自主判断数据是否充足

### 核心代码位置

| 组件 | 文件 | 说明 |
|------|------|------|
| 统一 ReAct 入口 | `crates/agents/src/agent_impl.rs` | `execute_unified_react()` |
| Prompt 组装 | `crates/agents/src/prompt/builder.rs` | `build_unified_react()` |
| Skills L1/L2 构建 | `crates/agents/src/agent_impl.rs` | `build_all_skills_levels()` |
| Tools 构建 | `crates/agents/src/agent_impl.rs` | `build_all_tool_definitions()` |
| ReAct 执行器 | `crates/agents/src/skills/unified_react_executor.rs` | `UnifiedReActExecutor::execute()` |
| L3 动态注入 | `crates/agents/src/skills/unified_react_executor.rs` | `extract_l3_request()` |

---

## 技能系统

### Skill 生命周期

```
Install → Load → Execute → Unload → Remove
```

### WASM Skill

```rust
pub trait Skill {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn execute(&self, input: Input) -> Result<Output>;
}
```

---

**最后更新**: 2026-05-13
