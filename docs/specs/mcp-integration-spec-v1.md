# BeeBotOS MCP 接入规范 v1.0

> Model Context Protocol (MCP) 集成规范
> 版本: 1.0.0 | 最后更新: 2026-04-30

## 一、概述

本文档定义 BeeBotOS 与 **Model Context Protocol (MCP)** 的集成规范，覆盖传输层、配置层、Skill 桥接、REST API、安全策略及市场规则兼容性。

BeeBotOS 的 MCP 实现遵循 [OpenClaw MCP 规范](https://openclaw.dev/specs/mcp)，支持以下核心能力：

| 能力 | 状态 | 说明 |
|------|------|------|
| `tools/list` | ✅ | 自动桥接为 BeeBotOS Skills |
| `tools/call` | ✅ | Agent 无感知调用 |
| `resources/list` | ✅ | 通过 MCP Client 访问 |
| `resources/read` | ✅ | 通过 MCP Client 访问 |
| `prompts/list` | ✅ | 通过 MCP Client 访问 |
| `prompts/get` | ✅ | 通过 MCP Client 访问 |
| `initialize` | ✅ | 启动时自动握手 |

### 架构分层

```
┌─────────────────────────────────────────┐
│  Layer 4: Applications                  │
│  - Agent Runtime                        │
│  - Skill Registry                       │
│  - Workflow Engine                      │
├─────────────────────────────────────────┤
│  Layer 3: MCP Bridge                    │
│  - McpSkillBridge (tools → Skills)      │
│  - Agent::execute_registered_skill      │
│  - Gateway REST API (/api/v1/mcp/*)     │
├─────────────────────────────────────────┤
│  Layer 2: MCP Protocol                  │
│  - MCPClient (JSON-RPC 2.0)             │
│  - MCPManager (多连接管理)               │
│  - Request-Response 匹配 (ID-based)     │
├─────────────────────────────────────────┤
│  Layer 1: Transport                     │
│  - StdioTransport (本地子进程)           │
│  - HttpTransport (HTTP POST + SSE)      │
│  - TransportBridge (Channel 桥接)       │
├─────────────────────────────────────────┤
│  Layer 0: External MCP Servers          │
│  - filesystem, github, postgres, etc.   │
└─────────────────────────────────────────┘
```

---

## 二、传输层规范

### 2.1 stdio 传输

通过 `tokio::process` 启动本地子进程，使用 **stdin/stdout 行分隔 JSON-RPC** 通信。

**配置格式 (TOML):**
```toml
[[mcp.servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
env = { NODE_ENV = "production" }
working_dir = "/opt/mcp"
```

**字段说明:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✅ | Server 唯一标识 |
| `transport` | string | ✅ | 固定值 `"stdio"` |
| `command` | string | ✅ | 可执行命令（支持绝对路径） |
| `args` | string[] | ❌ | 命令参数 |
| `env` | map<string,string> | ❌ | 子进程环境变量 |
| `working_dir` | string | ❌ | 子进程工作目录 |

**安全约束:**
- `command` 路径禁止包含 `..`（路径遍历防护）
- 支持 `allowed_commands` 白名单（全局配置）
- `Drop` 时自动 `kill` 子进程，防止僵尸进程

### 2.2 HTTP/SSE 传输

通过 HTTP POST 发送请求，SSE (`text/event-stream`) 或 HTTP Response 接收响应。

**配置格式 (TOML):**
```toml
[[mcp.servers]]
name = "github"
transport = "http"
url = "https://api.github.com/mcp"
auth_token = "ghp_xxxxxxxxxxxx"
headers = { "X-Custom-Header" = "value" }
use_sse = true
```

**字段说明:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✅ | Server 唯一标识 |
| `transport` | string | ✅ | 固定值 `"http"` |
| `url` | string | ✅ | MCP Server 基础 URL |
| `auth_token` | string | ❌ | Bearer Token（序列化时脱敏） |
| `headers` | map<string,string> | ❌ | 额外 HTTP Headers |
| `use_sse` | bool | ❌ | 是否使用 SSE 接收响应，默认 `false` |

**安全约束:**
- `enforce_tls = true` 时拒绝非 `https://` URL（不区分大小写）
- `auth_token` 使用 `secrecy::SecretString` 存储，Debug 输出脱敏
- 请求超时由 `timeout_ms` 控制（默认 30s）

### 2.3 传输公共接口

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: JsonRpcRequest) -> Result<(), MCPError>;
    async fn receive(&self) -> Result<JsonRpcResponse, MCPError>;
    async fn close(&self) -> Result<(), MCPError>;
    fn is_connected(&self) -> bool;
}
```

**TransportBridge** 负责将 `MCPClient` 的 Channel 与 Transport 实现桥接：
- Task 1: 从 `request_rx` 读取请求 → `transport.send()`
- Task 2: 从 `transport.receive()` 读取响应 → `response_tx`

---

## 三、配置规范

### 3.1 TOML 配置段

MCP 配置位于 `config/beebotos.toml` 的 `[mcp]` 段：

```toml
[mcp]
# 启动时自动连接并初始化 MCP servers
auto_init = true

# 全局超时（毫秒）
timeout_ms = 30000

# 全局重试次数
retry_count = 3

# 强制 HTTPS（生产环境建议开启）
enforce_tls = true

# stdio 命令白名单（空 = 允许所有）
# 示例: ["npx", "python", "/usr/local/bin/mcp-server"]
allowed_commands = []

# MCP Server 定义
[[mcp.servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[[mcp.servers]]
name = "github"
transport = "http"
url = "https://api.github.com/mcp"
auth_token = "ghp_xxx"
use_sse = true
```

### 3.2 配置结构 (Rust)

```rust
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
    pub timeout_ms: u64,
    pub retry_count: u32,
    pub auto_init: bool,
    pub allowed_commands: Vec<String>,
    pub enforce_tls: bool,
}

pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportConfig,
    pub timeout_ms: Option<u64>,
    pub retry_count: Option<u32>,
}

pub enum McpTransportConfig {
    Stdio { command, args, env, working_dir },
    Http { url, auth_token, headers, use_sse },
}
```

### 3.3 启动时行为

1. **加载配置** → 解析 `config.mcp.servers`
2. **创建 Transport** → 根据 `transport` 类型创建 `StdioTransport` 或 `HttpTransport`
3. **创建 Client** → `MCPClient::connect_stdio()` / `connect_http()`
4. **注册到 MCPManager** → `manager.register_client(name, client)`
5. **初始化握手** → `manager.initialize_all()`（调用 `initialize` + `notifications/initialized`）
6. **Skill 桥接** → `McpSkillBridge::bridge_all(manager, skill_registry)`
7. **链上注册**（可选）→ 若 `blockchain.enabled`，为 MCP skills 打印链上注册日志

---

## 四、Skill 桥接规范

### 4.1 桥接机制

MCP `tools/list` 结果自动注册为 BeeBotOS `Skill`，实现 **Agent 无感知调用**。

**Skill ID 格式:**
```
mcp:{server_name}/{tool_name}
```

**示例:**
- `mcp:filesystem/read_file`
- `mcp:github/create_issue`
- `mcp:postgres/query`

### 4.2 Skill 元数据映射

| MCP Tool 字段 | BeeBotOS Skill 字段 | 说明 |
|---------------|---------------------|------|
| `tool.name` | `id`, `name`, `entry_point` | Skill 标识 |
| `tool.description` | `manifest.description` | 功能描述 |
| `tool.input_schema` | `manifest.functions[0].inputs` | 参数列表（JSON Schema → FunctionDef） |
| server_name | `manifest.author` | 格式 `mcp:{server}` |
| — | `version` | 固定 `1.0.0` |
| — | `license` | 固定 `"MCP"` |
| — | `wasm_path` | 空（非 WASM Skill） |

### 4.3 执行路由

Agent 执行 Skill 时，若 Skill ID 以 `mcp:` 开头：

```rust
if let Some((server_name, tool_name)) = parse_mcp_skill_id(&skill_id) {
    // 1. 获取 MCP Client
    let client = mcp_manager.get_client(server_name).await?;

    // 2. 构建参数（input + parameters → JSON Map）
    let mut arguments = serde_json::Map::new();
    // input 优先解析为 JSON Object，失败则包装为 {"query": input}

    // 3. Security: JSON Schema 参数校验
    let tools = client.list_tools(None).await?;
    if let Some(tool) = tools.find(|t| t.name == tool_name) {
        validate_tool_arguments(&tool.input_schema, &arguments)?;
    }

    // 4. 调用工具
    let result = client.call_tool(tool_name, arguments).await?;

    // 5. 提取文本输出
    let output = result.content.iter()
        .filter_map(|c| match c { Text { text } => Some(text), _ => None })
        .join("\n");

    return SkillExecutionResult { success: !result.is_error, output, ... };
}
```

### 4.4 参数校验规则

`validate_tool_arguments(schema, arguments)` 执行以下校验：

1. **Required 检查**: 所有 `schema.required` 字段必须在 `arguments` 中存在
2. **未知参数检查**: `arguments` 中的键必须在 `schema.properties` 中定义
3. **类型检查**: 基础 JSON Schema 类型匹配
   - `string` / `integer` / `number` / `boolean` / `array` / `object`
   - `number` 兼容 `integer`

**不支持的高级特性**（预留扩展）：
- `anyOf` / `oneOf` / `allOf`
- `enum` 约束
- `minLength` / `maxLength` / `minimum` / `maximum`
- 嵌套对象深度校验
- `type: ["string", "null"]` 数组类型

---

## 五、Gateway REST API

### 5.1 端点列表

所有端点位于 `/api/v1/mcp/*`，受 `auth_middleware` 保护。

| 方法 | 路径 | 说明 | RBAC |
|------|------|------|------|
| `GET` | `/api/v1/mcp/servers` | 列出已连接的 MCP servers | 认证用户 |
| `GET` | `/api/v1/mcp/servers/:name/tools` | 列出某 server 的工具及市场元数据 | 认证用户 |
| `POST` | `/api/v1/mcp/servers/:name/tools/:tool/call` | 直接调用工具 | 认证用户 |
| `POST` | `/api/v1/mcp/bridge` | 手动触发 Skill 桥接 | **admin** |

### 5.2 请求/响应格式

#### `GET /api/v1/mcp/servers`

**Response:**
```json
[
  {
    "name": "filesystem",
    "connected": true
  },
  {
    "name": "github",
    "connected": true
  }
]
```

#### `GET /api/v1/mcp/servers/:name/tools`

**Response:**
```json
{
  "server": "filesystem",
  "tools": [
    {
      "name": "read_file",
      "description": "Read content of a file",
      "input_schema": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "File path" }
        },
        "required": ["path"]
      },
      "price": "0",
      "royalty_percent": 0,
      "nft_token_id": null
    }
  ]
}
```

#### `POST /api/v1/mcp/servers/:name/tools/:tool/call`

**Request:**
```json
{
  "arguments": {
    "path": "/tmp/test.txt"
  }
}
```

**Response:**
```json
{
  "success": true,
  "output": "Hello, World!\n",
  "is_error": false
}
```

#### `POST /api/v1/mcp/bridge`

**Response:**
```json
{
  "success": true,
  "registered": 3,
  "message": "3 MCP tool(s) bridged to skills"
}
```

---

## 六、安全策略

### 6.1 沙盒模式

BeeBotOS 支持两种 Skill 沙盒模式，通过 `SkillSecurityPolicy.sandbox_mode` 配置：

| 模式 | 说明 | 默认 |
|------|------|------|
| `Wasmtime` | WASM 运行时沙盒，Capability 权限模型，轻量级 | ✅ 默认 |
| `Docker` | Docker 容器沙盒，namespaces + cgroups，强隔离 | 可选 |

```rust
pub enum SandboxMode {
    Wasmtime,  // 默认
    Docker,    // 可选
}
```

### 6.2 传输层安全

**stdio:**
- `validate_command_path`: 禁止 `..` 路径遍历
- `validate_command_whitelist`: 命令白名单（`allowed_commands`）
- 白名单为空时允许所有命令（向后兼容，生产环境应配置白名单）

**HTTP:**
- `enforce_tls`: 强制 `https://`（不区分大小写）
- `auth_token`: `SecretString` 存储，Debug 输出 `[REDACTED]`

### 6.3 参数安全

- `validate_tool_arguments`: JSON Schema 基础校验（required + type）
- 工具调用前自动校验，失败返回 `InvalidConfig`

---

## 七、市场规则兼容

### 7.1 市场元数据

MCP Skill 注册时自动生成市场兼容元数据：

| 字段 | 值 | 说明 |
|------|-----|------|
| `source` | `"mcp"` | 来源标识 |
| `price` | `"0"` | 默认免费（可扩展） |
| `royalty_percent` | `0` | 默认无版税（可扩展） |
| `nft_token_id` | `null` | 未上链（可扩展） |
| `author` | `mcp:{server}` | 作者标识 |
| `license` | `"MCP"` | 许可证标识 |

### 7.2 Skills API 集成

- `GET /api/v1/skills` 返回本地 Skills + MCP Skills（`source: "mcp"`）
- `GET /api/v1/skills/:id` 支持查询 MCP Skill（`id` 以 `mcp:` 开头）
- MCP Skills 的 `downloads` 映射为 `usage_count`

### 7.3 链上注册扩展点

当 `blockchain.enabled = true` 时，Gateway 启动后为每个 MCP Skill 打印链上注册日志：

```
📌 MCP skill 'mcp:filesystem/read_file' ready for on-chain registration
```

未来可在 `ChainService` 中实现 `register_skill_nft()` 完成实际链上注册。

---

## 八、Agent 集成

### 8.1 Agent 配置 MCP

```rust
let manager = MCPManager::new();
// ... 注册 clients ...

let agent = Agent::new(config)
    .with_mcp(manager)
    .with_skill_registry(skill_registry)
    .build();
```

### 8.2 任务类型路由

| TaskType | 路由 | 说明 |
|----------|------|------|
| `TaskType::SkillExecution` | `handle_skill_task` → `execute_registered_skill` | Skill ID 以 `mcp:` 开头时路由到 MCP |
| `TaskType::McpTool` | `handle_mcp_task` | 直接调用 MCP tool（legacy，仅支持 default client） |

### 8.3 执行流程

```
Agent::execute_skill_by_id("mcp:filesystem/read_file", input, params)
  └─> SkillRegistry::get("mcp:filesystem/read_file")
      └─> RegisteredSkill (wasm_path 为空)
          └─> execute_registered_skill()
              └─> parse_mcp_skill_id() → ("filesystem", "read_file")
                  └─> MCPManager::get_client("filesystem")
                      └─> MCPClient::call_tool("read_file", args)
```

---

## 九、错误码

### 9.1 MCPError

| 错误 | 说明 | 场景 |
|------|------|------|
| `ConnectionFailed` | 连接失败 | Transport 创建/连接失败 |
| `InitializationFailed` | 初始化失败 | `initialize` 握手失败 |
| `RequestFailed` | 请求失败 | JSON-RPC error response |
| `SerializationFailed` | 序列化失败 | JSON 编解码错误 |
| `ToolNotFound` | 工具不存在 | `call_tool` 时工具名错误 |
| `ResourceNotFound` | 资源不存在 | `read_resource` 时资源名错误 |
| `InvalidParams` | 参数无效 | JSON Schema 校验失败 |
| `Timeout` | 请求超时 | 超过 `timeout_ms` |
| `NotInitialized` | 未初始化 | Client 未调用 `initialize` |

### 9.2 HTTP Status Code 映射

| MCPError | HTTP Status |
|----------|-------------|
| `ConnectionFailed` | `503 Service Unavailable` |
| `ToolNotFound` / `ResourceNotFound` | `404 Not Found` |
| `InvalidParams` | `400 Bad Request` |
| `Timeout` | `504 Gateway Timeout` |
| 其他 | `500 Internal Server Error` |

---

## 十、完整示例

### 10.1 配置文件

```toml
# config/beebotos.toml

[mcp]
auto_init = true
timeout_ms = 30000
retry_count = 3
enforce_tls = true
allowed_commands = ["npx", "python3"]

[[mcp.servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/workspace"]

[[mcp.servers]]
name = "github"
transport = "http"
url = "https://mcp.github.com"
auth_token = "${GITHUB_MCP_TOKEN}"
use_sse = true
```

### 10.2 Agent 调用 MCP Skill

```rust
// Agent 执行 MCP Skill（与本地 Skill 无差别）
let result = agent
    .execute_skill_by_id(
        "mcp:filesystem/read_file",
        r#"{"path": "/workspace/data.txt"}"#,
        None,
    )
    .await?;

println!("Output: {}", result.output);
```

### 10.3 REST API 调用

```bash
# 列出 MCP servers
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/mcp/servers

# 调用工具
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"arguments": {"path": "/tmp/test.txt"}}' \
  http://localhost:8000/api/v1/mcp/servers/filesystem/tools/read_file/call

# 手动桥接（admin only）
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/mcp/bridge
```

---

## 附录 A: 相关文件索引

| 文件 | 说明 |
|------|------|
| `crates/agents/src/mcp/mod.rs` | MCP 模块入口 |
| `crates/agents/src/mcp/client.rs` | MCPClient (JSON-RPC) |
| `crates/agents/src/mcp/server.rs` | MCPServer (纯逻辑) |
| `crates/agents/src/mcp/types.rs` | 协议类型定义 |
| `crates/agents/src/mcp/transport/mod.rs` | Transport trait + Bridge |
| `crates/agents/src/mcp/transport/stdio.rs` | StdioTransport |
| `crates/agents/src/mcp/transport/http.rs` | HttpTransport |
| `crates/agents/src/mcp/skill_bridge.rs` | MCP→Skill 桥接 |
| `apps/gateway/src/config.rs` | McpConfig / McpServerConfig |
| `apps/gateway/src/handlers/http/mcp.rs` | REST API handlers |
| `apps/gateway/src/main.rs` | 启动初始化 |
| `config/beebotos.toml` | 配置示例 |

## 附录 B: 版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| 1.0.0 | 2026-04-30 | 初始版本，覆盖 Phase 1-6 完整实现 |
