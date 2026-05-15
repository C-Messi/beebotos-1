# Foreign Runtime Tool Wrapper 技术方案

## 目标

在 BeeBotOS 的 skill/tool 调用体系中新增一个专门的 tool，让 Agent 可以显式通过新增的 Python/Node.js 隔离运行时执行脚本，而不是让 `process_exec` 直接调用宿主机 `python3` 或 `node`。

新增 tool 建议命名为：

```text
foreign_runtime_exec
```

它的职责是把 tool 参数转换为 `beebotos-foreign-rt` 的 `ScriptTask`，再调用共享的 `DefaultForeignRuntimeManager` 执行。这样 skill 可以在同一个 ReAct/tool loop 中选择：

- `process_exec`：执行宿主机 shell 命令，适合轻量本地脚本和开发调试
- `foreign_runtime_exec`：执行隔离 Python/Node.js 代码，适合生产、安全边界、资源计量和未来计费

## 当前现状

当前 `crates/agents/src/skills/tool_set.rs` 中已有 `ProcessExecTool`。它通过 `sh -c <command>` 执行命令，并只保留宿主机 `PATH`，因此能运行宿主机已有的 `python3` / `node`，但不会自动使用 foreign runtime 的 WASM 模块或 process rootfs。

Foreign runtime 现在已有这些入口：

- `beebotos_foreign_rt::DefaultForeignRuntimeManager`
- `ScriptTask` / `ScriptSource` / `SandboxRequirements`
- Gateway API：`POST /api/v1/tasks/execute-script`
- Agent TaskType：`ForeignPythonWasm`、`ForeignPythonProcess`、`ForeignNodeJsWasm`、`ForeignNodeJsProcess`

缺口是：skill tool set 里还没有一个 tool 把 LLM/skill 的 tool call 显式路由到 `DefaultForeignRuntimeManager`。

## 设计原则

1. 不改变 `process_exec` 的语义，避免破坏现有 skill。
2. 新增 tool 必须显式命名为 foreign runtime，避免 LLM 混淆宿主 shell 和隔离运行时。
3. tool 参数只接受结构化脚本执行请求，不接受任意 shell command。
4. 默认禁用网络、GPU、额外文件系统映射。
5. 文件型 source 必须限制在 skill 工作目录或显式授权的 workspace 内。
6. 输出保持 JSON 优先，方便上层 agent/LLM 消费。

## Tool 接口

### Tool 名称

```text
foreign_runtime_exec
```

### Tool 描述

```text
Execute Python or Node.js code through BeeBotOS foreign runtime sandbox.
Use this instead of process_exec when the script must run in the configured
isolated Python/Node.js environment with WASM/process sandboxing, resource
limits, gas accounting, and stdout/stderr capture.
```

### 参数 Schema

```json
{
  "type": "object",
  "properties": {
    "runtime": {
      "type": "string",
      "enum": ["python", "nodejs", "node"]
    },
    "source": {
      "type": "object",
      "properties": {
        "type": {
          "type": "string",
          "enum": ["inline", "file"]
        },
        "content": {
          "type": "string"
        }
      },
      "required": ["type", "content"]
    },
    "entrypoint": {
      "type": "string",
      "default": "main"
    },
    "input": {
      "type": "object",
      "default": {}
    },
    "timeout_secs": {
      "type": "integer",
      "default": 30,
      "minimum": 1,
      "maximum": 300
    },
    "sandbox": {
      "type": "object",
      "properties": {
        "max_memory_mb": {
          "type": "integer",
          "default": 256,
          "minimum": 64,
          "maximum": 2048
        },
        "network_allowed": {
          "type": "boolean",
          "default": false
        },
        "allowed_domains": {
          "type": "array",
          "items": { "type": "string" },
          "default": []
        },
        "filesystem_paths": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "host_path": { "type": "string" },
              "guest_path": { "type": "string" },
              "read_only": { "type": "boolean", "default": true }
            },
            "required": ["host_path", "guest_path"]
          },
          "default": []
        },
        "gpu_allowed": {
          "type": "boolean",
          "default": false
        }
      }
    },
    "route_hint": {
      "type": "string",
      "enum": ["auto", "wasm", "process"],
      "default": "auto"
    }
  },
  "required": ["runtime", "source"]
}
```

### 调用示例

Python inline：

```json
{
  "runtime": "python",
  "source": {
    "type": "inline",
    "content": "import json\n\ndef main(input):\n    return {\"sum\": input[\"a\"] + input[\"b\"]}\n\nprint(json.dumps(main({\"a\": 2, \"b\": 3})))"
  },
  "input": {
    "a": 2,
    "b": 3
  },
  "sandbox": {
    "max_memory_mb": 256
  }
}
```

Node.js inline：

```json
{
  "runtime": "nodejs",
  "source": {
    "type": "inline",
    "content": "function main(input) { return { ok: true, value: input.x + 1 }; }"
  },
  "entrypoint": "main",
  "input": {
    "x": 41
  }
}
```

文件执行：

```json
{
  "runtime": "python",
  "source": {
    "type": "file",
    "content": "scripts/analyze.py"
  },
  "input": {
    "symbol": "BTC/USD"
  },
  "route_hint": "process"
}
```

## Rust 结构设计

新增文件建议：

```text
crates/agents/src/skills/foreign_runtime_tool.rs
```

核心结构：

```rust
pub struct ForeignRuntimeExecTool {
    manager: Arc<beebotos_foreign_rt::DefaultForeignRuntimeManager>,
    work_dir: PathBuf,
    agent_id: Option<String>,
}
```

实现 `SkillTool`：

```rust
#[async_trait::async_trait]
impl SkillTool for ForeignRuntimeExecTool {
    fn name(&self) -> &str {
        "foreign_runtime_exec"
    }

    fn description(&self) -> &str {
        "Execute Python or Node.js code through BeeBotOS foreign runtime sandbox..."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // 返回上面的 JSON schema
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<String, String> {
        let request = ForeignRuntimeToolRequest::try_from(params)?;
        let task = self.build_script_task(request)?;
        let result = self.manager.execute(task).await.map_err(|e| e.to_string())?;
        Ok(format_script_result(result))
    }
}
```

内部 DTO：

```rust
#[derive(Debug, serde::Deserialize)]
struct ForeignRuntimeToolRequest {
    runtime: String,
    source: ForeignRuntimeToolSource,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default)]
    input: serde_json::Value,
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
    #[serde(default)]
    sandbox: ForeignRuntimeToolSandbox,
    #[serde(default = "default_route_hint")]
    route_hint: String,
}

#[derive(Debug, serde::Deserialize)]
struct ForeignRuntimeToolSource {
    #[serde(rename = "type")]
    source_type: String,
    content: String,
}
```

## Source 处理规则

### inline

直接映射到：

```rust
ScriptSource::Inline { code }
```

### file

必须通过和 `tool_set.rs` 一致的路径边界检查：

1. 相对路径基于 skill `work_dir`
2. 绝对路径必须位于 `work_dir` 内，或者位于显式授权的 workspace 目录内
3. 禁止 `..` 逃逸
4. 转换为 `ScriptSource::File { path }`

可以复用 `tool_set.rs` 中的 `resolve_work_path()`。

## Sandbox 映射

tool 参数映射到 `SandboxRequirements`：

```rust
let mut sandbox = SandboxRequirements::default();
sandbox.max_memory_mb = request.sandbox.max_memory_mb.unwrap_or(256);
sandbox.network_allowed = request.sandbox.network_allowed.unwrap_or(false);
sandbox.allowed_domains = request.sandbox.allowed_domains;
sandbox.gpu_allowed = request.sandbox.gpu_allowed.unwrap_or(false);
sandbox.filesystem_paths = checked_path_mappings;
sandbox.max_cpu_time_ms = request.timeout_secs * 1000;
```

默认策略：

- `network_allowed = false`
- `gpu_allowed = false`
- `filesystem_paths = []`
- `max_memory_mb = 256`
- `timeout_secs = 30`

超过上限时直接拒绝，不静默放大。

## Route Hint

当前 `DefaultForeignRuntimeManager::execute()` 内部使用 `RouteHints::default()`，还没有从 `ScriptTask` 传入 route hint。因此第一阶段可以：

- `route_hint = "auto"`：正常执行
- `route_hint = "wasm"`：执行前校验 `manager.is_wasm_available(runtime)`，不可用则报错
- `route_hint = "process"`：执行前校验 `manager.is_process_available(runtime)`，但实际强制 process 需要扩展 foreign-rt

建议第二阶段在 `beebotos-foreign-rt` 增加：

```rust
pub async fn execute_with_hints(
    &self,
    task: ScriptTask,
    hints: RouteHints,
) -> Result<ScriptResult>
```

然后 `foreign_runtime_exec` 可以真正支持 `route_hint = "wasm" | "process"`。

## 输出格式

tool 返回字符串，但内容建议统一为 JSON 字符串，便于 LLM 和上层系统解析：

```json
{
  "success": true,
  "output": { "sum": 5 },
  "execution_time_ms": 42,
  "gas_used": {
    "compute": 123,
    "memory": 456,
    "io": 0,
    "network": 0,
    "storage": 0,
    "total": 579
  },
  "logs": [
    {
      "level": "info",
      "message": "..."
    }
  ],
  "runtime": "python"
}
```

失败时：

```json
{
  "success": false,
  "error": "Process exited with code Some(1). stderr: ...",
  "output": null,
  "execution_time_ms": 42,
  "logs": []
}
```

## 注册方式

### 新增带 foreign runtime 的 tool set 构造器

在 `crates/agents/src/skills/tool_set.rs` 增加：

```rust
pub fn tool_set_with_foreign_runtime(
    work_dir: &Path,
    manager: Arc<beebotos_foreign_rt::DefaultForeignRuntimeManager>,
    agent_id: Option<String>,
) -> HashMap<String, Box<dyn SkillTool>> {
    let mut tools = default_tool_set(work_dir);
    tools.insert(
        "foreign_runtime_exec".to_string(),
        Box::new(ForeignRuntimeExecTool::new(
            manager,
            work_dir.to_path_buf(),
            agent_id,
        )),
    );
    tools
}
```

### Agent 集成

当前 `Agent` 已有：

```rust
foreign_rt_manager: Option<Arc<DefaultForeignRuntimeManager>>
```

并且有：

```rust
with_foreign_rt_manager(...)
```

在 `CodeSkillExecutor` 或 Agent skill 执行入口选择 tool set 时：

- 如果 agent 有 `foreign_rt_manager`，使用 `tool_set_with_foreign_runtime`
- 如果没有，继续使用 `default_tool_set`

为了避免 `CodeSkillExecutor` 只持有 LLM，建议增加可选 manager：

```rust
pub struct CodeSkillExecutor {
    llm: Arc<dyn LLMCallInterface>,
    foreign_rt_manager: Option<Arc<DefaultForeignRuntimeManager>>,
    agent_id: Option<String>,
}
```

或者不改 `CodeSkillExecutor`，在更高层构造 `UnifiedReActExecutor` 时传入带 foreign runtime 的 tools。

## Gateway 配置要求

`apps/gateway/src/main.rs` 当前使用：

```rust
ForeignRuntimeConfig::default()
```

默认配置不会设置：

- `wasm.pyodide_module_path`
- `wasm.quickjs_module_path`
- `process.python_rootfs`
- `process.nodejs_rootfs`

因此在实现 tool 前，需要先让 Gateway/Agent 能注入真实配置。建议新增配置项：

```toml
[foreign_runtime]
enabled = true

[foreign_runtime.wasm]
pyodide_module_path = "/opt/beebotos/wasm-modules/pyodide.asm.wasm"
quickjs_module_path = "/opt/beebotos/wasm-modules/qjs.wasm"
max_memory_mb = 512

[foreign_runtime.process]
python_rootfs = "/var/lib/beebotos/rootfs/python"
nodejs_rootfs = "/var/lib/beebotos/rootfs/nodejs"
max_process_slots = 10
```

然后在 Gateway 启动时从全局配置构造 `ForeignRuntimeConfig`。

## 权限与安全

### Capability

新增 tool 应要求至少具备：

```text
ForeignRuntimeBasic
```

当请求 process path、网络、GPU、额外文件映射时，需要更高能力：

- `route_hint = process`：`ForeignRuntimeProcess`
- `network_allowed = true`：`ForeignRuntimeNetwork`
- `gpu_allowed = true`：`ForeignRuntimeGPU`
- `filesystem_paths` 非空：`ForeignRuntimePrivileged` 或明确 allowlist

第一阶段如果 capability 检查还没有完整贯通，可以在 tool 内做保守拒绝：

- 默认拒绝 `network_allowed = true`
- 默认拒绝 `gpu_allowed = true`
- 默认拒绝任意 `filesystem_paths`
- 只允许 skill `work_dir` 下的 file source

### 与 `process_exec` 的区别

`process_exec`：

- 输入是 shell command
- 依赖宿主机 PATH
- 适合本地工具和开发脚本

`foreign_runtime_exec`：

- 输入是结构化脚本任务
- 不接受 shell command
- 通过 `DefaultForeignRuntimeManager`
- 使用 WASM/process sandbox、gas、cgroup、stdout/stderr 捕获

## 实施步骤

1. 新建 `crates/agents/src/skills/foreign_runtime_tool.rs`
2. 实现 `ForeignRuntimeExecTool`
3. 在 `crates/agents/src/skills/mod.rs` 导出该 tool
4. 在 `tool_set.rs` 增加 `tool_set_with_foreign_runtime`
5. 调整 Agent/CodeSkillExecutor 构造路径，在有 `foreign_rt_manager` 时注册 `foreign_runtime_exec`
6. 给 Gateway 增加 foreign runtime 配置读取，替代 `ForeignRuntimeConfig::default()`
7. 增加单元测试：参数解析、路径限制、runtime 映射、sandbox 映射
8. 增加集成测试：Python inline、Node.js inline、file source、runtime unavailable
9. 更新 manual testing guide，给出 curl/API 和 skill tool 调用示例

## 测试计划

### 单元测试

文件：

```text
crates/agents/src/skills/foreign_runtime_tool.rs
```

测试项：

- `runtime = "python"` 映射到 `ForeignRuntime::Python`
- `runtime = "node" | "nodejs"` 映射到 `ForeignRuntime::NodeJs`
- `source.type = "inline"` 映射为 `ScriptSource::Inline`
- `source.type = "file"` 不允许逃逸 `work_dir`
- `timeout_secs` 超过上限会被拒绝
- `network_allowed = true` 在无 capability 时被拒绝

### 集成测试

准备：

```bash
mkdir -p /opt/beebotos/wasm-modules
curl -L -o /opt/beebotos/wasm-modules/qjs.wasm \
  https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-wasi.wasm
```

然后验证：

```json
{
  "runtime": "nodejs",
  "source": {
    "type": "inline",
    "content": "function main(input) { return { value: input.x + 1 }; }"
  },
  "input": { "x": 41 }
}
```

预期：

```json
{
  "success": true,
  "output": {
    "value": 42
  }
}
```

### 回归测试

确认现有 `process_exec` 行为不变：

```json
{
  "command": "python3 script.py",
  "timeout_ms": 30000
}
```

仍然走宿主 shell，不走 foreign runtime。

## 风险与后续优化

1. `route_hint` 目前无法真正强制传入 foreign-rt router，需要新增 `execute_with_hints`。
2. Pyodide 的执行模型与普通 Python 文件执行不同，可能需要进一步包装 stdin/input。
3. QuickJS WASI command 需要确认 stdout JSON 契约，否则 tool 输出解析会失败。
4. process path 当前 `unshare` fallback 未真正 chroot 到 rootfs，需要继续完善 nsjail/rootfs 挂载。
5. capability 检查需要和 Agent 权限系统贯通，否则第一阶段应默认保守拒绝高风险能力。

## 最小可交付版本

MVP 范围：

- 支持 `runtime = python | nodejs`
- 支持 `source.type = inline`
- 默认 `route_hint = auto`
- 默认禁用网络/GPU/文件映射
- 注册到有 `foreign_rt_manager` 的 Agent tool set
- 输出 JSON 字符串

不进入 MVP：

- 强制 process/wasm route
- 网络 allowlist
- GPU
- 任意目录挂载
- prebuilt module

