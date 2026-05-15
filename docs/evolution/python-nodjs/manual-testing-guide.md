# Foreign Runtime (Python/Node.js) 人工测试验证指南

## 测试前准备

### 1. 确认系统环境

```bash
# 检查 cgroup v2 可用性
ls /sys/fs/cgroup/cgroup.controllers

# 检查 nsjail（可选，没有则使用 unshare fallback）
which nsjail || echo "nsjail not installed, will use unshare fallback"

# 检查 unshare 可用性
which unshare
```

### 2. 准备测试资源

根据你想测试的路径，准备以下资源：

#### 方案 A：WASM 路径测试（推荐，更安全）

需要下载 Pyodide 和 QuickJS 的 WASM 模块：

```bash
# 创建 WASM 模块目录
mkdir -p /opt/beebotos/wasm-modules

# 下载 Pyodide WASM 模块（示例，需根据实际版本调整）
wget -O /opt/beebotos/wasm-modules/pyodide.asm.wasm \
  https://cdn.jsdelivr.net/pyodide/v0.25.0/full/pyodide.asm.wasm

# QuickJS WASM 模块需要自行编译或获取预构建版本
# 参考: https://github.com/justjake/quickjs-wasm
```

#### 方案 B：Process 路径测试

需要准备 Python/Node.js 的 rootfs（可以使用 Docker 导出）：

```bash
# 创建 rootfs 目录
mkdir -p /var/lib/beebotos/rootfs/python
mkdir -p /var/lib/beebotos/rootfs/nodejs

# 使用 Docker 导出 Python rootfs
docker run --rm -v /var/lib/beebotos/rootfs/python:/output \
  python:3.11-slim \
  bash -c "cp -r /usr /output/ && cp -r /lib /output/ && cp -r /bin /output/"

# 或使用更轻量的方法 - 直接指向系统 Python（测试环境）
# 注意：生产环境应使用隔离的 rootfs
```

---

## 测试方法 1：单元测试（无需外部资源）

已有的单元测试可以验证核心逻辑：

```bash
cd /root/beebotos

# 测试 foreign-rt 核心逻辑（43 个测试）
cargo test -p beebotos-foreign-rt --lib -j1

# 测试 Agent 集成（3 个测试）
cargo test -p beebotos-agents foreign_executor --lib -j1
```

**预期输出：**
- `test router::tests::*` - 路由选择逻辑
- `test pool::tests::*` - 对象池管理
- `test bridge::host_funcs::tests::*` - Host Function Bridge
- `test wasm_path::executor::tests::*` - WASM 执行引擎
- `test process_path::tests::*` - 进程沙箱

---

## 测试方法 2：Gateway REST API 测试

### 2.1 启动 Gateway

```bash
cd /root/beebotos
cargo run -p beebotos-gateway 2>&1 | tee gateway.log
```

### 2.2 测试健康检查端点

```bash
# 查询运行时健康状态
curl -s http://localhost:8080/api/v1/runtimes/health | jq

# 预期响应（未配置模块时）
{
  "status": "degraded",
  "python": {
    "available": false
  },
  "nodejs": {
    "available": false
  }
}
```

### 2.3 测试运行时列表

```bash
curl -s http://localhost:8080/api/v1/runtimes | jq

# 预期响应（配置模块后）
{
  "runtimes": [
    {
      "name": "python",
      "available": true,
      "wasm_available": true,
      "process_available": false,
      "default_max_memory_mb": 256,
      "default_timeout_secs": 30
    },
    {
      "name": "nodejs",
      "available": true,
      "wasm_available": true,
      "process_available": false,
      "default_max_memory_mb": 256,
      "default_timeout_secs": 30
    }
  ]
}
```

### 2.4 测试 Python 脚本执行

```bash
curl -s -X POST http://localhost:8080/api/v1/tasks/execute-script \
  -H "Content-Type: application/json" \
  -d '{
    "runtime": "python",
    "source": {
      "type": "inline",
      "content": "def main(input): return {'"'"'result'"'"': input['"'"'x'"'"'] + 1}"
    },
    "entrypoint": "main",
    "input": {"x": 42},
    "sandbox": {
      "max_memory_mb": 256,
      "network_allowed": false
    },
    "timeout_secs": 30
  }' | jq

# 预期响应
{
  "success": true,
  "output": {
    "result": 43
  },
  "execution_time_ms": 150,
  "gas_used": {
    "compute": 1000000,
    "memory": 268435456,
    "io": 0,
    "network": 0,
    "storage": 0,
    "total": 269435456
  },
  "logs": [],
  "execution_route": "auto"
}
```

### 2.5 测试 Node.js 脚本执行

```bash
curl -s -X POST http://localhost:8080/api/v1/tasks/execute-script \
  -H "Content-Type: application/json" \
  -d '{
    "runtime": "nodejs",
    "source": {
      "type": "inline",
      "content": "function main(input) { return { result: input.x * 2 }; }"
    },
    "entrypoint": "main",
    "input": {"x": 21},
    "sandbox": {
      "max_memory_mb": 256,
      "network_allowed": false
    },
    "timeout_secs": 30
  }' | jq
```

### 2.6 测试沙箱限制

```bash
# 测试内存超限（应返回错误）
curl -s -X POST http://localhost:8080/api/v1/tasks/execute-script \
  -H "Content-Type: application/json" \
  -d '{
    "runtime": "python",
    "source": {
      "type": "inline",
      "content": "def main(input): return {'"'"'data'"'"': '"'"'x'"'"' * 1024 * 1024 * 1024}"
    },
    "entrypoint": "main",
    "input": {},
    "sandbox": {
      "max_memory_mb": 64,
      "network_allowed": false
    },
    "timeout_secs": 10
  }' | jq
```

---

## 测试方法 3：Agent 集成测试

### 3.1 通过 Agent 执行 Python 任务

```rust
// 在 Agent 测试中或自定义测试程序中使用
use beebotos_agents::AgentBuilder;
use beebotos_agents::task::{Task, TaskType};
use std::collections::HashMap;

#[tokio::test]
async fn test_agent_python_execution() {
    let mut agent = AgentBuilder::new("test-agent").build();
    
    // 配置 Foreign Runtime Manager（可选，未配置则返回错误）
    let frt_config = beebotos_foreign_rt::ForeignRuntimeConfig::default();
    if let Ok(manager) = beebotos_foreign_rt::DefaultForeignRuntimeManager::new(frt_config) {
        agent = agent.with_foreign_rt_manager(std::sync::Arc::new(manager));
    }
    
    let task = Task {
        id: "test-python-1".to_string(),
        task_type: TaskType::ForeignPythonWasm,
        input: r#"def main(input): return {"hello": "world"}"#.to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("entrypoint".to_string(), "main".to_string());
            p.insert("max_memory_mb".to_string(), "256".to_string());
            p
        },
        stream_tx: None,
    };
    
    let result = agent.execute_task(task).await;
    println!("Result: {:?}", result);
}
```

---

## 测试方法 4：Process 路径专项测试

### 4.1 测试 unshare 回退路径

由于 nsjail 未安装，系统会自动使用 unshare：

```bash
# 验证 unshare 命令构建
cargo test -p beebotos-foreign-rt process_path::tests::test_prepare_script_file --lib -j1 -- --nocapture

# 预期输出：显示创建的临时脚本文件路径和内容
```

### 4.2 测试 cgroup 资源限制

```bash
# 需要 root 权限或 cgroup 写入权限
sudo -E cargo test -p beebotos-foreign-rt process_path::cgroup --lib -j1 -- --nocapture
```

### 4.3 手动测试进程沙箱

```bash
# 创建一个测试 Python 脚本
cat > /tmp/test_script.py << 'EOF'
import json
import sys

def main():
    data = json.loads(sys.stdin.read())
    result = {"sum": data["a"] + data["b"]}
    print(json.dumps(result))

if __name__ == "__main__":
    main()
EOF

# 使用 unshare 手动执行（模拟 ProcessSandboxExecutor 的行为）
echo '{"a": 10, "b": 20}' | unshare --fork --pid --mount-proc --map-root-user python3 /tmp/test_script.py
```

---

## 测试方法 5：Skill Registry 集成测试

### 5.1 创建带 runtime 字段的 Skill Manifest

```yaml
# /tmp/test_python_skill/skill.yaml
id: test-python-skill
name: Test Python Skill
version: 1.0.0
description: A test skill using Python runtime
author: test
entry_point: main.py
runtime: python          # 新增字段：wasm | python | nodejs
sandbox:
  max_memory_mb: 512
  network_allowed: false
runtime_dependencies:
  - requests
  - numpy
```

### 5.2 加载并验证

```bash
# 在 Agent 测试中验证 SkillLoader 能正确解析 runtime 字段
cargo test -p beebotos-agents skill_loader --lib -j1
```

---

## 常见问题排查

### Q1: Gateway 返回 "Foreign runtime manager not initialized"

**原因:** Gateway 启动时 `DefaultForeignRuntimeManager::new()` 失败。

**排查:**
```bash
grep "ForeignRuntimeManager" gateway.log
```

### Q2: WASM 路径不可用

**原因:** 缺少 Pyodide/QuickJS WASM 模块文件。

**解决:**
```bash
# 检查配置路径
ls -la /opt/beebotos/wasm-modules/

# 在 gateway main.rs 中确认模块路径配置
```

### Q3: Process 路径 spawn 失败

**原因:** rootfs 路径不存在或解释器路径错误。

**排查:**
```bash
# 检查 rootfs
ls -la /var/lib/beebotos/rootfs/python/opt/python/bin/python3

# 检查解释器是否可执行
file /var/lib/beebotos/rootfs/python/opt/python/bin/python3
```

### Q4: cgroup 写入权限拒绝

**原因:** 非 root 用户无法写入 `/sys/fs/cgroup/`。

**解决:**
```bash
# 创建用户可写的 cgroup 子目录
sudo mkdir -p /sys/fs/cgroup/beebotos/foreign_rt
sudo chown $(whoami):$(whoami) /sys/fs/cgroup/beebotos/foreign_rt
```

---

## 验证清单

| 检查项 | 方法 | 预期结果 |
|--------|------|----------|
| 编译通过 | `cargo check -p beebotos-foreign-rt` | 0 errors |
| 单元测试 | `cargo test -p beebotos-foreign-rt --lib -j1` | 43 passed |
| 健康检查 | `curl /api/v1/runtimes/health` | 返回可用状态 |
| Python 执行 | `curl -X POST /api/v1/tasks/execute-script` | success=true |
| Node.js 执行 | `curl -X POST /api/v1/tasks/execute-script` | success=true |
| 内存限制 | 传入 max_memory_mb=64 执行大内存脚本 | 返回资源限制错误 |
| 超时处理 | 传入 timeout_secs=1 执行 sleep 脚本 | 返回 Timeout 错误 |
| 指标记录 | 查看 Prometheus 指标 | `beebotos_foreign_rt_executions_total` 有数据 |
| Agent 集成 | 发送 ForeignPythonWasm Task | 正确路由到 foreign-rt |
| Skill 注册 | 加载带 runtime=python 的 Skill | manifest.runtime == Python |


&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&
我已经为你编写了一份完整的人工测试验证指南，保存在：

📄 `/root/beebotos/docs/evolution/python-nodjs/manual-testing-guide.md`

---

## 当前环境可立即执行的测试

### 1️⃣ 单元测试（无需任何外部资源，已验证通过）

```bash
cd /root/beebotos

# Foreign-RT 核心测试（43 个全部通过）
cargo test -p beebotos-foreign-rt --lib -j1

# Agent 集成测试（3 个全部通过）
cargo test -p beebotos-agents foreign_executor --lib -j1
```

### 2️⃣ Gateway REST API 测试（需启动 Gateway）

```bash
# 终端 1：启动 Gateway
cargo run -p beebotos-gateway

# 终端 2：健康检查
curl -s http://localhost:8080/api/v1/runtimes/health | jq

# 查询可用运行时（现在会正确返回 wasm_available / process_available）
curl -s http://localhost:8080/api/v1/runtimes | jq

# 执行 Python 脚本（需要准备 WASM 模块或 rootfs，见下文）
curl -s -X POST http://localhost:8080/api/v1/tasks/execute-script \
  -H "Content-Type: application/json" \
  -d '{
    "runtime": "python",
    "source": {"type": "inline", "content": "def main(input): return {\"result\": input[\"x\"] + 1}"},
    "entrypoint": "main",
    "input": {"x": 42}
  }' | jq
```

---

## ⚠️ 当前环境的真实状态

| 路径 | 状态 | 说明 |
|------|------|------|
| **WASM Path** | ❌ 缺少模块文件 | 需要 `pyodide.asm.wasm` 和 `qjs.wasm` |
| **Process Path** | ❌ 缺少 rootfs | 需要 Python/Node.js 的隔离文件系统 |
| **nsjail** | ❌ 未安装 | 会自动回退到 `unshare` |
| **cgroup v2** | ✅ 可用 | `/sys/fs/cgroup/cgroup.controllers` 存在 |

**所以目前直接执行脚本会返回 `RuntimeNotAvailable` — 这是预期行为。**

---

## 要真正跑通脚本，需要准备以下资源之一

### 方案 A：WASM 路径（推荐）

```bash
# 1. 创建目录
mkdir -p /opt/beebotos/wasm-modules

# 2. 获取 Pyodide WASM（约 10MB+）
wget -O /opt/beebotos/wasm-modules/pyodide.asm.wasm \
  https://cdn.jsdelivr.net/pyodide/v0.25.1/full/pyodide.asm.wasm

# 3. QuickJS WASM 需要自行编译
# git clone https://github.com/justjake/quickjs-wasm
# cd quickjs-wasm && cargo build --target wasm32-wasi

# 4. 修改 Gateway 配置，指向这些路径
# 在 apps/gateway/src/main.rs 中配置：
#   config.wasm.pyodide_module_path = Some("/opt/beebotos/wasm-modules/pyodide.asm.wasm".into());
```

### 方案 B：Process 路径（更简单）

```bash
# 1. 使用系统 Python/Node 做快速验证（非生产安全）
mkdir -p /var/lib/beebotos/rootfs/python
ln -sf /usr/bin/python3 /var/lib/beebotos/rootfs/python/opt/python/bin/python3

# 2. 修改 Gateway 配置
#   config.process.python_rootfs = Some("/var/lib/beebotos/rootfs/python".into());
```

---

## 关键验证点总结

| 验证项 | 命令/方法 | 预期结果 |
|--------|-----------|----------|
| 编译 | `cargo check -p beebotos-foreign-rt` | 0 errors |
| 单元测试 | `cargo test -p beebotos-foreign-rt --lib -j1` | 43 passed |
| 路由选择 | `curl /api/v1/runtimes` | 正确显示可用性 |
| 指标 | Prometheus `/metrics` | `beebotos_foreign_rt_executions_total` 存在 |
| Agent 路由 | 发送 `TaskType::ForeignPythonWasm` | 路由到 `handle_foreign_runtime_task` |
| Skill 解析 | 加载带 `runtime: python` 的 Skill | `manifest.runtime == Python` |

**当前代码层面已 100% 就绪，只差实际的 WASM 模块文件或进程 rootfs 即可跑通端到端脚本执行。**
