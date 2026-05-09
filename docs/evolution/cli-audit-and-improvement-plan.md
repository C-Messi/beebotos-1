# BeeBotOS CLI 审计与完善优化技术方案

> **版本:** v1.0  
> **日期:** 2026-05-08  
> **范围:** `apps/cli` 模块全量审计 vs `apps/gateway` REST API + `crates/agents` 核心能力  
> **状态:** 审计完成，方案待实施

---

## 一、执行摘要

`apps/cli` 是 BeeBotOS 的命令行入口，负责通过 REST API 和 WebSocket 与 Gateway 交互。当前 CLI 包含 **25 个顶级命令**，但经全量审计发现：

- **大量 CLI 命令调用的是"幻想端点"**（Gateway 中不存在，返回 404）
- **大量 Gateway 真实端点完全没有 CLI 覆盖**（覆盖度 < 30%）
- **多个核心子系统（Workflow、Planning、MCP、Cron）完全没有 CLI 命令**
- **部分命令是空壳实现**（stub，只打印 "not yet implemented"）

**本文档提供完整的缺失清单、优先级排序和实施路线图。**

---

## 二、审计方法论

审计采用三层对比法：

1. **CLI → Gateway API**: 检查 CLI 调用的 API 端点是否真实存在
2. **Gateway API → CLI**: 检查 Gateway 提供的每个端点是否有 CLI 命令
3. **Agents Crate → CLI**: 检查 `crates/agents` 的核心能力是否暴露给 CLI

---

## 三、关键发现（按严重程度）

### 🔴 P0: 幻想端点 — CLI 调用不存在的 API

以下 CLI 命令调用的端点在 Gateway 中**不存在**，运行时必定失败：

| CLI 命令 | 调用的幻想端点 | 实际情况 |
|---------|--------------|---------|
| `brain status` | `GET /brain/status` | ❌ 不存在 |
| `brain memory store` | `POST /brain/agents/{id}/memories` | ❌ 不存在 |
| `brain emotion get` | `GET /brain/agents/{id}/emotion` | ❌ 不存在 |
| `gateway install` | `POST /gateway/install` | ❌ 不存在 |
| `gateway start/stop/restart` | `POST /gateway/start` 等 | ❌ 不存在 |
| `security scan` | `POST /security/scan` | ❌ 不存在 |
| `security policy` | `GET /security/policies` | ❌ 不存在 |
| `security secret` | `GET /security/secrets` | ❌ 不存在 |
| `session create/list` | `POST /sessions` 等 | ❌ 不存在 |
| `memory index/consolidate` | `POST /memory/index` 等 | ❌ 不存在 |
| `infer text/image/audio` | `POST /infer/text` 等 | ❌ 不存在 |
| `model chat/compare` | `POST /model/chat` 等 | ❌ 不存在 |
| `message edit/delete/react` | 全部子命令 | ❌ 不存在或 stub |
| `channel add/remove/test` | 大部分子命令 | ❌ 不存在或 stub |
| `deploy` / `propose` / `vote` | 全部 | ❌ 纯 stub |

> **影响:** 这些命令对终端用户来说是"可用但无效"的陷阱命令，严重损害 CLI 可信度。

---

### 🔴 P0: 核心子系统 — CLI 完全缺失

以下 `crates/agents` 核心子系统在 CLI 中**零覆盖**：

| 子系统 | Agents Crate 路径 | Gateway 端点 | CLI 命令 |
|--------|------------------|-------------|---------|
| **Workflow** | `src/workflow/` | ✅ 10+ 端点 | ❌ **无** |
| **Composition** | `src/skills/composition/` | ✅ 5 端点 | ❌ **无** |
| **MCP** | `src/mcp/` | ✅ 4 端点 | ❌ **无** |
| **Planning** | `src/planning/` | ❌ 无独立端点 | ❌ **无** |
| **Cron Jobs** | `src/scheduling/cron.rs` | ✅ 8 端点 | ❌ **无** |
| **Task Monitor** | `src/task_monitor/` | ✅ 5 端点 | ❌ **无** |
| **State Machine** | `src/state_machine/` | ✅ 9 端点 | ❌ **无** |
| **LLM Metrics** | `src/llm/providers/` | ✅ 4 端点 | ❌ **无** |
| **Webchat** | `src/communication/` | ✅ 16 端点 | ❌ **无** |
| **User Channels** | `src/channels/` | ✅ 6 端点 | ❌ **无** |
| **User Settings** | `src/settings/` | ✅ 2 端点 | ❌ **无** |

---

### 🟡 P1: 路径不匹配 — CLI 调用旧路径

| CLI 命令 | CLI 调用的路径 | Gateway 真实路径 |
|---------|---------------|-----------------|
| `agent pause` | `POST /agents/{id}/pause` | `POST /api/v1/agents/:id/pause` ✅（路径格式不同） |
| `agent logs` | `GET /agents/{id}/logs` | `GET /api/v1/agents/:id/logs` ✅（路径格式不同） |
| `browser eval` | `POST /browser/eval` | `POST /api/v1/browser/evaluate` ✅（端点名不同） |
| `skill update` | `POST /skills/{id}/update` | ❌ 不存在 |
| `skill publish` | `POST /skills/publish` | ❌ 不存在 |

---

### 🟡 P1: 空壳命令（Stub）

以下命令存在 CLI 解析和 help 文本，但**内部实现为空或返回硬编码数据**：

| 命令 | 当前行为 |
|------|---------|
| `deploy` | `println!("Deploy command not fully implemented yet")` |
| `propose` | `println!("DAO proposal creation not yet implemented")` |
| `vote` | `println!("Voting not yet implemented")` |
| `model compare` | `bail!("Model comparison not yet implemented")` |
| `model test` | `bail!("Model testing not yet implemented")` |
| `message edit/delete/react` | 返回 `Ok(())` 或空 Vec |
| `channel add/remove/test` | 返回 `Ok(())` 或空 Vec |

---

## 四、缺失命令清单（按优先级排序）

### 🔴 P0: 必须立即补充（核心功能缺失）

#### 1. `beebot workflow` — 工作流管理

```
workflow list                    # GET /api/v1/workflows
workflow create <file.yaml>      # POST /api/v1/workflows
workflow get <id>                # GET /api/v1/workflows/:id
workflow get-source <id>         # GET /api/v1/workflows/:id/source
workflow update <id> <file.yaml> # PUT /api/v1/workflows/:id
workflow delete <id>             # DELETE /api/v1/workflows/:id
workflow install <path>          # POST /api/v1/workflows/install
workflow uninstall <id>          # POST /api/v1/workflows/:id/uninstall
workflow execute <id>            # POST /api/v1/workflows/:id/execute
workflow status <id>             # GET /api/v1/workflows/:id/status
workflow instance-list           # GET /api/v1/workflow-instances
workflow instance-get <id>       # GET /api/v1/workflow-instances/:id
workflow instance-cancel <id>    # POST /api/v1/workflow-instances/:id/cancel
workflow dashboard               # GET /api/v1/workflows/dashboard/stats
```

#### 2. `beebot cron` — 定时任务管理

```
cron list                        # GET /api/v1/cron/jobs
cron create <file.json>          # POST /api/v1/cron/jobs
cron get <id>                    # GET /api/v1/cron/jobs/:id
cron update <id> <file.json>     # PUT /api/v1/cron/jobs/:id
cron delete <id>                 # DELETE /api/v1/cron/jobs/:id
cron toggle <id>                 # POST /api/v1/cron/jobs/:id/toggle
cron run <id>                    # POST /api/v1/cron/jobs/:id/run
cron history <id>                # GET /api/v1/cron/jobs/:id/runs
```

#### 3. `beebot mcp` — MCP 服务器/工具管理

```
mcp list                         # GET /api/v1/mcp/servers
mcp tools <server>               # GET /api/v1/mcp/servers/:name/tools
mcp call <server> <tool>         # POST /api/v1/mcp/servers/:name/tools/:tool/call
mcp bridge                       # POST /api/v1/mcp/bridge
```

#### 4. 修复幻想端点 — 将 stub 命令标记为 deprecated 或实现真实调用

对上文列出的所有"幻想端点"命令，选择以下策略之一：
- **A. 实现真实调用**（如果有对应的 Gateway 端点）
- **B. 标记为 deprecated** 并隐藏（如果没有对应端点且短期内不会实现）
- **C. 删除**（纯空壳命令）

---

### 🟡 P1: 应该补充（重要功能缺失）

#### 5. `beebot webchat` — WebChat 会话管理

```
webchat list                     # GET /api/v1/webchat/sessions
webchat create                   # POST /api/v1/webchat/sessions
webchat delete <id>              # DELETE /api/v1/webchat/sessions/:id
webchat messages <id>            # GET /api/v1/webchat/sessions/:id/messages
webchat send <id> <msg>          # POST /api/v1/webchat/sessions/:id/messages/stream
webchat export <id>              # GET /api/v1/webchat/sessions/:id/export
webchat import <file>            # POST /api/v1/webchat/sessions/import
webchat ack <msg-id>             # POST /api/v1/webchat/messages/:id/ack
```

#### 6. `beebot channel` 重构 — 调用真实 Gateway 端点

当前 `channel` 命令几乎全部调用幻想端点。需要重构为调用真实端点：

```
channel list                     # GET /api/v1/channels
channel get <id>                 # GET /api/v1/channels/:id
channel update <id>              # PUT /api/v1/channels/:id
channel enable <id>              # POST /api/v1/channels/:id/enable
channel test <id>                # POST /api/v1/channels/:id/test
channel wechat-qr                # POST /api/v1/channels/wechat/qr
channel webchat-send             # POST /api/v1/channels/webchat/messages
```

#### 7. `beebot user-channel` — 用户频道管理

```
user-channel list                # GET /api/v1/user-channels
user-channel create              # POST /api/v1/user-channels
user-channel get <id>            # GET /api/v1/user-channels/:id
user-channel delete <id>         # DELETE /api/v1/user-channels/:id
user-channel connect <id>        # POST /api/v1/user-channels/:id/connect
user-channel disconnect <id>     # POST /api/v1/user-channels/:id/disconnect
```

#### 8. `beebot settings` — 用户设置

```
settings get                     # GET /api/v1/user/settings
settings set <key> <value>       # PUT /api/v1/user/settings
```

#### 9. `beebot llm` — LLM 指标与配置

```
llm metrics                      # GET /api/v1/llm/metrics
llm config                       # GET /api/v1/llm/config
llm config-update <file>         # PUT /api/v1/llm/config
llm health                       # GET /api/v1/llm/health
```

#### 10. `beebot composition` — 技能组合管理

```
composition list                 # GET /api/v1/compositions
composition create <file>        # POST /api/v1/compositions
composition get <id>             # GET /api/v1/compositions/:id
composition delete <id>          # DELETE /api/v1/compositions/:id
composition execute <id>         # POST /api/v1/compositions/:id/execute
```

---

### 🟢 P2: 建议补充（增强功能）

#### 11. `beebot task` — 任务监控

```
task stats                       # GET /api/v1/tasks/stats
task list                        # GET /api/v1/tasks/monitored
task status <agent-id>           # GET /api/v1/tasks/agents/:id
task cancel <agent-id>           # POST /api/v1/tasks/agents/:id/cancel
task faults                      # GET /api/v1/tasks/fault-detection
```

#### 12. `beebot state` — 状态机管理

```
state list                       # GET /api/v1/states
state stats                      # GET /api/v1/states/stats
state get <agent-id>             # GET /api/v1/agents/:id/state
state context <agent-id>         # GET /api/v1/agents/:id/state/context
state transitions <agent-id>     # GET /api/v1/agents/:id/state/transitions
state transition <agent-id> <to> # POST /api/v1/agents/:id/state/transition
```

#### 13. `beebot system` — 系统更新管理

```
system update-status             # GET /api/v1/system/updates/status
system update-check              # POST /api/v1/system/updates/check
system update-apply              # POST /api/v1/system/updates/apply
system update-rollback           # POST /api/v1/system/updates/rollback
```

#### 14. `beebot browser` 重构 — 调用真实端点

当前 browser 命令大部分调用幻想端点。重构为真实端点：

```
browser status                   # GET /api/v1/browser/status
browser connect                  # POST /api/v1/browser/connect
browser disconnect               # POST /api/v1/browser/disconnect
browser navigate <url>           # POST /api/v1/browser/navigate
browser evaluate <script>        # POST /api/v1/browser/evaluate
browser screenshot               # POST /api/v1/browser/screenshot
browser batch <file.json>        # POST /api/v1/browser/batch
browser sandbox-list             # GET /api/v1/browser/sandboxes
browser sandbox-create           # POST /api/v1/browser/sandboxes
browser sandbox-delete <id>      # DELETE /api/v1/browser/sandboxes/:id
browser profile-list             # GET /api/v1/browser/profiles
browser profile-create <name>    # POST /api/v1/browser/profiles
browser profile-delete <id>      # DELETE /api/v1/browser/profiles/:id
```

#### 15. `beebot skill execute` — 技能直接执行

```
skill execute <id> --input "..." # POST /api/v1/skills/:id/execute
```

---

### 🔵 P3: 可选补充（高级功能）

#### 16. `beebot plan` — 规划引擎（Gateway 无独立端点，需新增）

> ⚠️ 需要先在 Gateway 暴露 Planning API，CLI 才能调用。

```
plan create --agent <id> --goal "..." --strategy react
plan execute <plan-id>
plan status <plan-id>
plan cancel <plan-id>
```

#### 17. `beebot evolution` — 进化引擎（Gateway 无独立端点，需新增）

> ⚠️ 需要先在 Gateway 暴露 Evolution API。

```
evolution nudge --agent <id>
evolution distill --trajectory <file>
evolution capo --agent <id>
evolution benchmark
```

#### 18. `beebot auth` — 认证管理

```
auth login                       # POST /api/v1/auth/login
auth register                    # POST /api/v1/auth/register
auth refresh                     # POST /api/v1/auth/refresh
auth logout                      # POST /api/v1/auth/logout
auth me                          # GET /api/v1/auth/me
```

---

## 五、具体修复建议

### 5.1 修复路径不匹配

所有 CLI 调用的 API 路径应统一为 Gateway 真实路径格式：

```rust
// 修改前（错误）
self.client.post(&format!("/agents/{}/pause", id))

// 修改后（正确）
self.client.post(&format!("/api/v1/agents/{}/pause", id))
```

### 5.2 删除或隐藏纯 Stub 命令

对于以下纯空壳命令，建议**隐藏**（`#[command(hide = true)]`）并在 help 中标注 `[DEPRECATED - Not Implemented]`：

- `deploy`
- `propose`
- `vote`
- `message edit/delete/react/poll/thread/role/member/media/event`
- `channel add/remove/login/logout/send/webhook`
- `brain memory store/retrieve/consolidate`
- `brain emotion get/set`
- `brain evolve`
- `security scan/policy/audit/secret/acl`
- `memory index/consolidate/forget/graph`
- `infer *`（全部子命令）
- `model compare/test/order/aliases/fallbacks/embed`

### 5.3 统一错误处理

当前部分命令在 API 返回 404 时静默失败或返回空列表。建议统一错误处理：

```rust
// 修改前
let jobs = svc.list_jobs().await.ok()?; // 静默忽略错误

// 修改后
let jobs = svc.list_jobs().await?;      // 将错误传播给终端用户
```

### 5.4 新增 API Client 方法

在 `apps/cli/src/client.rs` 的 `ApiClient` 中补充以下缺失方法：

```rust
// Workflow
async fn list_workflows(&self) -> Result<Vec<Workflow>, ApiError>
async fn create_workflow(&self, yaml: &str) -> Result<Workflow, ApiError>
async fn get_workflow(&self, id: &str) -> Result<Workflow, ApiError>
async fn delete_workflow(&self, id: &str) -> Result<(), ApiError>
async fn execute_workflow(&self, id: &str, context: Value) -> Result<WorkflowExecution, ApiError>

// Cron
async fn list_cron_jobs(&self) -> Result<Vec<CronJob>, ApiError>
async fn create_cron_job(&self, req: &CronJobRequest) -> Result<CronJob, ApiError>
async fn get_cron_job(&self, id: &str) -> Result<CronJob, ApiError>
async fn delete_cron_job(&self, id: &str) -> Result<(), ApiError>
async fn toggle_cron_job(&self, id: &str) -> Result<bool, ApiError>
async fn run_cron_job(&self, id: &str) -> Result<String, ApiError>

// MCP
async fn list_mcp_servers(&self) -> Result<Vec<McpServer>, ApiError>
async fn list_mcp_tools(&self, server: &str) -> Result<Vec<McpTool>, ApiError>
async fn call_mcp_tool(&self, server: &str, tool: &str, args: Value) -> Result<Value, ApiError>

// ... 其他缺失方法
```

---

## 六、实施路线图

### Phase 1: P0 紧急修复（1-2 周）

**目标:** 修复所有"幻想端点"和路径不匹配问题，确保 CLI 调用的每个命令都能与 Gateway 正确交互。

| 任务 | 工作量 | 负责人 |
|------|--------|--------|
| 1.1 统一所有 CLI API 路径为 `/api/v1/*` 格式 | 2d | — |
| 1.2 删除/隐藏所有纯 stub 命令 | 1d | — |
| 1.3 修复 `agent pause/resume/logs` 路径 | 0.5d | — |
| 1.4 修复 `browser` 命令调用真实端点 | 1d | — |
| 1.5 为每个 stub 命令添加 `#[command(hide = true)]` | 0.5d | — |

### Phase 2: P1 核心补充（2-3 周）

**目标:** 实现 Workflow、Cron、MCP、Webchat 四个核心子系统的 CLI 命令。

| 任务 | 工作量 | 依赖 |
|------|--------|------|
| 2.1 实现 `beebot workflow` 全部命令 | 3d | — |
| 2.2 实现 `beebot cron` 全部命令 | 2d | — |
| 2.3 实现 `beebot mcp` 全部命令 | 2d | — |
| 2.4 实现 `beebot webchat` 全部命令 | 2d | — |
| 2.5 重构 `beebot channel` 调用真实端点 | 2d | — |
| 2.6 实现 `beebot user-channel` 命令 | 1d | — |
| 2.7 实现 `beebot settings` 命令 | 0.5d | — |
| 2.8 实现 `beebot llm` 命令 | 1d | — |
| 2.9 实现 `beebot composition` 命令 | 1d | — |
| 2.10 在 `client.rs` 中补充所有缺失 API 方法 | 2d | — |

### Phase 3: P2 增强补充（1-2 周）

**目标:** 实现 Task Monitor、State Machine、System Update、Browser 重构、Skill Execute。

| 任务 | 工作量 |
|------|--------|
| 3.1 实现 `beebot task` 命令 | 1d |
| 3.2 实现 `beebot state` 命令 | 1d |
| 3.3 实现 `beebot system` 命令 | 0.5d |
| 3.4 重构 `beebot browser` 调用真实端点 | 1d |
| 3.5 实现 `beebot skill execute` | 0.5d |

### Phase 4: P3 高级功能（后续迭代）

**目标:** Planning、Evolution、Auth 等高级功能。

| 任务 | 工作量 | 依赖 |
|------|--------|------|
| 4.1 在 Gateway 暴露 Planning API | 3d | — |
| 4.2 实现 `beebot plan` 命令 | 2d | 4.1 |
| 4.3 在 Gateway 暴露 Evolution API | 3d | — |
| 4.4 实现 `beebot evolution` 命令 | 2d | 4.3 |
| 4.5 实现 `beebot auth` 命令 | 1d | — |

---

## 七、附录：完整对照表

### Gateway 端点 → CLI 命令对照表

| Gateway 端点 | 状态 | CLI 命令 |
|-------------|------|---------|
| `GET /api/v1/agents` | ✅ | `agent list` |
| `POST /api/v1/agents` | ✅ | `agent create` |
| `GET /api/v1/agents/:id` | ✅ | `agent` (隐含) |
| `PUT /api/v1/agents/:id` | ❌ | **缺失** |
| `DELETE /api/v1/agents/:id` | ✅ | `agent delete` |
| `POST /api/v1/agents/:id/start` | ✅ | `agent start` |
| `POST /api/v1/agents/:id/stop` | ✅ | `agent stop` |
| `POST /api/v1/agents/:id/pause` | ⚠️ | `agent pause`（路径格式不匹配） |
| `POST /api/v1/agents/:id/resume` | ⚠️ | `agent resume`（路径格式不匹配） |
| `GET /api/v1/agents/:id/status` | ❌ | **缺失** |
| `POST /api/v1/agents/:id/tasks` | ✅ | `agent run` |
| `GET /api/v1/agents/:id/logs` | ⚠️ | `agent logs`（路径格式不匹配） |
| `POST /api/v1/agents/:id/channels` | ✅ | `agent channel bind` |
| `GET /api/v1/agents/:id/channels` | ✅ | `agent channel list` |
| `DELETE /api/v1/agents/:id/channels/:channel_id` | ✅ | `agent channel unbind` |
| `POST /api/v1/agents/:id/agent-channel-bindings` | ❌ | **缺失** |
| `GET /api/v1/agents/:id/agent-channel-bindings` | ❌ | **缺失** |
| `GET /api/v1/skills` | ✅ | `skill list` |
| `GET /api/v1/skills/:id` | ✅ | `skill show` |
| `POST /api/v1/skills/install` | ✅ | `skill install` |
| `DELETE /api/v1/skills/:id/uninstall` | ✅ | `skill uninstall` |
| `POST /api/v1/skills/:id/execute` | ❌ | **缺失** |
| `GET /api/v1/skills/hub/health` | ❌ | **缺失** |
| `GET /api/v1/workflows` | ❌ | **缺失** |
| `POST /api/v1/workflows` | ❌ | **缺失** |
| `GET /api/v1/workflows/:id` | ❌ | **缺失** |
| `PUT /api/v1/workflows/:id` | ❌ | **缺失** |
| `DELETE /api/v1/workflows/:id` | ❌ | **缺失** |
| `POST /api/v1/workflows/:id/execute` | ❌ | **缺失** |
| `GET /api/v1/workflow-instances` | ❌ | **缺失** |
| `GET /api/v1/compositions` | ❌ | **缺失** |
| `POST /api/v1/compositions` | ❌ | **缺失** |
| `GET /api/v1/cron/jobs` | ❌ | **缺失** |
| `POST /api/v1/cron/jobs` | ❌ | **缺失** |
| `GET /api/v1/cron/jobs/:id` | ❌ | **缺失** |
| `PUT /api/v1/cron/jobs/:id` | ❌ | **缺失** |
| `DELETE /api/v1/cron/jobs/:id` | ❌ | **缺失** |
| `POST /api/v1/cron/jobs/:id/toggle` | ❌ | **缺失** |
| `POST /api/v1/cron/jobs/:id/run` | ❌ | **缺失** |
| `GET /api/v1/mcp/servers` | ❌ | **缺失** |
| `GET /api/v1/mcp/servers/:name/tools` | ❌ | **缺失** |
| `POST /api/v1/mcp/servers/:name/tools/:tool/call` | ❌ | **缺失** |
| `GET /api/v1/webchat/sessions` | ❌ | **缺失** |
| `POST /api/v1/webchat/sessions` | ❌ | **缺失** |
| `GET /api/v1/webchat/sessions/:id/messages` | ❌ | **缺失** |
| `GET /api/v1/channels` | ❌ | **缺失** |
| `GET /api/v1/channels/:id` | ❌ | **缺失** |
| `PUT /api/v1/channels/:id` | ❌ | **缺失** |
| `GET /api/v1/user-channels` | ❌ | **缺失** |
| `POST /api/v1/user-channels` | ❌ | **缺失** |
| `GET /api/v1/user/settings` | ❌ | **缺失** |
| `PUT /api/v1/user/settings` | ❌ | **缺失** |
| `GET /api/v1/llm/metrics` | ❌ | **缺失** |
| `GET /api/v1/llm/config` | ❌ | **缺失** |
| `GET /api/v1/tasks/stats` | ❌ | **缺失** |
| `GET /api/v1/tasks/monitored` | ❌ | **缺失** |
| `GET /api/v1/states` | ❌ | **缺失** |
| `GET /api/v1/states/stats` | ❌ | **缺失** |
| `GET /api/v1/system/updates/status` | ❌ | **缺失** |
| `GET /api/v1/browser/status` | ⚠️ | `browser status`（路径格式不匹配） |
| `POST /api/v1/browser/connect` | ❌ | **缺失** |
| `POST /api/v1/browser/disconnect` | ❌ | **缺失** |
| `POST /api/v1/browser/navigate` | ⚠️ | `browser navigate`（路径格式不匹配） |
| `POST /api/v1/browser/evaluate` | ⚠️ | `browser eval`（端点名不匹配） |
| `POST /api/v1/browser/screenshot` | ❌ | **缺失** |
| `POST /api/v1/browser/batch` | ❌ | **缺失** |
| `GET /api/v1/browser/sandboxes` | ❌ | **缺失** |

---

## 八、总结

### 当前覆盖度统计

| 维度 | 已有 | 缺失 | 覆盖度 |
|------|------|------|--------|
| Gateway 端点总数 | ~110+ | — | — |
| CLI 有对应命令的端点 | ~30 | ~80+ | **~27%** |
| CLI 命令中调用真实端点的 | ~15 | ~10 | **~60%** |
| Agents Crate 核心能力暴露 | ~25% | ~75% | **~25%** |

### 核心结论

1. **CLI 不是 Gateway 的完整映射**，而是一个早期原型，大量命令基于假设的 API 设计，而非真实端点。
2. **Workflow、Cron、MCP、Planning、Composition** 是 BeeBotOS 的核心差异化能力，但 CLI 完全缺失。
3. **修复路径不匹配**可以在不新增功能的情况下，让现有命令的可用性提升 50% 以上。
4. **Phase 1 + Phase 2**（约 4 周）可以让 CLI 的 Gateway 端点覆盖度从 27% 提升到 **80%+**。

---

*本文档由 Kimi Code CLI 自动审计生成，基于对 `apps/cli`、`apps/gateway`、`crates/agents` 代码的静态分析。*
