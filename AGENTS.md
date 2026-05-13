# BeeBotOS 项目指南

> 本文档为 AI 编程助手提供项目背景、构建流程、代码规范和开发指南。

## 项目概述

**BeeBotOS** 是一个 Web4.0 自主智能体操作系统，采用 5 层模块化架构，专为 AI Agent 设计的去中心化操作系统。

### 核心技术特点

- **WASM 沙箱 + Capability 权限模型**：Agent 之间安全隔离
- **内核级调度器 + Gas 计量系统**：精确控制资源使用
- **分布式 P2P 网络 + 区块链锚定**：避免单点故障
- **A2A 协议**：标准化 Agent-to-Agent 通信
- **多链支持**：Ethereum、BSC、Polygon、Solana

### 系统架构（5层模型）

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 4: Applications                                           │
│  DeFAI · Social AI · DAO Governance · Game AI                   │
├─────────────────────────────────────────────────────────────────┤
│ Layer 3: Agent Layer                                            │
│  A2A Protocol · MCP · Browser Automation · Workflow Engine      │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: Social Brain                                           │
│  NEAT · PAD · OCEAN · Memory System · Reasoning Engine          │
├─────────────────────────────────────────────────────────────────┤
│ Layer 1: Kernel                                                 │
│  Scheduler · Security · WASM Runtime · Syscalls · IPC           │
├─────────────────────────────────────────────────────────────────┤
│ Layer 0: Blockchain                                             │
│  Ethereum · BSC · Polygon · Solana · Cross-Chain Bridge         │
└─────────────────────────────────────────────────────────────────┘
```

## 技术栈与项目结构

### 技术栈

| 组件 | 技术选型 | 用途 |
|------|---------|------|
| 编程语言 | Rust 1.75+ | 核心系统开发 |
| 智能合约 | Solidity ^0.8.0 | 链上逻辑 |
| 异步运行时 | Tokio | 高并发任务调度 |
| Web 框架 | Axum | HTTP API 服务 |
| WASM 引擎 | Wasmtime | Agent 沙箱执行 |
| P2P 网络 | libp2p | 去中心化通信 |
| 区块链 | alloy, ethers-rs | 链上交互 |
| 数据库 | SQLite, PostgreSQL | 数据持久化 |
| 合约开发 | Foundry | Solidity 开发测试 |

### 项目目录结构

```
beebotos/
├── Cargo.toml              # Workspace 根配置
├── Makefile / justfile     # 构建脚本
├── crates/                 # Rust 核心库（11个 crates）
│   ├── core/              # 核心类型和工具
│   ├── kernel/            # 系统内核（调度/安全/WASM）
│   ├── brain/             # 神经网络和认知模型
│   ├── agents/            # Agent 运行时和 A2A 协议
│   ├── chain/             # 区块链集成
│   ├── crypto/            # 加密工具
│   ├── p2p/               # P2P 网络
│   ├── sdk/               # 开发工具包
│   ├── telemetry/         # 遥测和日志
│   ├── gateway-lib/       # 网关共享库
│   └── message-bus/       # 消息总线
├── apps/                   # 应用程序
│   ├── gateway/           # API 网关服务
│   ├── web/               # Web 前端
│   ├── cli/               # 命令行工具
│   └── beehub/            # Hub 服务
├── contracts/              # Solidity 智能合约
│   ├── src/               # 合约源码
│   ├── test/              # 合约测试
│   └── foundry.toml       # Foundry 配置
├── tests/                  # 集成测试和端到端测试
│   ├── unit/              # 单元测试
│   ├── integration/       # 集成测试
│   └── e2e/               # 端到端测试
├── config/                 # 配置文件
├── docker/                 # Docker 配置
├── docs/                   # 文档
├── skills/                 # Skill 定义
└── proto/                  # Protocol Buffer 定义
```

## Agent 任务处理架构（2026-05-13 更新）

### 统一 ReAct 入口

所有 `LlmChat` 类型任务统一进入 `execute_unified_react()`，不再进行前置 Intent 分类。

```
User Input
    ↓
process_task() ──→ process_task_v2() ──→ execute_unified_react(task)
                                              ↓
                                    build_unified_react_prompt()
                                    - Persona (SOUL.md)
                                    - Memories (all, no filtering)
                                    - Skills L1 + L2 (all, hierarchical)
                                    - Tools (all builtin + skill_call + parallel_delegate)
                                    - ReAct rules
                                              ↓
                                    UnifiedReActExecutor::execute()
                                    - LLM decides: call_tool or final_answer
                                    - Max 30 rounds
                                    - L3 injected on-demand
```

### Skills 注入层次

- **L1（索引）**：所有 skills 的 id + name + one-liner，始终注入
- **L2（摘要）**：所有 skills 的 summary，始终注入
- **L3（完整文档）**：默认不注入；LLM 在 thought 中请求时自动追加

### 关键文件

| 文件 | 职责 |
|------|------|
| `crates/agents/src/agent_impl.rs` | 统一 ReAct 入口、Prompt 组装、Skills/Tools 构建 |
| `crates/agents/src/prompt/builder.rs` | PromptBuilder：`build_unified_react()` |
| `crates/agents/src/skills/unified_react_executor.rs` | ReAct 执行引擎、L3 动态注入 |

### 已废弃的旧组件

以下组件保留源码但不再参与主路由：
- `IntentEngine`（启发式意图分类）
- `LLMIntentAnalyzer V2`（LLM 意图分析）
- `SkillSelector`（技能选择器）

详细技术方案见：`docs/evolution/LLM-trade/remove-v2-intent-unified-react-v1.md`

---

## 构建与测试命令

### 环境要求

- **Rust**: >= 1.75.0
- **Node.js**: >= 18.0 (前端可选)
- **Foundry**: Solidity 合约开发
- **Docker**: >= 24.0 (可选)

### 常用构建命令

```bash
# 构建整个 Workspace（Release 模式）
cargo build --workspace --release

# 构建 Debug 模式
cargo build --workspace

# 构建特定 crate
cargo build -p beebotos-kernel

# 构建特定应用
cargo build -p beebotos-gateway
```

### 使用 Makefile

```bash
make build          # Release 构建
make debug          # Debug 构建
make test           # 运行所有测试
make test-unit      # 仅单元测试
make test-integration # 仅集成测试
make bench          # 性能基准测试
make fmt            # 格式化代码
make lint           # 运行 Clippy
make check          # 完整检查（fmt + lint + test）
make doc            # 生成文档
make clean          # 清理构建产物
make install        # 安装 CLI 到本地
make docker         # 构建 Docker 镜像
```

### 使用 Just（推荐）

```bash
just build          # 构建
just test           # 测试
just check          # 完整检查
just dev            # 开发模式（watch）
just docker-up      # 启动 Docker 环境
just contract-test  # 测试智能合约
```

### 测试命令

```bash
# 运行所有测试
cargo test --workspace --all-features

# 运行特定 crate 的测试
cargo test -p beebotos-kernel

# 运行单元测试（仅 lib）
cargo test --workspace --lib

# 运行集成测试
cargo test --workspace --test '*'

# 端到端测试
cargo test --test e2e

# 性能测试
cargo bench --workspace

# 代码覆盖率
cargo tarpaulin --workspace --out Html
```

### 智能合约相关

```bash
# 构建合约
cd contracts && forge build

# 测试合约
cd contracts && forge test

# 格式化合约代码
cd contracts && forge fmt

# 部署合约（测试网）
./tools/deploy_contracts.py --network testnet
```

## 代码风格规范

### Rust 代码规范

项目遵循 [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) 和自定义规范：

#### 格式化配置（rustfmt.toml）

```toml
max_width = 100              # 每行最大 100 字符
tab_spaces = 4               # 4 空格缩进
edition = "2021"             # Rust 2021 版本
imports_granularity = "Module"  # 导入粒度
group_imports = "StdExternalCrate"  # 分组导入
format_code_in_doc_comments = true  # 格式化文档注释中的代码
```

执行格式化：
```bash
cargo fmt --all              # 格式化所有代码
cargo fmt --all -- --check   # 检查格式化（CI 使用）
```

#### Clippy 配置（clippy.toml）

```toml
cognitive-complexity-threshold = 30    # 认知复杂度阈值
too-many-arguments-threshold = 7       # 参数数量阈值
type-complexity-threshold = 300        # 类型复杂度阈值
too-many-lines-threshold = 100         # 函数行数阈值
```

运行静态分析：
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

#### 命名规范

| 项目 | 规范 | 示例 |
|------|------|------|
| 类型 | PascalCase | `AgentRuntime`, `TaskScheduler` |
| 函数/方法 | snake_case | `spawn_agent`, `process_task` |
| 变量 | snake_case | `task_count`, `agent_id` |
| 常量 | SCREAMING_SNAKE_CASE | `MAX_AGENTS`, `TIME_SLICE_MS` |
| 模块 | snake_case | `kernel`, `agent_runtime` |

#### 文档注释规范

```rust
/// 创建一个具有指定优先级的新任务。
///
/// # 参数
///
/// * `name` - 任务的名称，用于日志和调试
/// * `priority` - 任务的优先级，决定调度顺序
///
/// # 返回值
///
/// 返回新创建任务的 TaskId，可用于后续操作
///
/// # 示例
///
/// ```
/// use beebotos::kernel::{Task, Priority};
///
/// let task = Task::new("data_processor", Priority::High);
/// ```
pub fn new(name: impl Into<String>, priority: Priority) -> Self {
    // 实现...
}
```

### Solidity 代码规范

遵循 [Solidity Style Guide](https://docs.soliditylang.org/en/latest/style-guide.html)：

```bash
# 格式化合约代码
cd contracts && forge fmt

# 检查格式化
cd contracts && forge fmt --check
```

NatSpec 文档规范：
```solidity
/// @title Agent Registry
/// @author BeeBotOS Team
/// @notice 管理智能体的注册、更新和查询
contract AgentRegistry {
    /// @notice 注册新智能体
    /// @param agentId 智能体的唯一标识符
    /// @param owner 智能体所有者的地址
    /// @return success 注册是否成功
    function register(bytes32 agentId, address owner) 
        external 
        returns (bool success) 
    {
        // 实现...
    }
}
```

## 提交规范

### Conventional Commits

使用 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

```
<type>(<scope>): <subject>

<body>

<footer>
```

#### Type 类型

| Type | 说明 | 示例 |
|------|------|------|
| `feat` | 新功能 | `feat(kernel): add priority scheduler` |
| `fix` | Bug 修复 | `fix(contracts): prevent reentrancy` |
| `docs` | 文档更新 | `docs(api): update REST endpoints` |
| `refactor` | 代码重构 | `refactor(agents): simplify state machine` |
| `perf` | 性能优化 | `perf(memory): optimize allocator` |
| `test` | 测试相关 | `test(kernel): add scheduler tests` |
| `chore` | 构建/工具 | `chore(ci): update github actions` |

#### 提交示例

```bash
# 简单提交
git commit -m "feat(kernel): add priority scheduler"

# 详细提交
git commit -m "feat(kernel): add MLFQ priority scheduler

Implement Multi-Level Feedback Queue scheduler with 4 priority
levels (Critical, High, Normal, Low).

Closes #123"
```

## 架构边界与约束

### 重要：agents crate 架构约束

`crates/agents` **禁止**直接依赖 Web 框架（如 axum、actix-web）。所有 HTTP 相关功能应通过 `beebotos-gateway-lib` 提供。

**禁止的依赖**：
- axum
- actix-web / actix_web
- rocket
- warp
- tide
- salvo

CI 会自动检查这些约束（见 `.github/workflows/architecture-guard.yml`）。

### Crate 依赖关系

```
apps/gateway -> crates/gateway-lib -> crates/core
                      ↓
            crates/agents, kernel, chain, etc.
```

### 特性标志（Features）

常用特性标志：
- `wasm-runtime` - 启用 WASM 运行时
- `a2a-server` - 启用 A2A 协议服务端
- `sqlite` - 启用 SQLite 数据库支持
- `metrics` - 启用遥测指标收集

## 测试策略

### 测试覆盖率要求

| 模块 | 最低覆盖率 | 目标覆盖率 |
|------|-----------|-----------|
| `kernel` | 85% | 90% |
| `brain` | 80% | 85% |
| `agents` | 80% | 85% |
| `chain` | 75% | 80% |
| `contracts` | 90% | 95% |

### 测试组织

```
tests/
├── unit/              # 单元测试（与源码一起）
├── integration/       # 集成测试
│   ├── agent_integration.rs
│   ├── kernel_integration.rs
│   └── dao_integration.rs
└── e2e/               # 端到端测试
    ├── agent_lifecycle.rs
    ├── a2a_protocol.rs
    └── security.rs
```

### 编写测试示例

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scheduler_priority() {
        let mut scheduler = Scheduler::new(Config::default());
        let task = Task::new("test", Priority::High);
        let id = scheduler.submit(task).unwrap();
        assert_eq!(scheduler.get_task(id).priority(), Priority::High);
    }
    
    #[tokio::test]
    async fn test_async_operation() {
        let result = async_operation().await;
        assert!(result.is_ok());
    }
}
```

## 安全考虑

### 安全审计

```bash
# Rust 依赖安全审计
cargo audit

# 合约安全分析
cd contracts && slither .
```

### 安全编码规范

1. **避免 unwrap/expect**：使用 `?` 运算符或正确处理错误
2. **输入验证**：所有外部输入必须验证
3. **密钥管理**：使用 `secrecy` crate 保护敏感数据
4. **Capability 检查**：敏感操作前验证权限
5. **WASM 沙箱**：不可信代码必须在 WASM 沙箱中运行

### Cargo Deny 配置

项目使用 `deny.toml` 管理依赖：
- 允许的许可证：MIT, Apache-2.0, BSD, ISC
- 禁止不安全来源的 crate
- 检测重复依赖版本

运行检查：
```bash
cargo deny check
```

## 部署流程

### Docker 部署

```bash
# 构建镜像
docker build -t beebotos:v1.0.0 -f docker/Dockerfile .

# 使用 Docker Compose
docker-compose -f docker/docker-compose.yml up -d
```

### 环境变量

关键环境变量（参考 `.env.example`）：
- `DATABASE_URL` - 数据库连接字符串
- `JWT_SECRET` - JWT 签名密钥
- `RUST_LOG` - 日志级别（info/debug/trace）
- `KIMI_API_KEY` - Kimi API 密钥
- `LARK_APP_ID` / `LARK_APP_SECRET` - 飞书应用凭证

### 端口配置

| 服务 | 固定端口 | 说明 |
|------|---------|------|
| Gateway API | `8000` | API 网关服务端口 |
| Web 管理后台 | `8090` | Web 管理界面端口 |

**注意**：以上端口为固定配置，请勿修改以确保服务间正常通信。

## CI/CD 流程

项目使用 GitHub Actions：

1. **architecture-guard.yml** - 架构边界检查
2. **brain-ci.yml** - brain crate CI 流程

PR 必须通过以下检查：
- 代码格式化检查
- Clippy 静态分析
- 所有单元测试
- 架构边界检查
- 代码覆盖率检查

## 常见问题

### 构建问题

```bash
# 清理并重新构建
cargo clean && cargo build

# 更新依赖
cargo update

# 检查 lock 文件
cargo tree
```

### 测试问题

```bash
# 运行特定测试（带输出）
cargo test test_name -- --nocapture

# 忽略某些测试
cargo test --workspace -- --skip ignored_test
```

## 相关文档

- [项目 README](readme.md) - 项目简介和快速开始
- [贡献指南](CONTRIBUTING.md) - 详细贡献流程
- [架构设计文档](BeeBotOS-Design-v2-with-DAO.md) - 5层架构详细说明
- [内核层详解](BeeBotOS-Kernel-Layer-Detailed-Summary.md) - Layer 1 设计原理
- [API 文档](https://docs.beebotos.io/api) - 自动生成 API 参考

---

**注意**：修改本文件后，请确保同步更新其他相关文档。
