# BeeBotOS Agent 模块细分技术设计方案

> **版本**: v1.0  
> **日期**: 2026-05-27  
> **状态**: 设计阶段  
> **范围**: `crates/agents` → `crates/agent-*`  

---

## 1. 背景与动机

当前 `beebotos-agents`（`crates/agents`）是 BeeBotOS 中最核心的 crate，也是**体积最大、复杂度最高**的单体模块：

| 指标 | 现状 |
|------|------|
| **代码总行数** | ~97,709 行 |
| **源文件数** | 200+ `.rs` 文件 |
| **顶层子模块** | 40+ 个（`memory/`, `evolution/`, `communication/`, `llm/`...） |
| **外部依赖** | 60+ 个 crates（`tokio`, `sqlx`, `alloy-*`, `feishu-sdk`, `petgraph` 等） |
| **编译单元** | 单一 `lib` crate，任何子模块改动触发全量重编译 |

### 1.1 面临的问题

1. **编译效率低下**：全量编译时间长，开发迭代慢；任何渠道适配器（如 `wechat_channel.rs`）的小改动触发整个 Agent 层重编译。
2. **认知负载过高**：新开发者面对 200+ 文件、40+ 子模块难以快速定位代码。
3. **测试粒度粗**：单元测试必须拉取整个 crate 的依赖，CI 耗时严重。
4. **依赖污染**：通信层的 `feishu-sdk`、区块链层的 `alloy-*` 被所有 Agent 子模块强制继承，无法按需裁剪。
5. **版本耦合**：A2A 协议升级可能意外影响记忆系统；渠道适配器重构存在风险波及核心运行时。

### 1.2 目标状态

将单体 `beebotos-agents` 按**功能内聚、依赖解耦、分层清晰**的原则，拆分为 **10 个独立子 crate + 1 个 Facade crate**，实现：

- 各 crate **可独立编译、测试、发布**
- 依赖关系为严格 **DAG（无循环依赖）**
- 通过 `agent-core` 中的 **Trait 抽象** 实现模块间解耦
- **向后 100% 兼容**：现有 `apps/gateway` 等使用者无需修改代码

---

## 2. 设计目标

| 目标 | 说明 |
|------|------|
| **单一职责** | 每个子 crate 只负责一个明确的 Agent 能力域 |
| **最小依赖** | 子 crate 仅声明其真实需要的外部依赖 |
| **Trait 解耦** | 跨模块交互通过 `agent-core` 定义的 Trait 进行，避免直接依赖具体实现 |
| **增量迁移** | 支持分阶段拆分，不阻塞主分支开发 |
| **零 Breaking Change** | 通过 `beebotos-agents` Facade crate 保持现有 API 不变 |

---

## 3. 拆分原则

1. **按能力域拆分**：以 Agent 的核心能力边界（记忆、进化、通信、规划等）为划分依据，而非简单的文件目录平移。
2. **稳定依赖抽象**：`agent-core` 作为唯一被所有模块依赖的基础层，包含类型定义与 Trait 接口；禁止任何子 crate 被 `agent-core` 反向依赖。
3. **可选依赖**：各子 crate 对 `agent-core` 中 Trait 的实现是**可选的**；例如 `agent-memory` 实现 `MemorySearch`，但 `agent-planning` 只**使用** `MemorySearch` Trait。
4. **分层架构**：
   - **L0 基础层**：`agent-core`
   - **L1 能力层**：`agent-llm`, `agent-memory`, `agent-security`
   - **L2 业务层**：`agent-skills`, `agent-planning`, `agent-communication`, `agent-evolution`
   - **L3 协作层**：`agent-collaboration`
   - **L4 运行时层**：`agent-runtime`
   - **L5 门面层**：`beebotos-agents`（Facade）
5. **代码量均衡**：每个子 crate 控制在 5,000–20,000 行之间，避免过细（<3,000 行，维护 overhead 高）或过粗（>25,000 行，失去拆分意义）。

---

## 4. 总体架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        L5: Facade Layer                                     │
│                    beebotos-agents (Facade)                                 │
│            重新导出所有 agent-* 模块，保持现有 API 不变                       │
└─────────────────────────────────────────────────────────────────────────────┘
                                      ▲
┌─────────────────────────────────────────────────────────────────────────────┐
│                        L4: Runtime Layer                                    │
│  agent-runtime: AgentRuntime, SessionPool, Scheduler, StateMachine, Queue   │
└─────────────────────────────────────────────────────────────────────────────┘
       ▲                    ▲                    ▲                    ▲
┌──────┴──────┐    ┌────────┴────────┐    ┌────┴─────┐    ┌──────────┴──────────┐
│ L3: Collab  │    │ L2: Evolution   │    │ L2: Plan │    │ L2: Communication   │
│ agent-      │    │ agent-evolution │    │ agent-   │    │ agent-communication │
│ collaboration│   │ SkillDistiller  │    │ planning │    │ Channel Adapters    │
│ A2A / DID   │    │ PatchEngine     │    │ Workflow │    │ MessageRouter       │
│ Spawning    │    │ CAPO/DAPO/PAPO  │    │ DAG      │    │ Webhook             │
└──────┬──────┘    └────────┬────────┘    └────┬─────┘    └──────────┬──────────┘
       │                    │                  │                     │
       └────────────────────┴──────────────────┴─────────────────────┘
                                      ▲
┌─────────────────────────────────────────────────────────────────────────────┐
│                        L2: Skills Layer                                     │
│  agent-skills: SkillRegistry, ReActExecutor, UnifiedReActExecutor, MCP      │
└─────────────────────────────────────────────────────────────────────────────┘
                                      ▲
┌──────────────┬──────────────┬───────┴────────┬──────────────┬──────────────┐
│ L1: LLM      │ L1: Memory   │ L1: Security   │              │              │
│ agent-llm    │ agent-memory │ agent-security │              │              │
│ LLMClient    │ HybridSearch │ SecurityManager│              │              │
│ Providers    │ Embedding    │ ApprovalGate   │              │              │
│ PromptBuilder│ MarkdownStore│ PermissionSystem│             │              │
└──────────────┴──────────────┴────────────────┴──────────────┴──────────────┘
                                      ▲
┌─────────────────────────────────────────────────────────────────────────────┐
│                        L0: Core Layer                                       │
│  agent-core: AgentId, TaskType, AgentError, MemorySearch, LLMCallInterface  │
│              ToolExecutor, EventHandler, SkillResolver, DID...              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. 模块详细设计

### 5.1 agent-core（基础核心层）

> **路径**: `crates/agent-core`  
> **预估规模**: ~5,000 行  
> **定位**: 所有 Agent 子模块的**唯一公共基础**，不包含任何业务实现，只有类型、Trait、错误、工具函数。

#### 5.1.1 职责范围

| 类别 | 内容 |
|------|------|
| **基础类型** | `AgentId`, `TaskId`, `DID`, `AgentMetadata`, `SessionId`, `PlanId` |
| **任务系统** | `TaskType` 枚举、`ExecutionTask`、`TaskResult`、`Artifact` |
| **错误体系** | `AgentError` 枚举（25+ 变体）、`AgentResult<T>` |
| **核心 Trait** | `MemorySearch`, `LLMCallInterface`, `ToolExecutor`, `EventHandler`, `SkillResolver`, `PlatformAdapter`, `MessageHandler`, `Planner`, `Decomposer`, `RePlanner` |
| **配置结构** | `AgentConfig`, `MemoryConfig`, `PersonalityConfig`, `AgentBuilder`（仅声明，无实现） |
| **通用工具** | 序列化/反序列化辅助、时间戳工具、ID 生成器 |

#### 5.1.2 核心 Trait 定义示例

```rust
/// 记忆搜索抽象，由 agent-memory 实现，其他模块仅依赖此 Trait
#[async_trait]
pub trait MemorySearch: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> AgentResult<Vec<MemoryEntry>>;
    async fn store(&self, entry: MemoryEntry) -> AgentResult<()>;
}

/// LLM 调用抽象，由 agent-llm 实现
#[async_trait]
pub trait LLMCallInterface: Send + Sync {
    async fn call_llm(&self, messages: Vec<Message>, config: ModelConfig) -> AgentResult<String>;
    async fn call_llm_stream(&self, messages: Vec<Message>, config: ModelConfig) -> AgentResult<BoxStream<'static, String>>;
    fn supports_native_tools(&self) -> bool;
}

/// 工具执行抽象，由 agent-skills / agent-mcp 实现
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, args: Value) -> AgentResult<Value>;
}

/// 技能解析抽象
pub trait SkillResolver: Send + Sync {
    fn resolve(&self, skill_id: &str) -> Option<SkillMetadata>;
    fn list_skills(&self) -> Vec<SkillMetadata>;
}
```

#### 5.1.3 外部依赖

```toml
[dependencies]
beebotos-core = { path = "../core" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
async-trait = "0.1"
uuid = { version = "1.7", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

> **原则**: `agent-core` **不依赖** `tokio`（使用 `async-trait` 即可）、**不依赖** `sqlx`、**不依赖**任何 Agent 子模块。

---

### 5.2 agent-llm（大语言模型层）

> **路径**: `crates/agent-llm`  
> **预估规模**: ~10,000 行  
> **定位**: 统一的 LLM 客户端、模型路由、提示词构建器，屏蔽 12+ 提供商差异。

#### 5.2.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `client.rs` | `LLMClient`, `LLMClientBuilder`，统一入口 |
| `failover.rs` | 多模型故障转移、降级策略 |
| `adapter.rs` | 请求/响应格式适配层 |
| `prompt/` | `PromptBuilder`, `PromptCache`，统一 ReAct Prompt 组装 |
| `models/` | `ModelConfig`, `CompletionRequest/Response`, `ModelRouter`, `CostEstimator` |
| `providers/` | 12+ 提供商实现：`openai.rs`, `claude.rs`, `deepseek.rs`, `kimi.rs`, `gemini.rs`, `doubao.rs`, `qwen.rs`, `zhipu.rs`, `ollama.rs`, `anthropic.rs`... |
| `types.rs` | `Message`, `Role`, `Tool`, `FunctionDefinition` |

#### 5.2.2 关键设计

- **实现 `agent_core::LLMCallInterface`**，供 `agent-skills`、`agent-communication`、`agent-evolution` 使用。
- 内部维护 **Provider 注册表**，支持运行时动态增删模型。
- `PromptBuilder` 从 `agent_impl.rs` 和 `prompt/builder.rs` 迁移而来，但**不再硬引用** `SkillRegistry` 或 `MemorySearch`，而是通过参数传入。

#### 5.2.3 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
reqwest = { version = "0.11", features = ["json", "stream"] }
tokio = { version = "1.36", features = ["sync", "time"] }
tracing = "0.1"
serde_json = "1.0"
```

> **移除依赖**: 不再依赖 `sqlx`, `feishu-sdk`, `alloy-*`, `petgraph`。

---

### 5.3 agent-memory（记忆系统层）

> **路径**: `crates/agent-memory`  
> **预估规模**: ~8,000 行  
> **定位**: Agent 的长期记忆、短期工作记忆、向量检索、Markdown 文件存储。

#### 5.3.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `MemoryManager`, `MemoryEntry`, `MemoryLimitsConfig` |
| `search.rs` | 统一搜索接口，实现 `agent_core::MemorySearch` |
| `hybrid_search.rs` | Vector + BM25 混合搜索 |
| `hybrid_search_sqlite.rs` | SQLite FTS5 实现 |
| `embedding.rs` | 多提供商 Embedding 生成（调用 `agent-llm` 接口） |
| `local.rs` | 本地记忆缓存 |
| `markdown_storage.rs` | File-is-Truth Markdown 存储 |
| `markdown_search.rs` | Markdown 内容检索 |
| `memory_flush.rs` / `memory_flush_llm.rs` | 上下文窗口满时自动持久化 |
| `backup.rs` / `sync.rs` | 备份与同步 |
| `qmd.rs` | QMD 格式支持 |

#### 5.3.2 关键设计

- **实现 `agent_core::MemorySearch`** Trait，对外只暴露 `search`/`store`/`delete`。
- 内部依赖 `agent-llm` 仅用于 **Embedding 生成**（通过 `LLMCallInterface` 调用）。
- `agent-skills` 和 `agent-runtime` **不直接依赖** `agent-memory`，而是依赖 `agent_core::MemorySearch` Trait，便于未来替换为分布式记忆后端。

#### 5.3.3 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
agent-llm = { path = "../agent-llm" }  # 仅用于 embedding
tokio = { version = "1.36", features = ["sync", "fs"] }
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-native-tls"] }
rusqlite = { version = "0.32", features = ["bundled"] }
lru = "0.12"
dirs = "5.0"
```

---

### 5.4 agent-security（安全与权限层）

> **路径**: `crates/agent-security`  
> **预估规模**: ~5,000 行  
> **定位**: Agent 的安全沙箱、权限校验、审批门、会话隔离。

#### 5.4.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `SecurityManager`, `SecurityConfig` |
| `approval.rs` | `ApprovalGate`, `ApprovalRequest`, `RiskLevel`, `ApprovalMode` |
| `permission_system.rs` | `PermissionChecker`, `PermissionContext`, `PermissionResult` |
| `session_isolation.rs` | `SessionIsolationManager`, `IsolationLevel`, `ResourceLimits` |
| `webhook_security.rs` | Webhook 签名验证 |

#### 5.4.2 关键设计

- 实现 `agent_core::SecurityPolicy` Trait（新增）。
- `agent-runtime` 在任务执行前后调用 `SecurityManager::check()` 和 `ApprovalGate::request()`。
- 与 `beebotos-kernel` 的 Capability 系统对接，将 L1–L12 能力等级映射为 `PermissionResult`。

#### 5.4.3 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
beebotos-kernel = { path = "../kernel", features = ["wasm"] }
tokio = { version = "1.36", features = ["sync"] }
```

---

### 5.5 agent-skills（技能与 ReAct 执行层）

> **路径**: `crates/agent-skills`  
> **预估规模**: ~15,000 行  
> **定位**: Skill 注册发现、ReAct 执行循环、MCP 桥接、工具调用编排。

#### 5.5.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `SkillRegistry`（从 `skills/registry.rs` + `skill_matching/` 合并） |
| `executor.rs` / `react_executor.rs` | 经典 ReAct 执行器 |
| `unified_react_executor.rs` | **统一 ReAct 入口**（当前核心路径） |
| `general_react_prompt.rs` | ReAct Prompt 模板 |
| `composition/` | 技能组合：流水线、并行、条件、循环 |
| `discovery.rs` | Skill 自动发现 |
| `builtin_loader.rs` | 内置工具加载（`read_file`, `write_file`, `exec` 等） |
| `mcp_parameter_extractor.rs` | MCP 工具参数解析 |
| `code_executor.rs` | 代码执行沙箱接口 |
| `process_sandbox.rs` | 进程级沙箱 |
| `investment_analysis/` | 投资分析专用技能（保留） |

#### 5.5.2 MCP 子模块整合

将原 `crates/agents/src/mcp/` 整体迁入 `agent-skills/src/mcp/`：

| 文件 | 职责 |
|------|------|
| `mcp/mod.rs` | `MCPManager` |
| `mcp/client.rs` | MCP 客户端 |
| `mcp/server.rs` | MCP 服务器 |
| `mcp/skill_bridge.rs` | MCP ↔ Skill 桥接 |
| `mcp/transport/` | HTTP / stdio 传输 |

> **原因**: MCP 本质上是**外部工具的动态发现与调用协议**，与 Skill 系统高度内聚，不宜独立为 crate（规模约 2,000 行，过细）。

#### 5.5.3 关键设计

- **不直接依赖** `agent-memory`，而是通过 `agent_core::MemorySearch` Trait 获取上下文记忆。
- **依赖** `agent-llm` 的 `LLMCallInterface` 执行 ReAct 循环中的 LLM 调用。
- `UnifiedReActExecutor::execute()` 的参数从 `Agent` 结构体改为**显式注入接口**：
  ```rust
  pub async fn execute(
      &self,
      task: &ExecutionTask,
      llm: &dyn LLMCallInterface,
      memory: Option<&dyn MemorySearch>,
      skill_resolver: &dyn SkillResolver,
      tools: Vec<Box<dyn ToolExecutor>>,
  ) -> AgentResult<TaskResult>
  ```
- 这样 `agent-skills` 成为**纯业务逻辑 crate**，不持有任何 `Arc<RwLock<...>>` 状态。

#### 5.5.4 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
agent-llm = { path = "../agent-llm" }
beebotos-kernel = { path = "../kernel", features = ["wasm"] }
beebotos-foreign-rt = { path = "../foreign-rt" }
tokio = { version = "1.36", features = ["process", "sync"] }
wasmparser = "0.246"
serde_json = "1.0"
```

---

### 5.6 agent-planning（规划与任务分解层）

> **路径**: `crates/agent-planning`  
> **预估规模**: ~12,000 行  
> **定位**: 任务规划引擎、计划执行、动态重规划、工作流编排、队列调度。

#### 5.6.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `PlanningEngine`, `PlanExecutor`, `RePlanner` 入口 |
| `plan.rs` | `Plan`, `PlanStep`, `PlanStatus`, `StepStatus`, `StepType` |
| `decomposer.rs` | 任务分解器（层次/并行/领域/复合） |
| `engine.rs` | 规划策略选择（ReAct, CoT, Goal-based, Hybrid） |
| `executor.rs` | 计划执行器（顺序/并行/自适应） |
| `replanner.rs` | 反馈驱动重规划 |
| `tool_chain.rs` | 工具链压缩优化 |
| `tool_trail.rs` | 工具轨迹可视化 |
| `storage.rs` | 计划持久化 |
| `workflow/` | 工作流引擎（从原 `workflow/` 迁入） |
| `queue/` | 队列与调度（从原 `queue/` 迁入） |

#### 5.6.2 工作流与队列整合说明

- **workflow/** 包含 `WorkflowEngine`, `DAGBridge`, `Trigger`, `Template`。
- **queue/** 包含 `QueueManager`, `DAGScheduler`, `SubAgentQueue`, `ConcurrencyController`。
- 二者都与**任务的调度与执行顺序**强相关，归入 `agent-planning` 可避免 `agent-runtime` 同时依赖 `agent-planning` + `agent-workflow` + `agent-queue` 三个 crate。

#### 5.6.3 关键设计

- `PlanningEngine` 依赖 `agent_core::Planner`, `Decomposer`, `RePlanner` Trait。
- `PlanExecutor` 在执行具体步骤时，通过 `agent_core::ToolExecutor` 调用 `agent-skills` 提供的工具，**但本身不直接依赖** `agent-skills` crate。
- `WorkflowEngine` 的 DAG 调度依赖 `petgraph`。

#### 5.6.4 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
petgraph = "0.6"
tokio = { version = "1.36", features = ["sync", "time"] }
sqlx = { version = "0.8", features = ["sqlite"] }
```

---

### 5.7 agent-communication（全渠道通信层）

> **路径**: `crates/agent-communication`  
> **预估规模**: ~18,000 行  
> **定位**: 全渠道消息收发、平台适配器、消息路由、Webhook 管理。

#### 5.7.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `CommunicationManager`, `MessageRouterV2` |
| `agent_channel.rs` / `user_channel.rs` | Agent ↔ 用户 通道 |
| `channel_instance_manager.rs` | 渠道实例生命周期 |
| `message_router_v2.rs` | 消息路由核心 |
| `thread.rs` | 线程管理 |
| `voice.rs` | 语音消息处理 |
| `channel/` | **30+ 平台适配器**：微信、飞书、钉钉、Slack、Telegram、Discord、WhatsApp、Matrix、Line、Google Chat 等 |
| `webhook/` | **15+ Webhook 处理器** |
| `integration/` | 第三方集成（如 Google Calendar） |

#### 5.7.2 关键设计

- 实现 `agent_core::PlatformAdapter` Trait。
- `CommunicationManager` 依赖 `agent-llm` 的 `LLMCallInterface` 处理需要 LLM 介入的消息（如自动回复生成）。
- **不直接依赖** `agent-memory`；需要历史消息时，由 `agent-runtime` 通过 `MemorySearch` 查询后注入。
- 渠道适配器可进一步按平台拆分为 **feature flag**，例如：
  ```toml
  [features]
  default = ["webchat", "slack"]
  wechat = []
  lark = ["feishu-sdk"]
  discord = []
  ```
  这样 Gateway 编译时若只需要 WebChat，可关闭 `feishu-sdk` 依赖。

#### 5.7.3 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
agent-llm = { path = "../agent-llm" }
reqwest = { version = "0.11", features = ["json", "stream", "multipart"] }
tokio-tungstenite = { version = "0.21", features = ["native-tls"] }
tokio = { version = "1.36", features = ["sync", "net"] }

[dependencies.feishu-sdk]
version = "0.1"
features = ["websocket"]
optional = true

[features]
default = ["lark", "slack", "telegram", "discord"]
lark = ["dep:feishu-sdk"]
```

---

### 5.8 agent-evolution（自动进化层）

> **路径**: `crates/agent-evolution`  
> **预估规模**: ~10,000 行  
> **定位**: Agent 的自我进化：记忆整合、技能蒸馏、提示优化、过程奖励模型。

#### 5.8.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `EvolutionScheduler` 入口 |
| `memory_nudge.rs` / `memory_quality.rs` | Phase 1：记忆自动整合、质量评分、去重 |
| `skill_distiller.rs` / `skill_lineage.rs` / `patch_engine.rs` | Phase 2：轨迹→SKILL.md 提取、版本树、diff 更新 |
| `capo.rs` | Context-Aware Prompt Optimization |
| `atropos.rs` | 异步轨迹收集框架 |
| `dapo.rs` | 熵感知策略优化 |
| `papo.rs` | 过程奖励模型优化 |
| `sandbox.rs` | 进化沙箱（安全测试新技能） |
| `scheduler.rs` | 进化任务调度 |
| `benchmark.rs` | 进化效果基准测试 |

#### 5.8.2 关键设计

- **依赖** `agent-memory`（读取执行轨迹和记忆）、`agent-skills`（注册新技能）、`agent-llm`（生成优化后的 prompt / 技能文档）。
- 由于进化是**后台异步任务**，`agent-runtime` 通过 `EvolutionScheduler` Trait（定义在 `agent-core`）触发进化，具体实现由 `agent-evolution` 提供。
- `Sandbox` 使用 `beebotos-kernel` 的 WASM 运行时测试新技能，确保安全。

#### 5.8.3 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
agent-llm = { path = "../agent-llm" }
agent-memory = { path = "../agent-memory" }
agent-skills = { path = "../agent-skills" }
beebotos-kernel = { path = "../kernel", features = ["wasm"] }
tokio = { version = "1.36", features = ["sync", "time"] }
tempfile = "3.10"
```

---

### 5.9 agent-collaboration（多 Agent 协作层）

> **路径**: `crates/agent-collaboration`  
> **预估规模**: ~12,000 行  
> **定位**: A2A 协议、Agent 发现与协商、子 Agent 生成、服务网格、去中心化身份。

#### 5.9.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `a2a/` | A2A 协议全栈：`protocol.rs`, `discovery.rs`, `negotiation.rs`, `task_manager.rs`, `transport.rs`, `acp20_protocol.rs`, `commerce.rs`, `security.rs` |
| `collaboration/` | 多 Agent 协作：`hub.rs`, `spoke.rs`, `routing.rs`, `round_table.rs` |
| `spawning/` | 子 Agent 生成：`engine.rs`, `cross_agent.rs`, `workspace.rs`, `nonblocking.rs` |
| `service_mesh/` | 服务网格：`registry.rs`, `resolver.rs`, `routing.rs`, `health.rs` |
| `did/` | 去中心化身份：`DIDResolver`, `DIDDocument`, `ChainIdentityRegistry` |

#### 5.9.2 关键设计

- A2A 传输层可使用 `libp2p`（当前 `crates/p2p`）或 WebSocket；`agent-collaboration` 通过 `agent_core::Transport` Trait 抽象，不直接耦合 `beebotos-p2p`。
- `ServiceMesh` 依赖 `did` 进行链上身份验证。
- `SpawningEngine` 生成子 Agent 时，通过 `agent-runtime` 的接口创建新实例（反向依赖通过 `agent-core` Trait 解耦）。

#### 5.9.3 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
agent-communication = { path = "../agent-communication" }
agent-skills = { path = "../agent-skills" }
beebotos-chain = { path = "../chain", default-features = false }
beebotos-p2p = { path = "../p2p" }  # 可选，未来可通过 feature flag 裁剪
tokio = { version = "1.36", features = ["sync", "net"] }
p256 = { version = "0.13", features = ["ecdsa", "pem"] }
```

---

### 5.10 agent-runtime（运行时与生命周期层）

> **路径**: `crates/agent-runtime`  
> **预估规模**: ~10,000 行  
> **定位**: Agent 的完整生命周期管理：初始化、运行、调度、会话、状态机、资源池。

#### 5.10.1 职责范围

| 子模块 | 内容 |
|--------|------|
| `mod.rs` | `AgentRuntime`, `RuntimeConfig`, `SharedResourcePool` |
| `agent.rs` / `agent_runtime_impl.rs` | 运行时主逻辑（原 `agent_impl.rs` 的**集成逻辑**部分） |
| `executor.rs` | 任务执行器 |
| `scheduler.rs` | 运行时调度器（区别于 `agent-planning` 的 DAG 调度） |
| `session_pool.rs` | 会话池管理 |
| `state_machine.rs` | `AgentState` 状态机转换 |
| `lifecycle/` | 生命周期钩子：`on_init`, `on_start`, `on_stop`, `on_error` |
| `react_framework.rs` | ReAct 框架运行时包装 |
| `signals.rs` | 信号处理（暂停、恢复、终止） |
| `session/` | 会话管理（从原 `session/` 迁入）：`unified_session.rs`, `context.rs`, `websocket.rs`, `session_persistence.rs` |
| `scheduling/` | 定时调度（从原 `scheduling/` 迁入）：`cron.rs`, `heartbeat.rs`, `webhook.rs`, `situated.rs` |
| `context/` | 上下文组装器 |
| `events/` | 运行时事件总线接入 |

#### 5.10.2 agent_impl.rs 的拆分策略

原 `agent_impl.rs`（11,325 行）是拆分中最复杂的部分，按以下策略分解：

| 内容 | 目标位置 | 说明 |
|------|----------|------|
| `Agent` 结构体定义 + `AgentBuilder` | `agent-core` | 纯数据结构 |
| `Agent::execute_unified_react()` 核心逻辑 | `agent-skills` | ReAct 执行业务 |
| `Agent::build_unified_react_prompt()` | `agent-llm` | Prompt 组装 |
| `Agent::process_task()` 任务分发 | `agent-runtime` | 运行时集成 |
| `Agent` 中各字段初始化（`new()`） | `agent-runtime` | Builder 实现 |
| `Agent` 与 Kernel / Foreign RT 的集成 | `agent-runtime` | 系统对接 |

#### 5.10.3 关键设计

- `agent-runtime` 是**唯一一个依赖所有其他 agent 子模块**的 crate，承担**依赖注入容器（DI Container）**的角色。
- `AgentRuntime` 在初始化时构造 `LLMClient`, `MemoryManager`, `SkillRegistry` 等实例，并通过 `Arc<dyn Trait>` 注入到 `Agent` 中。
- `session/` 和 `scheduling/` 并入 `agent-runtime` 是因为它们与生命周期强相关，且规模适中（各约 2,000–3,000 行），独立成 crate 价值不高。

#### 5.10.4 外部依赖

```toml
[dependencies]
agent-core = { path = "../agent-core" }
agent-llm = { path = "../agent-llm" }
agent-memory = { path = "../agent-memory" }
agent-security = { path = "../agent-security" }
agent-skills = { path = "../agent-skills" }
agent-planning = { path = "../agent-planning" }
agent-communication = { path = "../agent-communication" }
agent-evolution = { path = "../agent-evolution" }
agent-collaboration = { path = "../agent-collaboration" }

beebotos-kernel = { path = "../kernel", features = ["wasm"] }
beebotos-foreign-rt = { path = "../foreign-rt" }
beebotos-message-bus = { path = "../message-bus", features = ["memory"] }

tokio = { version = "1.36", features = ["full"] }
tracing = "0.1"
```

---

### 5.11 beebotos-agents（Facade 兼容层）

> **路径**: `crates/agents`（保留，内容替换为 Facade）  
> **预估规模**: ~2,000 行  
> **定位**: **向后兼容门面**，对上层应用（`apps/gateway`, `apps/cli` 等）暴露与当前完全一致的 API。

#### 5.11.1 职责范围

- `lib.rs`：通过 `pub use agent_core::*;`, `pub use agent_runtime::*;` 等**重新导出**所有子模块的公开类型。
- 保留 `Agent`, `AgentBuilder`, `AgentConfig` 的**类型别名或薄包装**，确保现有代码无需修改。
- 提供**统一初始化函数**：
  ```rust
  pub async fn initialize_default_runtime() -> Arc<AgentRuntime> { ... }
  ```

#### 5.11.2 兼容性策略

| 当前 API | Facade 映射 |
|----------|-------------|
| `use beebotos_agents::Agent;` | `pub use agent_runtime::Agent;` |
| `use beebotos_agents::memory::MemoryEntry;` | `pub use agent_memory::MemoryEntry;` |
| `use beebotos_agents::a2a::A2AClient;` | `pub use agent_collaboration::a2a::A2AClient;` |
| `use beebotos_agents::skills::SkillRegistry;` | `pub use agent_skills::SkillRegistry;` |

> **Phase 1（迁移期）**: 所有现有 `use beebotos_agents::...` 继续工作。  
> **Phase 2（稳定期）**: 新代码鼓励直接使用 `agent_core`, `agent_runtime` 等子 crate。  
> **Phase 3（可选）**: 若 Facade 维护成本高，可在 major version 升级时废弃，提供 `cargo fix` 自动迁移脚本。

#### 5.11.3 Cargo.toml

```toml
[package]
name = "beebotos-agents"
version = "1.0.0"
edition = "2021"
description = "BeeBotOS Agents - Unified facade for all agent sub-crates"

[dependencies]
agent-core = { path = "../agent-core" }
agent-llm = { path = "../agent-llm" }
agent-memory = { path = "../agent-memory" }
agent-security = { path = "../agent-security" }
agent-skills = { path = "../agent-skills" }
agent-planning = { path = "../agent-planning" }
agent-communication = { path = "../agent-communication" }
agent-evolution = { path = "../agent-evolution" }
agent-collaboration = { path = "../agent-collaboration" }
agent-runtime = { path = "../agent-runtime" }
```

---

## 6. 模块间依赖关系（DAG）

```
                         ┌─────────────────┐
                         │ beebotos-agents │  (Facade, L5)
                         │   (L5 Facade)   │
                         └────────┬────────┘
                                  │
                         ┌────────▼────────┐
                         │  agent-runtime  │  (L4)
                         └────────┬────────┘
                                  │
        ┌─────────────┬───────────┼───────────┬─────────────┐
        │             │           │           │             │
┌───────▼─────┐ ┌─────▼─────┐ ┌──▼───┐ ┌────▼────┐ ┌──────▼──────┐
│agent-collab │ │agent-evol │ │agent-│ │ agent-  │ │ agent-comm  │
│   (L3)      │ │  (L2)     │ │plan  │ │ skills  │ │   (L2)      │
└───────┬─────┘ └─────┬─────┘ └─┬────┘ └────┬────┘ └──────┬──────┘
        │             │         │           │             │
        │             │         │           │             │
        └─────────────┴────┬────┴───────────┴─────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
       ┌──────▼─────┐ ┌───▼────┐ ┌────▼─────┐
       │ agent-llm  │ │agent-mem│ │agent-sec │
       │   (L1)     │ │  (L1)   │ │  (L1)   │
       └──────┬─────┘ └────┬────┘ └────┬─────┘
              │            │           │
              └────────────┼───────────┘
                           │
                    ┌──────▼──────┐
                    │  agent-core │  (L0)
                    │   (L0)      │
                    └─────────────┘
```

### 6.1 依赖矩阵

| 模块 | 依赖的 Agent 子模块 | 被哪些 Agent 子模块依赖 |
|------|---------------------|------------------------|
| `agent-core` | 无 | 全部 |
| `agent-llm` | `agent-core` | `agent-memory`, `agent-skills`, `agent-communication`, `agent-evolution`, `agent-runtime` |
| `agent-memory` | `agent-core`, `agent-llm` | `agent-evolution`, `agent-runtime` |
| `agent-security` | `agent-core` | `agent-runtime` |
| `agent-skills` | `agent-core`, `agent-llm` | `agent-planning`, `agent-evolution`, `agent-collaboration`, `agent-runtime` |
| `agent-planning` | `agent-core` | `agent-runtime` |
| `agent-communication` | `agent-core`, `agent-llm` | `agent-collaboration`, `agent-runtime` |
| `agent-evolution` | `agent-core`, `agent-llm`, `agent-memory`, `agent-skills` | `agent-runtime` |
| `agent-collaboration` | `agent-core`, `agent-communication`, `agent-skills` | `agent-runtime` |
| `agent-runtime` | 全部 | `beebotos-agents` |
| `beebotos-agents` | 全部 | `apps/gateway`, `apps/cli` 等 |

> **验证**: 矩阵中无循环依赖。`agent-planning` 不直接依赖 `agent-skills`，只通过 `agent_core::ToolExecutor` Trait 间接使用，符合解耦原则。

---

## 7. 核心 Trait 解耦设计详述

拆分成功的关键在于 `agent-core` 中 Trait 设计的完备性。以下是需要提取到 `agent-core` 的核心接口：

### 7.1 MemorySearch（记忆访问）

```rust
#[async_trait]
pub trait MemorySearch: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> AgentResult<Vec<MemoryEntry>>;
    async fn search_with_filter(&self, query: &str, filter: MemoryFilter, limit: usize) -> AgentResult<Vec<MemoryEntry>>;
    async fn store(&self, entry: MemoryEntry) -> AgentResult<Uuid>;
    async fn delete(&self, id: Uuid) -> AgentResult<bool>;
    async fn consolidate(&self) -> AgentResult<()>;
}
```

**实现方**: `agent-memory::MemoryManager`  
**使用方**: `agent-skills`（ReAct 上下文注入）、`agent-evolution`（轨迹分析）、`agent-runtime`（会话恢复）

### 7.2 LLMCallInterface（LLM 调用）

```rust
#[async_trait]
pub trait LLMCallInterface: Send + Sync {
    async fn call_llm(&self, request: CompletionRequest) -> AgentResult<CompletionResponse>;
    async fn call_llm_stream(&self, request: CompletionRequest) -> AgentResult<BoxStream<'static, String>>;
    async fn call_llm_with_tools(&self, request: CompletionRequest, tools: Vec<ToolDefinition>) -> AgentResult<ToolAwareResponse>;
    fn supports_native_tools(&self) -> bool;
    fn estimate_tokens(&self, messages: &[Message]) -> u32;
}
```

**实现方**: `agent-llm::LLMClient`  
**使用方**: `agent-skills`, `agent-communication`, `agent-evolution`, `agent-planning`

### 7.3 SkillResolver（技能解析）

```rust
pub trait SkillResolver: Send + Sync {
    fn resolve(&self, skill_id: &str) -> Option<SkillMetadata>;
    fn list_skills(&self) -> Vec<SkillMetadata>;
    fn list_tools(&self) -> Vec<ToolDefinition>;
    fn get_skill_documentation(&self, skill_id: &str, level: DocLevel) -> Option<String>;
}
```

**实现方**: `agent-skills::SkillRegistry`  
**使用方**: `agent-llm`（PromptBuilder L1/L2/L3 组装）、`agent-runtime`

### 7.4 SecurityPolicy（安全策略）

```rust
#[async_trait]
pub trait SecurityPolicy: Send + Sync {
    async fn check_permission(&self, ctx: &PermissionContext, action: &str) -> PermissionResult;
    async fn request_approval(&self, request: ApprovalRequest) -> AgentResult<ApprovalDecision>;
    fn get_isolation_level(&self, session_id: &SessionId) -> IsolationLevel;
}
```

**实现方**: `agent-security::SecurityManager`  
**使用方**: `agent-runtime`（任务执行前后）、`agent-skills`（高危工具调用前）

### 7.5 Transport（A2A 传输抽象）

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, target: &DID, message: A2AMessage) -> AgentResult<()>;
    async fn receive(&self) -> AgentResult<A2AMessage>;
    async fn broadcast(&self, capability: &str, message: A2AMessage) -> AgentResult<()>;
}
```

**实现方**: `agent-collaboration::a2a::transport`（WebSocket / libp2p 适配器）  
**使用方**: `agent-collaboration::A2AClient`

---

## 8. 目录结构映射

### 8.1 迁移前后对比

**迁移前（单体）**:

```
crates/agents/
├── Cargo.toml
└── src/
    ├── lib.rs                    # 40+ mod 声明
    ├── agent_impl.rs             # 11,325 行，核心 Agent 结构体与逻辑
    ├── types.rs
    ├── task.rs
    ├── error.rs
    ├── a2a/                      # A2A 协议
    ├── browser/                  # 浏览器自动化
    ├── collaboration/            # 多 Agent 协作
    ├── communication/            # 全渠道通信（最大子模块）
    ├── context/
    ├── device/                   # 移动设备
    ├── did/
    ├── evolution/                # 自动进化
    ├── events/
    ├── intent/
    ├── kernel_integration.rs
    ├── llm/                      # LLM 集成
    ├── mcp/                      # MCP 协议
    ├── media/
    ├── memory/                   # 记忆系统
    ├── models/
    ├── planning/                 # 规划系统
    ├── prompt/
    ├── queue/
    ├── runtime/                  # Agent 运行时
    ├── scheduling/
    ├── security/                 # 安全
    ├── service_mesh/
    ├── session/                  # 会话管理
    ├── skills/                   # 技能系统
    ├── skill_matching/
    ├── spawning/                 # 子 Agent 生成
    ├── state_manager/
    ├── wallet/
    └── workflow/                 # 工作流
```

**迁移后（多 crate）**:

```
crates/
├── agent-core/                 # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── types.rs            # (从 agents/src/types.rs 迁移)
│       ├── task.rs             # (从 agents/src/task.rs 迁移)
│       ├── error.rs            # (从 agents/src/error.rs 迁移)
│       ├── config.rs           # AgentConfig, MemoryConfig...
│       ├── traits.rs           # 所有核心 Trait 定义
│       └── builder.rs          # AgentBuilder 声明
│
├── agent-llm/                  # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── client.rs           # (从 agents/src/llm/client.rs)
│       ├── adapter.rs
│       ├── failover.rs
│       ├── prompt/
│       │   ├── builder.rs      # (从 agents/src/prompt/builder.rs)
│       │   └── cache.rs
│       ├── models/
│       │   ├── mod.rs
│       │   ├── router.rs       # (从 agents/src/models/router.rs)
│       │   └── cost.rs
│       └── providers/
│           ├── mod.rs
│           ├── openai.rs
│           ├── claude.rs
│           └── ...
│
├── agent-memory/               # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # (从 agents/src/memory/mod.rs)
│       ├── search.rs           # (从 agents/src/memory/search.rs)
│       ├── hybrid_search.rs
│       ├── embedding.rs
│       ├── local.rs
│       ├── markdown_storage.rs
│       └── ...
│
├── agent-security/             # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # (从 agents/src/security/mod.rs)
│       ├── approval.rs
│       ├── permission_system.rs
│       └── session_isolation.rs
│
├── agent-skills/               # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # (从 agents/src/skills/mod.rs)
│       ├── registry.rs         # SkillRegistry + skill_matching 合并
│       ├── unified_react_executor.rs
│       ├── react_executor.rs
│       ├── composition/
│       ├── builtin_loader.rs
│       └── mcp/                # (从 agents/src/mcp/ 整体迁入)
│           ├── mod.rs
│           ├── client.rs
│           ├── server.rs
│           └── ...
│
├── agent-planning/             # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # (从 agents/src/planning/mod.rs)
│       ├── plan.rs
│       ├── decomposer.rs
│       ├── engine.rs
│       ├── executor.rs
│       ├── replanner.rs
│       ├── workflow/           # (从 agents/src/workflow/ 迁入)
│       └── queue/              # (从 agents/src/queue/ 迁入)
│
├── agent-communication/        # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # (从 agents/src/communication/mod.rs)
│       ├── message_router_v2.rs
│       ├── channel/            # 30+ 平台适配器
│       └── webhook/            # 15+ Webhook 处理器
│
├── agent-evolution/            # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # (从 agents/src/evolution/mod.rs)
│       ├── skill_distiller.rs
│       ├── patch_engine.rs
│       ├── capo.rs
│       ├── atropos.rs
│       └── sandbox.rs
│
├── agent-collaboration/        # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── a2a/                # (从 agents/src/a2a/ 迁入)
│       ├── collaboration/      # (从 agents/src/collaboration/ 迁入)
│       ├── spawning/           # (从 agents/src/spawning/ 迁入)
│       ├── service_mesh/       # (从 agents/src/service_mesh/ 迁入)
│       └── did/                # (从 agents/src/did/ 迁入)
│
├── agent-runtime/              # 新增
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── agent.rs            # Agent 结构体运行时包装
│       ├── agent_runtime_impl.rs # 原 agent_impl.rs 的集成逻辑
│       ├── executor.rs
│       ├── scheduler.rs
│       ├── state_machine.rs
│       ├── session_pool.rs
│       ├── session/            # (从 agents/src/session/ 迁入)
│       ├── scheduling/         # (从 agents/src/scheduling/ 迁入)
│       └── events/             # (从 agents/src/events/ 迁入)
│
└── agents/                     # 保留，改为 Facade
    ├── Cargo.toml              # 依赖全部 agent-* 子 crate
    └── src/
        └── lib.rs              # 重新导出所有公开类型
```

### 8.2 边界文件处理

存在部分文件被多个模块引用，需按以下策略处理：

| 原文件 | 处理方式 | 说明 |
|--------|----------|------|
| `agent_impl.rs` | **拆分** | 数据结构→`agent-core`；ReAct 逻辑→`agent-skills`；Prompt 构建→`agent-llm`；任务分发/初始化→`agent-runtime` |
| `kernel_integration.rs` | **移至** `agent-runtime/src/` | 运行时与 Kernel 的集成，依赖 `Agent` 实例 |
| `context/` | **拆分** | 通用上下文定义→`agent-core`；上下文组装（注入 memory/skills）→`agent-runtime` |
| `models/` | **拆分** | `ModelConfig`→`agent-core`；模型路由逻辑→`agent-llm` |
| `media/` | **待定** | 若规模小（<1,500 行），并入 `agent-communication`；若大，独立为 `agent-media` |
| `intent/` / `skill_matching/` | **合并入** `agent-skills` | 意图分析与技能选择属于技能系统的前置环节 |
| `browser/` / `device/` / `wallet/` | **决策待定** | 方案 A：独立为 `agent-peripherals`（~6,000 行）；方案 B：分别独立为 `agent-browser`, `agent-device`, `agent-wallet`。推荐 **方案 A**，因为它们都是**外围能力**，且被 `agent-runtime` 统一调用。 |

---

## 9. 迁移路线图

### Phase 0：准备工作（1 周）

1. **创建 `agent-core`**：提取 `types.rs`, `task.rs`, `error.rs` 和核心 Trait。
2. **在 `agents` 内部建立临时模块边界**：
   ```rust
   // crates/agents/src/lib.rs 新增内部模块隔离
   pub mod agent_core_internal { ... }
   ```
   验证 Trait 抽象是否足够，不实际拆分 crate。
3. **CI 准备**：为每个新 crate 预留编译、测试、clippy 流水线模板。

### Phase 1：基础层拆分（1–2 周）

按**从底向上**顺序，先拆分无依赖或依赖最少的 crate：

1. `agent-core` → 独立编译通过，所有原模块 `use crate::types::AgentId` 改为 `use agent_core::AgentId`。
2. `agent-llm` → 将 `llm/` + `prompt/` + `models/` 迁出，依赖 `agent-core`。
3. `agent-memory` → 将 `memory/` 迁出，依赖 `agent-core` + `agent-llm`。
4. `agent-security` → 将 `security/` 迁出，依赖 `agent-core`。

**验证点**：每个新 crate 的单元测试全部通过；`agents` 的集成测试暂时通过 `path` 依赖继续使用内部模块。

### Phase 2：能力层拆分（2–3 周）

1. `agent-skills` → 迁出 `skills/` + `mcp/` + `intent/` + `skill_matching/`，依赖 `agent-core` + `agent-llm`。
2. `agent-planning` → 迁出 `planning/` + `workflow/` + `queue/`，依赖 `agent-core` + `agent-skills`（通过 Trait）。
3. `agent-communication` → 迁出 `communication/`，依赖 `agent-core` + `agent-llm`。
4. `agent-evolution` → 迁出 `evolution/`，依赖 `agent-core` + `agent-memory` + `agent-skills` + `agent-llm`。

**验证点**：`apps/gateway` 能够通过更新后的 `beebotos-agents` 正常编译运行。

### Phase 3：协作与运行时拆分（2 周）

1. `agent-collaboration` → 迁出 `a2a/` + `collaboration/` + `spawning/` + `service_mesh/` + `did/`，依赖 `agent-core` + `agent-communication` + `agent-skills`。
2. `agent-runtime` → 迁出 `runtime/` + `session/` + `scheduling/` + `events/` + `agent_impl.rs` 的集成逻辑，依赖所有已拆分 crate。
3. **处理外围模块**：`browser/`, `device/`, `wallet/` 视规模决定并入 `agent-runtime` 或独立为 `agent-peripherals`。

### Phase 4：Facade 与兼容性（1 周）

1. 将 `crates/agents` 清空，改为 **纯 Facade crate**：
   - `lib.rs` 中全部改为 `pub use agent_*::...;`
   - 保留 `Agent`, `AgentBuilder` 的薄包装（若类型签名变化）。
2. **全量回归测试**：运行 `cargo test --workspace`。
3. **更新文档**：`docs/architecture/04-agent-runtime.md` 等架构文档同步更新。

### Phase 5：优化与清理（持续）

1. **feature flag 精细化**：为 `agent-communication` 的渠道适配器添加独立 feature，减少编译依赖。
2. **依赖审计**：移除 `agents` Facade 中未被上层实际使用的 re-export。
3. **CI 并行化**：利用多 crate 结构实现并行编译、并行测试。

---

## 10. Cargo.toml Workspace 更新

根目录 `Cargo.toml` 更新后的 `[workspace]` 段：

```toml
[workspace]
members = [
    # 基础层
    "crates/core",
    "crates/kernel",
    "crates/chain",
    "crates/p2p",
    "crates/crypto",
    "crates/message-bus",
    "crates/gateway-lib",
    "crates/foreign-rt",
    "crates/sdk",
    "crates/telemetry",
    "crates/update-client",

    # Agent 子模块（新增）
    "crates/agent-core",
    "crates/agent-llm",
    "crates/agent-memory",
    "crates/agent-security",
    "crates/agent-skills",
    "crates/agent-planning",
    "crates/agent-communication",
    "crates/agent-evolution",
    "crates/agent-collaboration",
    "crates/agent-runtime",
    # "crates/agent-peripherals",  # 可选：browser + device + wallet

    # Agent Facade（保留原路径）
    "crates/agents",

    # 应用层
    "apps/gateway",
    "apps/web",
    "apps/cli",
    "apps/beehub",
    "beeweb",
]
```

---

## 11. 风险与应对措施

| 风险 | 影响 | 可能性 | 应对措施 |
|------|------|--------|----------|
| **Trait 设计不完善** | 拆分后模块间无法通过 Trait 交互，被迫引入循环依赖或打破分层 | 中 | Phase 0 预留 1 周做 Trait 抽象验证；若发现遗漏，允许在 `agent-core` 中追加 Trait，但严禁追加具体实现 |
| **`agent_impl.rs` 拆分困难** | 该文件 11,325 行，高度内聚，拆分时容易引入运行时 Bug | 高 | 采用**复制-修改-删除**策略：先在目标 crate 复制代码并适配，验证通过后再从原位置删除；保留 Git 历史 |
| **现有测试失效** | 集成测试分布在 `crates/agents/tests/`，拆分后依赖路径变化 | 中 | 集成测试随被测主逻辑迁移到对应 crate；`beebotos-agents` Facade 保留全量集成测试作为最终验收 |
| **编译时间未明显改善** | 若 `agent-runtime` 仍依赖全部子模块，全量编译时间可能不变 | 低 | 目标是**增量编译**改善；日常开发修改 `communication/channel/wechat_channel.rs` 时，只需编译 `agent-communication`，无需编译 `agent-evolution` |
| **API 兼容性破坏** | Facade 无法 100% 复刻原 API，导致 `apps/gateway` 编译失败 | 低 | 每次 Phase 结束后立即运行 `cargo check -p gateway`；若发现 API 差异，在 Facade 层添加兼容包装 |
| **版本管理复杂化** | 10+ crate 的版本号需要同步升级 | 低 | 使用 Workspace 统一的 `version = "1.0.0"`；发布时统一 bump；依赖使用 `path` + 相同版本号 |
| **文档/示例碎片化** | 开发者不清楚该去哪个 crate 找文档 | 低 | 在 `docs/agents/` 下维护 `MODULE_GUIDE.md`，说明各 crate 职责；在 `agent-core/src/lib.rs` 添加模块导航注释 |

---

## 12. 预期收益

| 指标 | 拆分前 | 拆分后（预期） |
|------|--------|----------------|
| **全量编译时间** | ~3–5 min | ~3–5 min（不变） |
| **增量编译（修改通信渠道）** | ~2–3 min | ~15–30 s |
| **单 crate 代码量** | 97,709 行 | 最大 18,000 行 |
| **单 crate 外部依赖** | 60+ | 最大 15+ |
| **单元测试隔离度** | 低（必须拉全依赖） | 高（各 crate 独立测试） |
| **新开发者上手时间** | 2–3 天 | < 半天（只需关注一个 crate） |
| **可选编译能力** | 无 | 支持（如关闭 `feishu-sdk`） |

---

## 13. 附录

### 13.1 术语表

| 术语 | 说明 |
|------|------|
| **Facade** | 门面模式，此处指 `beebotos-agents` crate 作为统一出口，向后兼容旧 API |
| **Trait 解耦** | 模块 A 不直接依赖模块 B 的具体类型，而是依赖 `agent-core` 中定义的 Trait |
| **ReAct** | Reasoning + Acting 框架，Agent 的思考-行动-观察循环 |
| **MCP** | Model Context Protocol，模型上下文协议 |
| **A2A** | Agent-to-Agent 协议 |
| **CAPO/DAPO/PAPO** | 进化系统中的提示优化/策略优化/过程奖励优化算法 |

### 13.2 参考文档

- `docs/architecture/04-agent-runtime.md` — Agent 运行时架构
- `docs/agents/skill-composition-workflow-design.md` — 技能组合设计
- `docs/agents/skill-execution-redesign.md` — 技能执行重设计
- `crates/agents/src/agent_impl.rs` — 当前统一 ReAct 入口实现
