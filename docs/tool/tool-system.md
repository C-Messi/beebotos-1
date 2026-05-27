# BeeBotOS Tool System

本文档说明当前 Agent tool system 的运行方式，重点覆盖默认 toolset、native tool loop、MCP tool search mode，以及 skill 与 MCP tool 的边界。

## Design Goals

BeeBotOS 的 tool system 采用分层暴露策略：

1. 默认工具保持稳定、直接注册到 native toolset。
2. MCP 工具不再桥接成 skill，也不在启动时把完整 schema 全量注入给 LLM。
3. MCP 只先暴露一个内部搜索工具 `mcp_tool_search`。
4. LLM 搜索到目标 MCP tool 后，运行时再动态加载对应 tool schema，并加入下一轮 native toolset。

这样可以减少上下文膨胀，避免 MCP tool 以 skill 形式绕过 tool calling，同时让 LLM 真正能调用 MCP tool，而不是只在 prompt 中看到说明。

## Tool Categories

### Default Workspace Toolset

默认 workspace 工具由 Agent 直接构建并注册到 native toolset。它们是稳定内置工具，不经过 MCP search mode。

当前基础工具包括：

| Tool | Purpose |
| --- | --- |
| `read_file` | 读取 workspace 内 UTF-8 文本文件 |
| `list_dir` | 列出 workspace 内目录内容 |
| `glob` | 按 glob 查找文件 |
| `grep` | 按正则搜索文本 |
| `create_cron_job` | 创建 Gateway 管理的定时任务 |

受控写入/执行工具默认可用，可通过 `BEEBOTOS_ENABLE_CONTROLLED_WORKSPACE_TOOLS=0` 关闭：

| Tool | Purpose |
| --- | --- |
| `write_file` | 写入 UTF-8 文本文件 |
| `edit_file` | 替换文本内容 |
| `exec` | 在受控工作目录执行 shell 命令 |

实现位置：

- `crates/agents/src/agent_impl.rs`
- `Agent::builtin_workspace_tools()`
- `Agent::controlled_workspace_tool_definitions()`

### ReAct Toolset

ReAct toolset 在 workspace toolset 之上增加会话级工具。

| Tool | Purpose |
| --- | --- |
| `activate_skill` | 激活 skill context pack，把 skill 文档注入后续上下文 |
| `session_search` | 搜索同一用户历史会话，只有配置了 system info provider 时暴露 |
| `web_fetch` | 获取公开 HTTP/HTTPS 页面文本 |
| `web_search` | 搜索实时或易变信息 |
| `mcp_tool_search` | 搜索 MCP 工具，并触发动态 schema 加载 |

实现位置：

- `Agent::builtin_react_tools()`
- `Agent::add_mcp_tool_search_if_available()`
- `Agent::execute_react_tool_call()`

`mcp_tool_search` 只在 Agent 配置了 `MCPManager` 时加入 toolset。

### Skills Are Context Packs

当前设计中，skill 与 tool 有明确边界：

- skill 是 context pack 或本地执行能力。
- skill 可以通过 `activate_skill` 注入上下文。
- MCP tool 不再注册为 skill。
- skill catalog 会过滤 `mcp:` 开头的历史条目。

这意味着 LLM 不能再通过 `SKILL:mcp:server/tool` 或 `skill_call` 方式调用 MCP。MCP 必须走 `mcp_tool_search`。

## Native Tool Loop

Agent 的 native tool loop 采用多轮结构：

1. 构造 messages。
2. 构造当前可用 `Vec<ToolDefinition>`。
3. 调用 `LLMCallInterface::call_llm_tool_turn()`。
4. 如果 LLM 返回 tool calls，Agent 执行工具并把 tool result 追加回 messages。
5. 如有必要更新 toolset，再进入下一轮。
6. 如果 LLM 不再返回 tool calls，则该轮 content 作为最终回复。

默认工具在第一轮就可用；MCP 具体工具通常在 `mcp_tool_search` 之后的下一轮才可用。

核心类型：

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
```

核心执行入口：

- `Agent::execute_native_tool_call()`
- `Agent::execute_react_tool_call()`
- `Agent::force_final_react_answer()`

## MCP Tool Search Mode

MCP search mode 参考 Claude Code 的分阶段工具发现模式。

### Stage 1: Startup

启动时只初始化 MCP clients：

- 连接 stdio 或 HTTP MCP server。
- `MCPManager::initialize_all()` 初始化连接。
- 不调用 MCP Skill Bridge。
- 不把 MCP tool schema 注册到 SkillRegistry。
- 不把全部 MCP tool schema 注入 LLM。

Gateway 启动位置：

- `apps/gateway/src/main.rs`

### Stage 2: Expose Search Tool

如果 Agent 有 `MCPManager`，toolset 里加入：

```text
mcp_tool_search
```

参数：

```json
{
  "query": "natural-language capability search",
  "server": "optional MCP server name",
  "limit": 5
}
```

这个工具只返回轻量匹配结果，包括 server、tool name、description 和动态 tool name，不返回完整 schema。

### Stage 3: Search MCP Catalog

`mcp_tool_search` 通过 `MCPManager::list_tool_summaries()` 获取轻量目录。

`list_tool_summaries()` 会调用 MCP `tools/list`，但只保留：

- `server_name`
- `tool_name`
- `description`

schema 不会直接暴露给 LLM。

### Stage 4: Dynamic Schema Load

当 LLM 调用 `mcp_tool_search` 后，Agent 会用同一组搜索参数再计算命中目标，并调用：

```rust
MCPManager::get_tool_schema(server_name, tool_name)
```

然后将命中的 MCP tool 转换为 `ToolDefinition`，追加到当前 native toolset。

动态 MCP tool name 使用安全编码格式：

```text
mcp__<base64url(server:tool)>
```

这样可以避免 provider 对 tool name 中冒号、斜杠等字符的限制。

### Stage 5: Execute MCP Tool

下一轮 LLM 可以调用动态注入的 MCP tool。Agent 会：

1. 反解 `mcp__...` 得到 `server_name` 与 `tool_name`。
2. 将 tool arguments 转成 JSON object。
3. 加默认参数，例如部分 Alpaca tool 的 `loc`、`type`、`time_in_force`。
4. 用 schema 做参数校验。
5. 对高风险 MCP tool 走 approval gate。
6. 调用 `MCPClient::call_tool()`.
7. 截断过大的输出并返回 tool result。

执行入口：

- `Agent::execute_mcp_dynamic_tool()`

## MCP And Approval

MCP 高风险工具仍然受 approval gate 保护。高风险识别目前基于 tool id 关键字，例如：

- `_order`
- `place_order`
- `cancel_order`
- `_transfer`
- `_withdraw`
- `_delete`
- buy/sell 相关前后缀

只读 MCP 查询工具不会因为 MCP 身份自动触发审批。是否审批由高风险判断和 approval rule 决定。

## What Was Removed

旧设计中，MCP tools 会通过 `McpSkillBridge` 注册成：

```text
mcp:{server}/{tool}
```

这个设计已经从主路径移除：

- Gateway 启动时不再执行 MCP -> SkillRegistry bridge。
- `/api/v1/mcp/bridge` 保留为 legacy endpoint，只返回说明，不再注册 skill。
- skill 列表 API 不再把 MCP tools 合并为 skills。
- skill selector 召回会过滤 `mcp:` skill。
- Agent skill catalog 会过滤 `mcp:` skill。
- 执行 `mcp:` skill 会返回提示，要求使用 `mcp_tool_search`。

`crates/agents/src/mcp/skill_bridge.rs` 中仍保留部分通用辅助函数，例如 schema validation；这不是 MCP-as-skill 主路径。

## Adding A New Default Tool

如果要新增默认工具：

1. 在 `Agent::builtin_workspace_tools()` 或 `Agent::builtin_react_tools()` 添加 `ToolDefinition`。
2. 在 `Agent::execute_builtin_workspace_tool()` 或 `Agent::execute_react_tool_call()` 添加执行逻辑。
3. 如果工具涉及写入、命令执行或外部副作用，接入现有权限/审批策略。
4. 保持参数 schema 小而明确，避免把业务文档塞进 description。

默认工具应当是稳定、通用、Agent 自身提供的能力。

## Adding A New MCP Server

如果要新增 MCP server：

1. 在 gateway MCP 配置中添加 server。
2. 确保启动时 `MCPManager` 能注册并初始化 client。
3. 不需要写 skill。
4. 不需要注册 tool schema 到 SkillRegistry。
5. LLM 会通过 `mcp_tool_search` 搜到对应 tool，并由 runtime 按需加载 schema。

MCP server 自己提供的 `tools/list` description 会直接影响搜索命中质量，因此 description 应该简短、可检索，并包含关键业务词。

## Runtime Invariants

当前 tool system 应保持以下不变量：

- 默认 toolset 不依赖 MCP。
- MCP tool 不进入 SkillRegistry。
- MCP tool schema 不在启动时全量注入 LLM。
- LLM 必须先调用 `mcp_tool_search` 才能获得具体 MCP tool schema。
- 动态 MCP tool 只在当前 native tool loop 中临时加入 toolset。
- 高风险 MCP tool 必须走 approval gate。
- skill catalog 是 context catalog，不是 MCP tool catalog。

## Known Follow-Ups

部分历史文档和演进记录仍提到 MCP Skill Bridge 或 `mcp:{server}/{tool}` skill id。这些内容代表旧设计或历史方案，不应作为当前实现依据。当前实现以本文档和以下代码路径为准：

- `crates/agents/src/agent_impl.rs`
- `crates/agents/src/mcp/mod.rs`
- `apps/gateway/src/main.rs`
- `apps/gateway/src/handlers/http/mcp.rs`
- `apps/gateway/src/handlers/http/skills.rs`
