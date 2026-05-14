# BeeBotOS

Web4.0 自主智能体操作系统。项目以 Rust workspace 为主体，结合 WASM 沙箱、Capability 权限模型、A2A 通信、P2P 网络和链上锚定，为 AI Agent 提供运行、协作、治理和支付基础设施。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![Solidity](https://img.shields.io/badge/solidity-%5E0.8.0-blue.svg)](https://soliditylang.org)

## 核心能力

| 能力 | 实现 |
|------|------|
| Agent 隔离 | WASM 沙箱、Capability 权限、Gas 计量 |
| 任务调度 | 内核级调度器、资源限制、任务监控 |
| Agent 协作 | A2A 协议、MCP、Workflow、消息总线 |
| 认知模型 | NEAT、PAD、OCEAN、记忆与推理模块 |
| 去中心化 | libp2p、多链交互、DAO 治理、链上注册 |
| 对外服务 | Gateway API、Web 控制台、CLI、BeeHub、BeeWeb |

## 架构

```text
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

### Agent 任务入口

`LlmChat` 类型任务统一进入 `execute_unified_react()`：

```text
process_task()
  -> process_task_v2()
  -> execute_unified_react(task)
     -> build_unified_react_prompt()
     -> UnifiedReActExecutor::execute()
```

当前策略：

- 不再做前置 Intent 分类。
- L1 skills 索引和 L2 skills 摘要始终注入。
- L3 完整 skill 文档按需注入。
- LLM 在 ReAct 循环中决定调用工具或直接回答。

相关文件：

| 文件 | 职责 |
|------|------|
| `crates/agents/src/agent_impl.rs` | 统一 ReAct 入口、Prompt 组装、Skills/Tools 构建 |
| `crates/agents/src/prompt/builder.rs` | `PromptBuilder::build_unified_react()` |
| `crates/agents/src/skills/unified_react_executor.rs` | ReAct 执行引擎、L3 动态注入 |

## 项目结构

```text
beebotos/
├── Cargo.toml
├── Makefile
├── justfile
├── crates/
│   ├── core
│   ├── kernel
│   ├── brain
│   ├── agents
│   ├── chain
│   ├── crypto
│   ├── p2p
│   ├── sdk
│   ├── telemetry
│   ├── gateway-lib
│   ├── message-bus
│   └── update-client
├── apps/
│   ├── gateway
│   ├── web
│   ├── cli
│   └── beehub
├── beeweb/
├── contracts/
│   ├── src
│   ├── test
│   └── foundry.toml
├── config/
├── docs/
├── examples/
├── proto/
├── skills/
└── tests/
```

## 环境要求

- macOS 26
- Rust 1.75+
- Node.js 22
- Foundry
- Docker 24+，可选

## 快速开始

```bash
cargo fetch
cargo build --workspace
cargo test --workspace --all-features
```

推荐使用 `just`：

```bash
just build
just test
just check
```

也可以使用 `make`：

```bash
make debug
make test
make check
```

## 运行服务

### Gateway

```bash
cargo run -p beebotos-gateway
```

默认配置在 `config/beebotos.toml`，Gateway API 固定端口为 `8000`。

### Web

```bash
cargo run -p beebotos-web --features server --bin web-server
```

Web 服务配置在 `config/web-server.toml`，默认端口为 `8090`，后端代理地址为 `http://localhost:8000`。

### CLI

```bash
cargo run -p beebotos-cli -- --help
```

安装到本地：

```bash
cargo install --path apps/cli --force
```

### BeeHub

```bash
cargo run -p beebotos-beehub
```

### BeeWeb

```bash
cargo run -p beebotos-beeweb
```

## 合约开发

```bash
cd contracts
forge build
forge test
forge fmt
```

合约源码位于 `contracts/src`，测试位于 `contracts/test`。

## 常用命令

| 任务 | 命令 |
|------|------|
| Release 构建 | `cargo build --workspace --release` |
| Debug 构建 | `cargo build --workspace` |
| 全量测试 | `cargo test --workspace --all-features` |
| 单元测试 | `cargo test --workspace --lib` |
| 集成测试 | `cargo test --workspace --test '*'` |
| 格式化 | `cargo fmt --all` |
| 格式检查 | `cargo fmt --all -- --check` |
| Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| 文档 | `cargo doc --workspace --no-deps` |
| 合约测试 | `cd contracts && forge test` |
| 安全审计 | `cargo audit` |

## 配置

敏感配置从环境变量读取，可参考 `.env.example`：

```bash
BEE__JWT__SECRET=...
BEE__MODELS__KIMI__API_KEY=...
BEE__MODELS__ZHIPU__API_KEY=...
BEE__CHANNELS__LARK__APP_SECRET=...
```

主要配置文件：

| 文件 | 说明 |
|------|------|
| `config/beebotos.toml` | Gateway、模型、数据库、通道、MCP 配置 |
| `config/web-server.toml` | Web server、静态资源、Gateway 代理配置 |
| `contracts/foundry.toml` | Solidity 合约构建与测试配置 |

## 架构约束

`crates/agents` 禁止直接依赖 Web 框架。HTTP 相关能力必须放在 `apps/gateway` 或 `crates/gateway-lib`。

禁止在 `crates/agents` 中引入：

- `axum`
- `actix-web`
- `rocket`
- `warp`
- `tide`
- `salvo`

## 文档入口

| 文档 | 说明 |
|------|------|
| `docs/getting-started.md` | 入门说明 |
| `docs/architecture/OVERVIEW.md` | 架构总览 |
| `docs/architecture/01-overview.md` | 架构概述 |
| `docs/api/README.md` | API 文档入口 |
| `docs/specs/A2A_PROTOCOL.md` | A2A 协议 |
| `docs/specs/CAPABILITY_SYSTEM.md` | Capability 系统 |
| `docs/evolution/LLM-trade/remove-v2-intent-unified-react-v1.md` | 统一 ReAct 方案 |
| `CONTRIBUTING.md` | 贡献指南 |
| `ROADMAP.md` | 路线图 |

## 许可证

本项目采用 MIT 许可证，详见 `LICENSE`。
