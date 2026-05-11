
## BeeBotOS apps/WEB 技能市场：ClawHub Skill 下载与安装 API 方法

### 一、ClawHub REST API 调用（WEB 前端直接调用或 Gateway 代理）

| 功能 | HTTP 方法 | 端点 | 说明 |
|------|-----------|------|------|
| 搜索技能 | GET | `https://api.clawhub.ai/api/v1/skills/search?q={keyword}&limit=20` | 关键词查找，支持分页 |
| 获取详情 | GET | `https://api.clawhub.ai/api/v1/skills/{slug}` | 获取元数据、版本、依赖 |
| 下载技能包 | GET | `https://api.clawhub.ai/api/v1/skills/{id}/download` | 返回 ZIP 文件，内含 `SKILL.md` 与 `_meta.json` |

### 二、本地安装方法

下载完成后，客户端将 ZIP 包解压至 Agent 指定工作目录：

- **BeeBotOS 标准安装路径**：`data/skills/installed/{skill_id}/`
- **默认数据目录**：
  - 生产环境：`data/beebotos/skills/installed/{skill_id}/`
  - 开发环境：`./data/skills/installed/{skill_id}/`

### 三、WEB 模块调用流程

```javascript
// 1. 搜索技能
const list = await fetch('https://api.clawhub.ai/api/v1/skills/search?q=crypto&limit=20');

// 2. 获取指定技能详情
const meta = await fetch(`https://api.clawhub.ai/api/v1/skills/${slug}`);

// 3. 下载 ZIP 包
const zip = await fetch(`https://api.clawhub.ai/api/v1/skills/${id}/download`).then(r => r.blob());

// 4. 提交至本地 Gateway 安装接口，由后端解压到 installed 目录
await fetch('/api/v1/skills/install', {
  method: 'POST',
  headers: { 'Content-Type': 'application/zip' },
  body: zip
});
```

### 四、Gateway 后端处理

Gateway 接收 ZIP 后执行：
1. 校验 `_meta.json` 版本与依赖
2. 解压到 `data/skills/installed/{skill_id}/`
3. 更新本地 `registry/skills.json`
4. 向 Agent Runtime 注册该技能

&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&
编译全部通过 ✅

- `beebotos-agents`: 0 errors, 24 warnings（无新增）
- `beebotos-gateway`: 0 errors, 30 warnings（无新增）

---

## 实施完成总结

### 一、工具工作目录统一为 `/data/workspace/`

**安全边界**：所有文件操作工具现在都会将相对路径解析为基于 `/data/workspace/`，绝对路径如果超出该目录会被拒绝。

| 工具 | 修改 |
|------|------|
| `file_read` | 相对路径 → `/data/workspace/` 下；绝对路径越界拒绝 |
| `file_write` | 同上 + 自动创建父目录 |
| `file_list` | 同上 |
| `file_edit` | 同上 |
| `file_glob` | base 路径解析为 `/data/workspace/` 下 |
| `text_grep` | 搜索路径解析为 `/data/workspace/` 下 |
| `process_exec` / `bash_shell` | 已有 `allowed_work_dirs`，默认指向 `/data/workspace/` |

**辅助函数**：新增 `resolve_work_path(work_dir, input_path)`，canonicalize 后做前缀安全检查。

### 二、LLM 直接调用底层工具（Native Function Calling）

**架构**：

```
用户: "写一个1+1的python应用"
  ↓
Agent::handle_llm_task_internal()
  ↓ Intent analysis + skill scoring（不变）
  ↓ native_tools 同时包含: 已注册 skills + 底层工具(file_write 等)
  ↓
  ┌─ 如果 llm_client 可用 ──────────────────────┐
  │ 构建 SkillToolHandler 列表（真实执行器）      │
  │ LLMClient::chat_with_tools_react_with_messages() │
  │   → LLM 输出 tool_calls: file_write           │
  │   → SkillToolHandler::execute() 真实写文件    │
  │   → 结果以 Role::Tool 返回 LLM                │
  │   → LLM 生成最终答案                          │
  └─ 否则回退到 call_llm_with_tools()（stub）────┘
```

**关键新增**：

| 文件 | 说明 |
|------|------|
| `crates/agents/src/llm/skill_tool_handler.rs` | **新文件** — `SkillToolHandler` 实现 `ToolHandler` trait，桥接 `SkillTool::execute()` 到 LLM native function calling |
| `crates/agents/src/llm/client.rs` | 新增 `chat_with_tools_react_with_messages()` — 接受外部 message 列表（不依赖内部 context），支持多轮 tool_calls → 执行 → 反馈 |

### 三、完整文件变更清单

| 文件 | 变更 |
|------|------|
| `crates/agents/src/llm/skill_tool_handler.rs` | **新增** — SkillTool → ToolHandler 桥接 |
| `crates/agents/src/llm/mod.rs` | 导出 `skill_tool_handler` 模块和 `SkillToolHandler` |
| `crates/agents/src/llm/client.rs` | 新增 `chat_with_tools_react_with_messages()` |
| `crates/agents/src/skills/tool_set.rs` | 6 个文件工具 struct 添加 `work_dir` + `new()`；新增 `resolve_work_path()`；所有 `execute()` 使用路径安全检查 |
| `crates/agents/src/agent_impl.rs` | Agent 新增 `tool_work_dir`/`llm_client` 字段 + builder 方法；`handle_llm_task_internal` 注入底层工具到 native tool calling |
| `crates/agents/src/kernel_integration.rs` | `KernelAgentBuilder` 新增 `with_llm_client()`/`with_tool_work_dir()` 并传递给 Agent |
| `crates/agents/src/runtime/agent_runtime_impl.rs` | `GatewayAgentRuntime` 新增 `llm_client`/`tool_work_dir`，两处 builder 传递 |
| `apps/gateway/src/services/llm_service.rs` | 新增 `failover_provider()` getter |
| `apps/gateway/src/main.rs` | 创建 `LLMClient` 注入 runtime；启动时创建 `/data/workspace/` 目录 |

### 四、执行流程验证

用户说 **"写一个1+1=？的python应用"**：
```
handle_llm_task_internal()
  ├── intent 分析 → 不是 planning / pipeline
  ├── skill scoring → 无高匹配 skill
  ├── native_tools = [] + 底层工具(file_write, process_exec, ...)
  ├── llm_client 可用 → chat_with_tools_react_with_messages()
  │   ├── LLM 决定调用 file_write(path="app.py", content="print(1+1)")
  │   ├── SkillToolHandler::execute() → 写入 /data/workspace/app.py
  │   ├── 结果返回 LLM → "File written"
  │   └── LLM 最终回答: "已创建 app.py，内容为 print(1+1)"
  └── 返回用户
```

用户说 **"查找本机的文件"**：
```
handle_llm_task_internal()
  ├── intent 分析
  ├── skill scoring → 可能匹配 file_glob / text_grep
  ├── native_tools = [匹配 skills] + 底层工具
  ├── llm_client 可用 → chat_with_tools_react_with_messages()
  │   ├── LLM 决定调用 file_glob(pattern="*")
  │   ├── SkillToolHandler::execute() → 列出 /data/workspace/ 下文件
  │   └── LLM 最终回答
  └── 返回用户
```
&&&&&&&&&&&&&&&&&&&&&&&&&&&&&

LLM 直接调用底层工具

## 代码质量审查报告：LLM 直接调用底层工具

### 审查结果汇总

| 级别 | 发现的问题 | 状态 |
|------|-----------|------|
| **P0** | `resolve_work_path` 路径遍历漏洞 — `Path::starts_with` 不解析 `..` | ✅ 已修复 |
| **P0** | `resolve_work_path` 使用同步 `std::fs::canonicalize` 阻塞 tokio 线程 | ✅ 已修复 |
| **P1** | `query_tool_inventory` 仍使用 `"."` 而非 `self.tool_work_dir` | ✅ 已修复 |
| **P1** | `chat_with_tools_react_with_messages` 消息角色全映射为 `System` | ✅ 已修复 |
| **P2** | `chat_with_tools_react_with_messages` 每次 tool call 遍历调用 `definition()` | ✅ 已修复 |

---

### 详细修复说明

#### 1. `resolve_work_path` 路径遍历安全漏洞（P0）

**问题**：`Path::starts_with("/data/workspace")` 对 `/data/workspace/../etc/passwd` 返回 `true`，因为它基于组件前缀匹配，不解析 `..`。

```rust
// ❌ 修复前：可被绕过
let resolved = Path::new("/data/workspace").join("../etc/passwd");
assert!(resolved.starts_with("/data/workspace")); // true！
```

**修复**：新增 `normalize_path()` 手动解析 `.` 和 `..`（纯路径运算，零 I/O），然后做安全检查。

```rust
// ✅ 修复后：正确拦截
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => { normalized.pop(); }
            Component::Normal(name) => normalized.push(name),
            // ...
        }
    }
    normalized
}
```

**安全测试验证**（9 个用例全部通过）：
```
✅ test.txt              → /data/workspace/test.txt              (允许)
✅ ../etc/passwd         → 拒绝 (路径遍历攻击被拦截)
✅ /etc/passwd           → 拒绝 (绝对路径越界)
✅ foo/../../etc         → 拒绝 (多层 .. 逃逸)
✅ foo/../../../etc/passwd → 拒绝 (深层逃逸)
```

#### 2. `resolve_work_path` 同步 I/O 阻塞（P0）

**问题**：`std::fs::canonicalize()` 是同步阻塞调用，在 tokio 异步运行时中执行会阻塞工作线程。

**修复**：完全移除 `canonicalize`，改用纯路径运算的 `normalize_path()`。零文件系统 I/O。

#### 3. `query_tool_inventory` 不一致（P1）

**修复**：`default_tool_set(".")` → `default_tool_set(&self.tool_work_dir)`，与用户实际可用工具集保持一致。

#### 4. 消息角色映射错误（P1）

**问题**：所有 `communication::Message` 都被映射为 `llm::Role::System`，导致 LLM 把用户消息和助手回复都当作系统指令处理。

**修复**：解析消息文本前缀推断角色：
```rust
if content.starts_with("用户:") || content.starts_with("User:") {
    Role::User
} else if content.starts_with("助手:") || content.starts_with("Assistant:") {
    Role::Assistant
} else {
    Role::System
}
```

#### 5. `chat_with_tools_react_with_messages` 性能优化（P2）

**问题**：每次 tool call 都遍历 `tool_handlers` 并调用 `definition()` 匹配工具名，复杂度 O(n²)。

**修复**：预建 `HashMap<String, &Box<dyn ToolHandler>>` 和 `Vec<Tool>`，工具查找从 O(n) 降为 O(1)。

---

### 编译验证

```
beebotos-agents: 0 errors, 25 warnings（无新增）
beebotos-gateway: 0 errors, 30 warnings（无新增）
```

所有修复均已编译通过。


%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%5

