# BeeBotOS Agent Guide

## 交互与改动原则

- 对用户输出使用中文；与工具、命令、模型交互使用英文。
- 代码、注释、文档都保持精简高效，非必要不新增注释或文档。
- 只做当前需求直接相关的改动，严禁顺手影响已有功能或无关文件。

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
