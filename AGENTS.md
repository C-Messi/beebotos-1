# BeeBotOS Agent Guide

## 交互与改动原则

- 对用户输出使用中文；与工具、命令、模型交互使用英文。
- 代码、注释、文档都保持精简高效，非必要不新增注释或文档。
- 只做当前需求直接相关的改动，严禁顺手影响已有功能或无关文件。

## 开发流程

OpenSpec 与 Superpowers 同时可用时，按分层职责执行，避免互相覆盖。

### 职责分层

- OpenSpec 是外层流程，负责需求边界、方案、任务拆分、状态记录和归档。
- Superpowers 是内层执行纪律，负责每个任务里的思考、TDD、调试、验证和收尾。
- 需求、设计、任务和长期规格以 OpenSpec 为正本；Superpowers 过程记录默认留在聊天、测试代码和最终汇报，不新增长期 Markdown。
- 项目约束始终优先于两者：架构边界、端口、最小改动、语言风格和验证要求不得被覆盖。

### 入口判断

- 复杂功能、架构调整、多文件改动、需求不清或用户明确提到 OpenSpec 时，先走 OpenSpec。
- 新增路由/API、数据库或配置 schema、跨 `apps` 与 `crates`、权限/任务流/用户流程变化，默认创建 OpenSpec change。
- 明确 bug、构建失败、测试失败或异常行为，先用 Superpowers 系统化调试；若修复会扩大到需求或架构变更，再补 OpenSpec。
- 小修、文案、配置、窄范围样式和单点改动可不创建 OpenSpec change，但仍按最小改动和完成前验证执行。
- 用户明确要求“先讨论/探索”时，只探索和读代码，不写业务代码；确认后再进入 OpenSpec 或直接实现。

### 推荐顺序

1. 探索：用户明确要求 explore 或不写代码时用 OpenSpec explore；新功能需求澄清时 Superpowers brainstorming 只作为思考方法，结论回写 OpenSpec。
2. 提案：需要正式变更时，用 OpenSpec propose 创建 `proposal.md`、`design.md`、`tasks.md`。
3. 实施：用 OpenSpec apply 读取任务和上下文；每个任务内部按 Superpowers 执行。
4. 验证：完成前必须用 Superpowers verification-before-completion 选择并运行真实验证命令。
5. 归档：OpenSpec 任务完成并验证后，用 `openspec archive <change-name>` 归档。

### 实施细则

- OpenSpec task 只定义“做什么”；具体“怎么做”由 Superpowers 和本项目既有代码模式决定。
- 新功能、bug 修复、行为变化和重构默认使用 Superpowers TDD：先写失败测试，再写最小实现，再重构。
- 遇到错误或测试失败，不直接试补丁；先用 Superpowers systematic-debugging 找根因，再改代码。
- 每完成一个 OpenSpec task 的代码并通过对应验证后，把 `tasks.md` 复选框从 `- [ ]` 改为 `- [x]`。
- 实施中发现 OpenSpec 设计不成立时，暂停修改代码，先更新或确认 OpenSpec 设计/任务。
- 不为满足流程而制造文档；小改动没有必要时，不创建 OpenSpec change。

### 冲突处理

- 用户直接指令 > 本 `AGENTS.md` > OpenSpec artifacts > Superpowers 技能说明 > 通用默认行为。
- OpenSpec 与 Superpowers 冲突时，以职责分层裁决：范围和验收听 OpenSpec，编码和验证方法听 Superpowers。
- OpenSpec 任务要求快速实现但缺少测试时，仍按 Superpowers TDD 补测试；除非用户明确批准跳过。
- Superpowers 建议扩大重构但 OpenSpec 未包含时，不扩大范围；先向用户说明并更新 change。
- 验证结果与 OpenSpec 任务状态冲突时，以真实命令和运行态结果为准，不标记完成。

## 项目速览

- Rust workspace，核心代码在 `crates/`，应用在 `apps/`。
- Web 前端在 `apps/web`，Gateway 在 `apps/gateway`。
- 统一 ReAct 主链路重点文件：
  - `crates/agents/src/agent_impl.rs`
  - `crates/agents/src/prompt/builder.rs`
  - `crates/agents/src/skills/unified_react_executor.rs`

## 架构边界

- `crates/agents` 禁止直接依赖 Web 框架，如 `axum`、`actix-web`、`rocket`、`warp`、`tide`、`salvo`。
- HTTP 相关能力应通过 `beebotos-gateway-lib` 或应用层提供。
- 前端新增路由时检查路由顺序，避免动态路由吞掉固定路由。

## 端口与本地运行

- Gateway API 固定端口：`8000`。
- Web 管理后台固定端口：`8090`。
- 不要长期使用临时 Web 端口替代 `8090`；聊天 WebSocket 对 `8090 -> 8000` 有特判。
- 当前 `8090` 服务静态目录可能是 `data/run/web-static`，源码改完若要让现有服务看到，需要重新构建并同步静态文件。
- 本地测试账号用户名为 `user`；密码需要时由用户临时提供，不写入文件。
- 日常开发避免反复跑 release 构建；优先用 `cargo check -p ...`、`cargo run -p ...` 和 `trunk serve`。
- `beebotos-dev.sh build` 当前偏发布构建，gateway/web 会走 `--release`，web 还会 `trunk build --release`。
- 裸 `nightly` 工具链更新会导致 Rust 缓存失效；若频繁全量重编，优先考虑固定 nightly 日期。

## 常用命令

```bash
cargo build --workspace
cargo test --workspace --all-features
cargo check -p beebotos-web --target wasm32-unknown-unknown
cd apps/web && wasm-pack build --target web --out-dir pkg --dev
cd contracts && forge test
```
