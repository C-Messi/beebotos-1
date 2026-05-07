# CLI 功能完整性审计报告

## 执行摘要

Gateway 共有 **179 个 API 路由**，覆盖 20+ 个功能模块。
CLI 现有 **23 个命令**，但存在大量功能缺口。

---

## 一、完全缺失的 CLI 命令模块（10 个）

以下 Gateway API 模块在 CLI 中完全没有对应的顶层命令：

| 模块 | Gateway 路由数 | 缺失原因分析 |
|------|--------------|-------------|
| **cron** | 8 | 定时任务管理，CLI 完全没有 |
| **mcp** | 4 | MCP 服务器和工具调用，CLI 完全没有 |
| **workflow** | 18 (workflows+instances) | 工作流编排，CLI 完全没有 |
| **webchat** | 16 | WebChat 会话管理，CLI 完全没有 |
| **composition** | 5 | Skill 组合管理，CLI 完全没有 |
| **state** | 3 | 状态机管理，CLI 完全没有 |
| **task** | 5 | 任务监控，CLI 完全没有 |
| **treasury** | 2 | 金库管理，CLI 完全没有 |
| **user-settings** | 2 | 用户设置，CLI 完全没有 |
| **auth** | 5 | 认证管理，CLI 完全没有 |

> **注**: 部分功能可能通过其他命令间接覆盖（如 agent pause/resume 覆盖了部分 state machine 功能），但缺乏完整的独立命令支持。

---

## 二、现有 CLI 命令的功能缺口

### 2.1 Agent 命令


**已有子命令**: Create, List, Start, Stop, Pause, Resume, Logs, Delete, Clone, Export, Import, SetIdentity, Exec, Run, Bind, Unbind

**缺失子命令**:
- `Update` - PUT /api/v1/agents/:id
- `Retry` - POST /api/v1/agents/:id/retry
- `Show` / `Info` - GET /api/v1/agents/:id (Logs 获取日志但不获取配置)
- `State` / `GetState` - GET /api/v1/agents/:id/state
- `Transition` - POST /api/v1/agents/:id/state/transition
- `ValidTransitions` - GET /api/v1/agents/:id/state/transitions
- `GetStateContext` - GET /api/v1/agents/:id/state/context

- list_agents_v2, create_agent_v2, get_agent_v2, delete_agent_v2
- start_agent_v2, stop_agent_v2, get_agent_status_v2, execute_task_v2
- list_agent_channels, bind_agent_channel, unbind_agent_channel
- list_agent_channel_bindings_v2, bind_agent_channel_v2, unbind_agent_channel_v2

### 2.2 Browser 命令

Gateway API (14 路由)

**已有子命令**: Status, Start, Stop, ResetProfile, Navigate, Screenshot, Snapshot, Pdf, Click, Type, Press, Hover, Scroll, Fill, Upload, Wait, Eval, Console, Cdp

**缺失子命令**:
- `Connect` - POST /api/v1/browser/connect
- `Disconnect` - POST /api/v1/browser/disconnect
- `Evaluate` (已有 Eval 但可能不完全对应) - POST /api/v1/browser/evaluate
- `Batch` - POST /api/v1/browser/batch
- `ListProfiles` - GET /api/v1/browser/profiles
- `CreateProfile` - POST /api/v1/browser/profiles
- `DeleteProfile` - DELETE /api/v1/browser/profiles/:id
- `ListSandboxes` - GET /api/v1/browser/sandboxes
- `CreateSandbox` - POST /api/v1/browser/sandboxes
- `DeleteSandbox` - DELETE /api/v1/browser/sandboxes/:id
- `GetSandboxStats` - GET /api/v1/browser/sandboxes/:id/stats

### 2.3 Chain 命令


**已有子命令**: Status, Balance, Transfer, Deploy, Call, Watch

**缺失子命令**:
- `Identity` / `GetIdentity` - GET /api/v1/chain/agents/:id/identity
- `RegisterIdentity` - POST /api/v1/chain/agents/:id/identity
- `HasIdentity` - GET /api/v1/chain/agents/:id/has-identity
- `Proposals` / `ListProposals` - GET /api/v1/chain/dao/proposals
- `GetProposal` - GET /api/v1/chain/dao/proposals/:id
- `CreateProposal` - POST /api/v1/chain/dao/proposals
- `CastVote` - POST /api/v1/chain/dao/proposals/:id/vote
- `DaoSummary` - GET /api/v1/chain/dao/summary

- get_wallet_info, transfer (V2)

### 2.4 Channel 命令


**已有子命令**: List, Status, Capabilities, Resolve, Logs, Add, Remove, Login, Logout, Send, Test, Generate

**缺失子命令**:
- `Update` - PUT /api/v1/channels/:id
- `Enable` / `Disable` - POST /api/v1/channels/:id/enable
- `SendWebchatMessage` - POST /api/v1/channels/webchat/messages
- `GetWechatQr` - POST /api/v1/channels/wechat/qr
- `CheckWechatQr` - POST /api/v1/channels/wechat/qr/check

**V2 User Channels 完全未覆盖** (6 路由):
- list_user_channels, create_user_channel, get_user_channel, delete_user_channel
- connect_user_channel, disconnect_user_channel

### 2.5 Config 命令

**已有子命令**: Show, Get, Set, List, Edit, Reset, Validate

**缺失子命令**:
- `Reload` - POST /api/v1/admin/config/reload (这是 Gateway 配置重载，不是本地配置)

### 2.6 Gateway 命令

**已有子命令**: Run, Install, Uninstall, Start, Stop, Restart, Status, Health, Probe, Discover, Call, UsageCost, Logs, Upgrade

**缺失子命令**:
- `Metrics` - GET /metrics
- `Liveness` - GET /live
- `Readiness` - GET /ready
- `SystemStatus` - GET /status
- `WebSocket` - GET /ws
- `WebSocketStatus` - GET /ws/status

### 2.7 Infer 命令

Gateway 没有直接的 infer 模块，但 Capabilities API 有 2 个路由：
- GET /api/v1/capabilities - list_capability_types
- POST /api/v1/capabilities/validate - validate_capabilities

**缺失子命令**:
- `Capabilities` / `ListCapabilities` - 列出能力类型
- `ValidateCapability` - 验证能力配置

### 2.8 LLM Config/Metrics

Gateway API (4 路由)

CLI `model` 命令主要管理模型配置，但缺少：
- `Config` / `GetConfig` - GET /api/v1/llm/config
- `UpdateConfig` - PUT /api/v1/llm/config
- `Health` - GET /api/v1/llm/health
- `Metrics` - GET /api/v1/llm/metrics

### 2.9 Message 命令

**已有子命令**: Send, Broadcast, Edit, Delete, Read, Chat, History, 以及 reaction/poll/thread/role/member 子命令

Gateway 没有独立的 message 模块（消息通过 channels/webchat 处理），CLI message 命令看起来是独立的本地功能而非 Gateway API 客户端。

### 2.10 Model 命令

**已有子命令**: List, Status, Set, SetImage, Scan, Info, Compare, Test, Chat, Complete, Embed, Update, 以及 provider/alias/fallback 管理子命令

Gateway LLM API 覆盖情况良好，但缺少与 Gateway 同步的子命令。

### 2.11 Payment 命令

Gateway Treasury API (2 路由)

CLI payment 命令看起来是独立的链上支付功能，但缺少 Treasury 相关功能：
- `Treasury` / `GetTreasury` - GET /api/v1/treasury
- `TreasuryTransfer` - POST /api/v1/treasury/transfer

### 2.12 Security 命令

**已有子命令**: Status, Scan, Policy, Audit, Secret, Acl

Gateway 没有独立的安全模块 API，CLI security 命令看起来是本地安全扫描工具。

### 2.13 Session 命令

**已有子命令**: Create, List, Resume, Show, Archive, Delete

Gateway 没有独立的 session 模块 API，CLI session 命令看起来是本地会话管理。

### 2.14 Skill 命令

Gateway API (6 路由)

**已有子命令**: List, Show, Install, Uninstall, Update, Create, Publish

**缺失子命令**:
- `Execute` - POST /api/v1/skills/:id/execute
- `HubHealth` - GET /api/v1/skills/hub/health
- `Instance` 相关命令 - 参见 Instance 模块

### 2.15 Instance 命令

Gateway API (6 路由)

**CLI 完全没有 instance 命令！**

缺失功能:
- `ListInstances` - GET /api/v1/instances
- `CreateInstance` - POST /api/v1/instances
- `GetInstance` - GET /api/v1/instances/:id
- `UpdateInstance` - PUT /api/v1/instances/:id
- `DeleteInstance` - DELETE /api/v1/instances/:id
- `ExecuteInstance` - POST /api/v1/instances/:id/execute

### 2.16 Update 命令

**已有子命令**: Check, Force, Rollback, Version, Server, Channel

这是自更新命令，不是 Gateway 系统更新管理。Gateway 有 updater API (4 路由)，但 CLI 只管理自身的更新。

---

## 三、功能缺口严重程度分级

### 🔴 严重缺失（完全无 CLI 命令）

| 模块 | 路由数 | 影响 |
|------|-------|------|
| workflow | 18 | 工作流是核心功能，完全无法通过 CLI 管理 |
| webchat | 16 | 聊天会话管理，完全无法通过 CLI 管理 |
| cron | 8 | 定时任务，完全无法通过 CLI 管理 |
| auth | 5 | 认证，完全无法通过 CLI 管理 |
| mcp | 4 | MCP 集成，完全无法通过 CLI 管理 |
| composition | 5 | Skill 组合，完全无法通过 CLI 管理 |
| task | 5 | 任务监控，完全无法通过 CLI 管理 |
| state | 3 | 状态机，完全无法通过 CLI 管理 |
| treasury | 2 | 金库，完全无法通过 CLI 管理 |
| user-settings | 2 | 用户设置，完全无法通过 CLI 管理 |
| capabilities | 2 | 能力验证，完全无法通过 CLI 管理 |

**合计: 70 个路由（39% 的 Gateway API）**

### 🟡 中度缺失（有命令但缺关键子命令）

| 命令 | 缺失子命令 | 影响 |
|------|-----------|------|
| agent | Update, Retry, Show, State/Transition | 无法更新 agent 配置、管理状态机 |
| browser | Connect/Disconnect, Profiles, Sandboxes, Batch | 无法管理浏览器配置、批量操作 |
| chain | Identity, DAO, Proposals | 无法管理链上身份和 DAO |
| channel | Update, Enable, Wechat QR | 无法更新/禁用通道，缺少微信集成 |
| skill | Execute, HubHealth | 无法执行 skill，无法检查 Hub 健康 |
| gateway | Metrics, Liveness, Readiness, SystemStatus | 无法查看系统指标和健康状态 |

**合计: ~30 个路由**


|------|-------------|

**合计: 26 个路由**

---

## 四、建议优先级

### 高优先级（P0）
1. **workflow** - 18 路由，核心编排功能
2. **webchat** - 16 路由，主要用户交互界面
3. **instance** - 6 路由，Skill 实例管理
4. **cron** - 8 路由，定时任务管理

### 中优先级（P1）
5. **agent** 子命令补全（Update, Show, State）
6. **browser** 子命令补全（Profiles, Sandboxes, Connect）
7. **mcp** - 4 路由
8. **auth** - 5 路由
10. **task** - 5 路由

### 低优先级（P2）
11. **composition** - 5 路由
12. **state** - 3 路由
13. **treasury** - 2 路由
14. **user-settings** - 2 路由
15. **capabilities** - 2 路由
16. **gateway** Metrics/Health 子命令

---

## 五、统计汇总

| 类别 | 数量 |
|------|------|
| Gateway API 路由总数 | 179 |
| CLI 已覆盖路由（估算） | ~80 |
| CLI 缺失路由（估算） | ~99 |
| 完全缺失的命令模块 | 10 |
| 现有命令中缺失子命令 | 6+ |
| CLI 命令总数 | 23 |
| 建议新增命令数 | 10 |
