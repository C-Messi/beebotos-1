# BeeBotOS Python & Node.js 运行时集成技术方案

> **文档编号**: BEE-EVOL-PYNODE-v1  
> **版本**: v1.0  
> **日期**: 2026-05-15  
> **状态**: 技术设计（Tech Design）  
> **适用范围**: `crates/kernel`, `crates/agents`, `apps/gateway`  
> **作者**: BeeBotOS Architecture Team  

---

## 一、设计目标与核心原则

### 1.1 设计目标

在 BeeBotOS 现有 5 层架构基础上，引入 **Python 3.11+** 与 **Node.js 20+** 运行时支持，使 Agent 能够直接执行 Python/Node.js 脚本，满足以下场景：

| 场景 | 典型用例 | 优先级 |
|------|---------|--------|
| **数据科学 & AI 推理** | Pandas/NumPy/PyTorch 数据处理、模型推理 | P0 |
| **Web 自动化 & 爬虫** | Playwright/Puppeteer、Selenium、 Cheerio | P0 |
| **脚本化 Skill** | 将现有 Python/Node.js 生态工具快速封装为 Agent Skill | P0 |
| **快速原型** | Agent 动态编写并执行脚本完成一次性任务 | P1 |
| **遗留系统集成** | 调用企业内已有的 Python/Node.js 服务或库 | P1 |

### 1.2 核心原则

1. **安全优先，零信任默认**：任何外部语言代码执行必须经过多层沙箱隔离，默认拒绝所有系统访问，按 Capability 显式授权。
2. **WASM 优先，进程降级**：优先利用现有 WASM 基础设施（Layer 1 Kernel）执行语言运行时；仅当 WASM 方案无法覆盖（如原生 C 扩展、V8 完整特性）时，才降级到外部进程沙箱。
3. **内核级资源管控**：Python/Node.js 运行时必须纳入 Kernel 的调度器（Scheduler）与资源计量（Gas/Fuel）体系，禁止绕过内核直接占用系统资源。
4. **与现有架构正交**：不破坏现有 WASM 沙箱、Capability 权限、A2A 协议、Skill 注册机制；新增模块以插件形式存在，可独立启用/禁用。
5. **性能可预测**：提供运行时预热池（Warm Pool）、模块预编译缓存、并行执行策略，确保脚本执行延迟可控。

### 1.3 明确排除项

- **不替代现有 WASM Skill 体系**：Python/Node.js 是补充而非替代，Rust → WASM 仍是性能敏感 Skill 的首选。
- **不支持 GUI 应用**：不考虑在 BeeBotOS 内核中运行带图形界面的 Python/Node.js 程序。
- **不内置包管理器联网**：`pip install` / `npm install` 不在运行时内部自动执行，依赖镜像在构建/部署阶段准备。

---

## 二、总体架构

### 2.1 架构定位

在 BeeBotOS 5 层架构中，Python/Node.js 运行时横跨 **Layer 1 (Kernel)** 与 **Layer 3 (Agent Layer)**：

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Layer 4: Applications                                                       │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Skill Marketplace · DeFAI · DAO Governance · Game AI               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 3: Agent Layer                                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │    │
│  │  │   A2A        │  │  Workflow    │  │  Skill Registry          │  │    │
│  │  │  Protocol    │  │  Engine      │  │  (WASM / Py / Node)      │  │    │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │    │
│  │  ┌─────────────────────────────────────────────────────────────────┐ │    │
│  │  │          Foreign Runtime Bridge (agents::foreign_rt)            │ │    │
│  │  │   ┌────────────┐  ┌────────────┐  ┌────────────────────────┐  │ │    │
│  │  │   │  Python    │  │  Node.js   │  │  WASM Fallback         │  │ │    │
│  │  │   │  Bridge    │  │  Bridge    │  │  (Pyodide/QuickJS)     │  │ │    │
│  │  │   └────────────┘  └────────────┘  └────────────────────────┘  │ │    │
│  │  └─────────────────────────────────────────────────────────────────┘ │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 2: Social Brain                                                       │
│  NEAT · Memory System · Reasoning Engine · CAPO                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 1: Kernel                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Scheduler · Security · Capability · IPC · Syscalls                 │    │
│  │  ┌─────────────────────────────────────────────────────────────┐   │    │
│  │  │         Foreign Runtime Manager (kernel::foreign_rt)        │   │    │
│  │  │  ┌────────────┐  ┌────────────┐  ┌────────────────────┐    │   │    │
│  │  │  │  WASM RT   │  │  Process   │  │  Resource          │    │   │    │
│  │  │  │  (Pyodide) │  │  Sandbox   │  │  Controller        │    │   │    │
│  │  │  │  (QuickJS) │  │  (nsjail)  │  │  (cgroup/seccomp)  │    │   │    │
│  │  │  └────────────┘  └────────────┘  └────────────────────┘    │   │    │
│  │  └─────────────────────────────────────────────────────────────┘   │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 0: Blockchain                                                         │
│  Ethereum · BSC · Polygon · Solana · Cross-Chain Bridge                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 双路径执行模型

针对 Python 与 Node.js 的生态差异，设计 **WASM 主路径** 与 **进程降级路径** 的双轨执行模型：

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Foreign Script Execution Request                       │
│                              (Skill / Agent Task)                             │
└────────────────────────┬────────────────────────────────────────────────────┘
                         │
                         ▼
              ┌─────────────────────┐
              │  Runtime Router     │
              │  (基于任务标签路由)  │
              └──────────┬──────────┘
                         │
         ┌───────────────┼───────────────┐
         │               │               │
         ▼               ▼               ▼
   ┌──────────┐   ┌──────────┐   ┌──────────────┐
   │  WASM    │   │  WASM    │   │  Process     │
   │  Path    │   │  Path    │   │  Sandbox     │
   │ (Python) │   │ (Node.js)│   │  Path        │
   │          │   │          │   │  (Fallback)  │
   └────┬─────┘   └────┬─────┘   └──────┬───────┘
        │              │                │
        ▼              ▼                ▼
   ┌──────────┐   ┌──────────┐   ┌──────────────┐
   │ Pyodide  │   │ QuickJS  │   │ nsjail /     │
   │ (wasmtime)│  │ (wasmtime)│  │ Firecracker  │
   │ 128MB    │   │  64MB    │   │  512MB       │
   │ 10M fuel │   │  10M fuel│   │  30s/300s    │
   └──────────┘   └──────────┘   └──────────────┘
```

**路由策略**：

| 条件 | 路径选择 | 说明 |
|------|---------|------|
| 任务标记 `runtime: wasm-only` | WASM 路径 | 强制沙箱内执行，最安全 |
| 依赖纯标准库 / 已编译 WASM 的库 | WASM 路径 | 无原生扩展 |
| 依赖 numpy/pandas/pytorch / V8 完整特性 | 进程路径 | WASM 当前无法支持原生 C 扩展 |
| 任务标记 `runtime: process-allowed` | 进程路径 | 用户显式授权 |
| 内存需求 > 256MB | 进程路径 | WASM 内存上限可配置，但进程路径更灵活 |

---

## 三、Python 运行时方案

### 3.1 WASM 主路径：Pyodide on Wasmtime

**技术选型**：Pyodide 是将 CPython 3.11 + 核心科学计算栈（NumPy、SciPy、Pandas 等）编译为 WASM 的成熟方案。

**集成方式**：

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Pyodide WASM Runtime                               │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    wasmtime::Store / Instance                       │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │   │
│  │  │   Pyodide    │  │  WASI        │  │  BeeBotOS Host Functions │  │   │
│  │  │   VM         │  │  ctx (fd)    │  │  (syscall bridge)        │  │   │
│  │  │              │  │  preopen     │  │  · kernel::ipc::send     │  │   │
│  │  │  micropip    │  │  /tmp        │  │  · storage::get/put      │  │   │
│  │  │  numpy       │  │  /workspace  │  │  · llm::chat_completion  │  │   │
│  │  │  pandas      │  │  /dev/stdin  │  │  · chain::call_contract  │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│  Kernel Integration                                                         │
│  · Fuel Metering (每字节码指令计费)                                          │
│  · Memory Limit (max_memory_size)                                           │
│  · Preopen Dir (只读 /workspace, 可写 /tmp)                                  │
│  · Capability Filter (通过 Host Function ACL 控制)                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键实现点**：

1. **预编译模块缓存**：Pyodide 的 `.so` WASM 模块（如 numpy）体积巨大（>5MB），需在 Kernel 的 `wasm::engine::module_cache` 中缓存编译后的 `wasmtime::Module`，避免每次冷启动重新编译。
2. **文件系统虚拟化**：通过 `wasmtime_wasi::WasiCtxBuilder` 只预开放任务指定的 `/workspace`（只读）和 `/tmp`（可写，tmpfs），禁止访问宿主任意路径。
3. **Host Function Bridge**：在 `kernel::wasm::host_funcs` 中新增 `py_bridge_*` 系列函数，使 Python 代码能通过 `import js` 风格的 API 调用 BeeBotOS 内核能力：
   - `beebotos_ipc_send(agent_id, message)` → A2A 通信
   - `beebotos_storage_get(key)` → 键值存储
   - `beebotos_llm_chat(model, prompt)` → LLM 推理
   - `beebotos_chain_call(chain_id, contract, data)` → 区块链调用
4. **包加载策略**：Pyodide 的 `micropip` 可从预配置的 PyPI 镜像（如企业内部 Nexus）下载纯 Python 包。所有 `.whl` 需在执行前预下载到 `/workspace/packages`，运行时禁止联网下载。

**性能指标（目标）**：

| 指标 | 目标值 | 备注 |
|------|--------|------|
| 冷启动（首次加载 Pyodide） | < 2s | 依赖 module_cache 预热 |
| 热启动（从实例池获取） | < 100ms | Instance Pool 复用 Store |
| 内存占用 | 128MB ~ 256MB | 可通过 `EngineConfig` 调节 |
| 标准库脚本执行 | < 50ms | 无复杂 import |
| NumPy 矩阵运算 (1000x1000) | < 500ms | WASM SIMD 加速 |

### 3.2 进程降级路径：nsjail + CPython

**适用场景**：需要调用 PyTorch GPU 推理、Cython 扩展、或内存需求 > 512MB 的 Python 任务。

**技术选型**：

- **隔离器**：`nsjail`（轻量级进程沙箱，基于 Linux namespaces + seccomp-bpf + cgroup）
- **备选**：`bubblewrap`（更轻，但功能稍弱）或 `gVisor`（更强隔离，开销更大）
- **运行时**：宿主系统安装的 CPython 3.11+（通过 Docker 多阶段构建固定版本）

**沙箱配置**：

```protobuf
// ForeignRuntimeConfig (扩展自 kernel::security::SandboxConfig)
message PythonProcessConfig {
  string runtime_id = 1;          // "python-3.11-gpu"
  string interpreter_path = 2;    // "/opt/python-3.11/bin/python3"
  repeated string allowed_paths = 3;  // ["/workspace/task-123"]
  string tmp_dir = 4;             // "/tmp/beebotos-py-XXXX"
  uint64 max_memory_mb = 5;       // 2048
  uint64 max_cpu_time_ms = 6;     // 300000 (5min)
  uint32 max_pids = 7;            // 8
  bool network_allowed = 8;       // false (默认)
  bool gpu_allowed = 9;           // true (仅特定任务)
  repeated string seccomp_bpf_allow = 10; // 额外允许的 syscall
}
```

**执行流程**：

```text
Agent Task
    │
    ▼
┌─────────────────┐
│ foreign_rt_mgr  │ 创建临时 cgroup (memory, cpu, pids)
│  (kernel)       │ 创建 tmpfs /workspace, /tmp
└────────┬────────┘
         │ fork + execve
         ▼
┌─────────────────┐
│ nsjail          │ 挂载新的 rootfs (overlayfs on /opt/python-root)
│ (namespaces)    │ drop capabilities, set seccomp filter
│                 │ bind mount 只读 /workspace, 可写 /tmp
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ CPython 3.11    │ 执行用户脚本
│ (in cgroup)     │ stdout/stderr 通过 pipe 捕获
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ foreign_rt_mgr  │ 解析输出为 TaskResult
│                 │ 清理 cgroup, tmpfs, 审计日志
└─────────────────┘
```

**安全措施**：

1. **seccomp-bpf 白名单**：默认仅允许 `read`, `write`, `exit`, `exit_group`, `mmap`, `munmap`, `futex`, `clock_gettime`。禁止 `socket`, `connect`, `openat`（超出预开放目录）。
2. **Capability 传递**：进程路径同样需持有 Capability Token，内核在 `foreign_rt_mgr` 层校验 `CAP_FOREIGN_RT_EXEC` 与具体资源权限（如 `CAP_NETWORK`）。
3. **输出过滤**：Python 的 stdout/stderr 经过 DLP（数据防泄漏）扫描，防止脚本通过侧信道（如时序编码）泄露敏感数据。

---

## 四、Node.js 运行时方案

### 4.1 WASM 主路径：QuickJS + TypeScript Compiler on Wasmtime

**技术选型说明**：Node.js 官方 V8 引擎庞大且依赖复杂，不适合直接编译为 WASM。选用 **QuickJS**（Fabrice Bellard 编写的轻量 JS 引擎，完整支持 ES2023）编译为 WASM，配合 **SWC** 或 `tsc` 的 WASM 版本进行 TypeScript → JavaScript 转译。

**架构**：

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                        QuickJS WASM Runtime                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    wasmtime::Store / Instance                       │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │   │
│  │  │   QuickJS    │  │  WASI        │  │  BeeBotOS Host Functions │  │   │
│  │  │   Engine     │  │  ctx         │  │  (node_bridge_*)         │  │   │
│  │  │              │  │  · /workspace│  │  · fetch (受限)          │  │   │
│  │  │  · ES2023    │  │  · /tmp      │  │  · crypto::sign          │  │   │
│  │  │  · Async/Await│  │  · /dev/stdin│  │  · storage::*            │  │   │
│  │  │  · setTimeout│  │              │  │  · ipc::*                │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│  QuickJS 特别适配                                                           │
│  · Event Loop 映射到 Tokio 任务调度                                          │
│  · setTimeout/setInterval 通过 host func 桥接到 tokio::time                │
│  · Promise 状态机与 Async/Await 支持                                         │
│  · console.log 重定向到结构化 tracing 日志                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

**TypeScript 支持**：

1. 在构建阶段将 `tsc`（或 swc wasm）与类型定义一起打包为 WASM 模块。
2. 运行时先将 `.ts` 源码通过 `tsc.wasm` 转译为 `.js`，再注入 QuickJS 执行。
3. 限制：不支持 `tsconfig.json` 中复杂的 `compilerOptions`，仅提供标准 ES2023 target。

**npm 生态兼容层**：

QuickJS 不支持原生 Node.js API，但可提供 **Shim 层**：

```javascript
// beebotos:node-shim
// 在 QuickJS 全局注入兼容 API
import { readFile, writeFile } from 'beebotos:fs';
import { fetch } from 'beebotos:fetch';
import { setTimeout } from 'beebotos:timers';

// 模拟部分 Node.js API
globalThis.Buffer = BeeBotOSBuffer;
globalThis.process = { env: getAllowedEnv() };
```

**适用场景**：轻量数据转换、JSON/YAML 处理、简单 HTTP 请求编排、小型算法脚本。

### 4.2 进程降级路径：Node.js in nsjail

**适用场景**：需要完整 Node.js/V8 特性（如复杂异步 I/O、原生 C++ 扩展 `node-gyp`、Electron 相关工具链）、Playwright/Puppeteer 浏览器自动化。

**技术选型**：

- **运行时**：官方 Node.js 20 LTS（通过 `node:20-alpine` Docker 镜像提取）
- **隔离器**：`nsjail`（同 Python）
- **浏览器自动化**：在 nsjail 内运行 Chromium（通过 `playwright-core` 下载的 Chromium 可执行文件），配合 `--headless --no-sandbox`（nsjail 本身已是沙箱）

**资源配额示例**：

| 任务类型 | 内存 | CPU | 网络 | 磁盘 | 超时 |
|---------|------|-----|------|------|------|
| 轻量脚本 | 256MB | 1核 | 否 | 100MB | 30s |
| HTTP 服务调用 | 512MB | 1核 | 是（白名单域名） | 100MB | 60s |
| Playwright 爬虫 | 2GB | 2核 | 是（代理出口） | 1GB | 300s |

**特殊安全考量（Playwright）**：

1. **Chromium 沙箱中沙箱**：nsjail 外层隔离 + Chromium 自身 `--no-sandbox` 改为 `--sandbox`（如果支持），或利用 `--disable-setuid-sandbox`。
2. **网络代理强制**：所有 outbound 流量强制经过 BeeBotOS Gateway 的出站代理，进行域名白名单校验和 TLS 拦截审计。
3. **屏幕截图限制**：Playwright 截图文件经过病毒扫描和敏感信息 OCR 检测后才能返回给 Agent。

---

## 五、Kernel 层核心模块设计

### 5.1 新增 Crate：`crates/foreign-rt`

为保持内核简洁，新增独立 crate `foreign-rt`（Foreign Runtime），被 `kernel` 依赖。

```text
crates/foreign-rt/
├── src/
│   ├── lib.rs
│   ├── config.rs          # ForeignRuntimeConfig, SandboxPolicy
│   ├── router.rs          # RuntimeRouter (WASM vs Process)
│   ├── wasm_path/
│   │   ├── mod.rs
│   │   ├── pyodide.rs     # Pyodide 引擎初始化、包加载
│   │   ├── quickjs.rs     # QuickJS 引擎封装
│   │   └── shim.rs        # JS/TS Shim 注入
│   ├── process_path/
│   │   ├── mod.rs
│   │   ├── sandbox.rs     # nsjail/bwrap 配置生成与执行
│   │   ├── cgroup.rs      # cgroup v2 资源限制
│   │   └── seccomp.rs     # seccomp-bpf 规则生成
│   ├── pool.rs            # 运行时实例预热池
│   ├── metering.rs        # 跨路径资源计量 (Gas 统一换算)
│   └── bridge/
│       ├── mod.rs
│       ├── host_funcs.rs  # WASM Host Functions 统一注册
│       └── protocol.rs    # 进程间通信协议 (JSON Lines over Unix Domain Socket)
```

### 5.2 运行时预热池（Warm Pool）

为降低冷启动延迟，对 WASM 路径实现 **Instance Pool**：

```rust
// crates/foreign-rt/src/pool.rs
pub struct RuntimePool {
    /// WASM Store 池（按运行时类型分片）
    wasm_stores: HashMap<RuntimeType, Arc<ObjectPool<wasmtime::Store<HostContext>>>>,
    /// 进程槽位池（仅维护准备好环境的 nsjail rootfs，非运行中进程）
    process_slots: HashMap<RuntimeType, Arc<Semaphore>>,
    config: PoolConfig,
}

pub struct PoolConfig {
    pub pyodide_warm_instances: usize,   // 默认 2
    pub quickjs_warm_instances: usize,   // 默认 4
    pub max_process_slots: usize,        // 默认 10
    pub idle_timeout: Duration,          // 60s
}
```

**工作原理**：

1. **WASM 预热**：Kernel 启动时，预初始化 2 个 Pyodide Store 和 4 个 QuickJS Store，加载核心模块（如 numpy shim、console polyfill）。任务到达时从池中获取，执行后重置 Store 状态（释放全局变量、清空 /tmp）并归还。
2. **进程槽位**：不预 fork 进程（避免内存浪费），但预创建好 overlayfs rootfs 和 cgroup slice。任务到达时只需 `clone()` + `execve()`，节省目录准备时间。

### 5.3 统一资源计量（Gas 体系）

无论 WASM 还是进程路径，所有资源消耗需统一换算为 **BeeBotOS Gas**，与现有区块链 Gas 模型对齐：

```rust
// crates/foreign-rt/src/metering.rs
pub struct ForeignGasReport {
    pub compute_gas: u64,      // CPU 指令 / 时间换算
    pub memory_gas: u64,       // 内存占用 × 时间
    pub io_gas: u64,           // 读写字节数
    pub network_gas: u64,      // 出站流量
    pub storage_gas: u64,      // KV 存储操作
}

impl ForeignGasReport {
    pub fn total(&self) -> u64 {
        self.compute_gas
            + self.memory_gas
            + self.io_gas
            + self.network_gas
            + self.storage_gas
    }
}
```

**计量方式**：

| 路径 | compute_gas | memory_gas | 实现机制 |
|------|-------------|------------|----------|
| WASM | wasmtime fuel consumption | Store 内存线性增长量 | wasmtime 内置 |
| Process | cgroup `cpu.stat.user_usec` | cgroup `memory.peak` | 轮询或任务结束后读取 |

**超限处理**：

- WASM：直接 `wasmtime::Trap::OutOfFuel` 或内存限制 Trap。
- Process：发送 `SIGKILL`，并记录审计日志。

### 5.4 Capability 集成

在现有 11 层 Capability 模型中，新增 **Layer 6: Foreign Runtime** 能力：

```rust
// 扩展 kernel::capabilities
pub enum CapabilityLevel {
    // ... existing levels 0-10
    ForeignRuntimeBasic,      // 允许执行 WASM 路径脚本（Pyodide/QuickJS）
    ForeignRuntimeProcess,    // 允许降级到进程沙箱
    ForeignRuntimeNetwork,    // 允许脚本访问网络（需配合域名白名单）
    ForeignRuntimeGPU,        // 允许访问 GPU（仅进程路径）
    ForeignRuntimePrivileged, // 允许挂载额外目录、提升超时
}
```

**Capability 委托**：Agent 可将 `ForeignRuntimeBasic` 委托给特定 Skill，但 `ForeignRuntimeProcess` 及以上需用户显式确认（类似 macOS 提权对话框，在 BeeHub UI 或 CLI 中实现）。

---

## 六、Agent Layer 集成设计

### 6.1 Skill 系统扩展

现有 Skill 注册以 WASM 模块或 Rust 代码为主，需扩展支持 Python/Node.js Skill：

```yaml
# example: skills/data_analyzer/SKILL.md
skill:
  name: data_analyzer
  version: 1.0.0
  runtime: python          # python | nodejs | wasm
  entrypoint: analyze.py   # 或 main.ts
  # 沙箱需求声明（供调度器选择路径）
  sandbox:
    min_memory_mb: 512
    requires_gpu: false
    requires_network: true
    allowed_domains:
      - api.example.com
  # 依赖声明（构建阶段解析）
  dependencies:
    python:
      - pandas==2.0.0
      - requests==2.31.0
    nodejs:
      - cheerio@1.0.0
  # Capability 需求
  capabilities:
    - ForeignRuntimeBasic
    - ForeignRuntimeNetwork
```

**Skill Registry 加载流程**：

1. `agents::skill_registry` 解析 `SKILL.md`，提取 `runtime` 和 `sandbox` 字段。
2. 若 `runtime == python` 且所有依赖为纯 Python / 已有 WASM wheel → 标记为 `wasm-viable`。
3. 若存在原生扩展或 `requires_gpu: true` → 标记为 `process-required`。
4. 执行时，`RuntimeRouter` 根据标记和当前系统负载选择最优路径。

### 6.2 TaskExecutor 扩展

在 `agents::runtime::executor` 中新增 `ForeignTaskExecutor`：

```rust
pub struct ForeignTaskExecutor {
    pool: Arc<RuntimePool>,
    router: RuntimeRouter,
    gas_oracle: Arc<dyn GasOracle>,
}

#[async_trait]
impl TaskExecutor for ForeignTaskExecutor {
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        let skill_meta = self.registry.get(&task.skill_id)?;
        let route = self.router.select(&skill_meta, &task)?;

        match route {
            Route::WasmPyodide => self.exec_pyodide(task).await,
            Route::WasmQuickJS => self.exec_quickjs(task).await,
            Route::ProcessPython => self.exec_python_process(task).await,
            Route::ProcessNodejs => self.exec_nodejs_process(task).await,
        }
    }
}
```

**批量执行优化**：

- 同类型、同依赖的 Python 任务可复用同一个 warmed Store，通过 `micropip` 动态加载差异包。
- Node.js 进程路径支持 `execute_parallel`，但受 `process_slots` 信号量限制。

### 6.3 A2A 协议兼容

Python/Node.js 脚本在执行过程中可能需要与其他 Agent 通信。通过 Host Function Bridge 暴露 A2A Client：

```python
# Python (Pyodide) 中调用
import beebotos

result = beebotos.a2a.send_message(
    target_agent="finance_agent",
    message_type="price_query",
    payload={"symbol": "BTC-USD"},
    timeout_ms=5000
)
```

```javascript
// Node.js (QuickJS 或进程) 中调用
const { a2a } = require('beebotos');

await a2a.sendMessage({
  targetAgent: 'finance_agent',
  messageType: 'price_query',
  payload: { symbol: 'BTC-USD' },
  timeoutMs: 5000,
});
```

底层通过 `kernel::ipc` 的 Unix Domain Socket 或 Message Bus 转发，不暴露底层网络细节给脚本。

---

## 七、性能优化策略

### 7.1 预编译与缓存金字塔

```text
┌─────────────────────────────────────────────────────────────┐
│                    Compilation Cache Pyramid                │
├─────────────────────────────────────────────────────────────┤
│  L1: In-Memory Module Cache (kernel::wasm::engine)          │
│      · wasmtime::Module 对象缓存                            │
│      · 键: (runtime_type, module_hash)                      │
│      · 淘汰: LRU, max 64 entries                            │
├─────────────────────────────────────────────────────────────┤
│  L2: Disk AOT Cache ($BEE_DATA/foreign_rt/aot/)             │
│      · wasmtime AOT compiled artifacts (.cwasm)             │
│      · 跨进程持久化，重启后秒级恢复                         │
├─────────────────────────────────────────────────────────────┤
│  L3: Docker Image / OCI Bundle Cache                        │
│      · Python/Node.js 进程路径的 rootfs 层                  │
│      · 使用 overlayfs，增量更新依赖                           │
├─────────────────────────────────────────────────────────────┤
│  L4: PyPI/npm Mirror 本地代理                               │
│      · 企业内部 Nexus/Verdaccio                             │
│      · 构建阶段预拉取，运行时零下载                           │
└─────────────────────────────────────────────────────────────┘
```

### 7.2 SIMD 与 GPU 加速

- **WASM SIMD**：Pyodide 已支持 WASM SIMD128 用于 NumPy 基础运算。在 `EngineConfig` 中确保 `wasm_simd: true`（wasmtime 默认开启）。
- **GPU 透传**：仅进程路径支持。通过 nvidia-container-toolkit 或 `/dev/dri` bind mount 将 GPU 暴露给 nsjail 内的 Python 进程。Capability `ForeignRuntimeGPU` 为硬门槛。

### 7.3 冷启动优化对比

| 优化手段 | WASM Pyodide | WASM QuickJS | Process Python | Process Node.js |
|---------|--------------|--------------|----------------|-----------------|
| Module Cache | -60% (编译) | -60% | N/A | N/A |
| Instance Pool | -90% (初始化) | -90% | -30% (rootfs) | -30% |
| AOT Artifacts | -50% (加载) | -50% | N/A | N/A |
| Lazy Import | -40% (启动) | N/A | -20% | -20% |

---

## 八、安全纵深防御体系

### 8.1 五层安全模型

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Layer 5: Application Security                                               │
│  · Skill Registry 签名验证 (ed25519)                                        │
│  · 依赖漏洞扫描 (OSV / Snyk) 在构建阶段阻断                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 4: Capability & ACL                                                   │
│  · 动态 Capability 校验：脚本运行前校验 Token 有效性                        │
│  · 最小权限原则：脚本只能访问声明的目录和 API                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 3: Runtime Sandbox                                                    │
│  · WASM: wasmtime memory limits + fuel + WASI capability-based security    │
│  · Process: namespaces + seccomp + AppArmor/SELinux (若可用)                │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 2: Resource Limits                                                    │
│  · cgroup v2: memory.max, cpu.max, pids.max                                 │
│  · 内核级 OOM Killer 优先级调整 (oom_score_adj=1000)                        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Layer 1: Host Hardening                                                     │
│  · 只读 rootfs (进程路径 overlayfs lowerdir 只读)                           │
│  · 无特权用户 (nobody:noguid)                                               │
│  · 审计日志：所有 syscall、文件访问、网络连接记录到 kernel::security::audit │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 8.2 供应链安全

- **锁定文件**：Python 的 `requirements.txt` (SHA256) 和 Node.js 的 `package-lock.json` 必须提交，构建时 `--require-hashes` / `--frozen-lockfile`。
- **私有 Registry**：运行时禁止访问 public PyPI/npm，所有包通过内部 Nexus/Verdaccio 代理，经过漏洞扫描和许可证合规检查。
- **SBOM 生成**：每个 Skill 的 Python/Node.js 依赖生成 CycloneDX SBOM，上链存证。

### 8.3 侧信道防护

- **时序攻击**：进程路径的 CPU time 计量加入随机噪声（±5%），防止脚本通过 CPU 调度时序编码信息。
- **缓存状态泄露**：WASM Instance Pool 归还前必须执行 `__beebotos_cleanup()`，重置所有全局变量、清空本地存储、注销 event listeners。

---

## 九、数据流与接口设计

### 9.1 脚本执行完整数据流

```text
User / Agent
    │
    ▼
┌─────────────────────────────────────┐
│  Gateway (apps/gateway)             │
│  POST /api/v1/tasks/execute         │
│  { skill_id, input, runtime_hint }  │
└─────────────┬───────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│  Agent Runtime (agents::runtime)    │
│  · 解析 Skill 元数据                │
│  · 校验 Capability                  │
│  · 调用 RuntimeRouter               │
└─────────────┬───────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│  Foreign Runtime Manager            │
│  (kernel::foreign_rt 或 foreign-rt crate)
│  · 选择路径 (WASM / Process)        │
│  · 分配资源 (Pool / Cgroup)         │
│  · 注入 Host Functions / Env        │
└─────────────┬───────────────────────┘
              │
    ┌─────────┴─────────┐
    ▼                   ▼
┌──────────┐     ┌──────────────┐
│ WASM     │     │ Process      │
│ Runtime  │     │ Sandbox      │
│          │     │              │
│ · 执行   │     │ · fork/exec  │
│ · fuel   │     │ · seccomp    │
│ · trap   │     │ · cgroup     │
└────┬─────┘     └──────┬───────┘
     │                  │
     ▼                  ▼
┌─────────────────────────────────────┐
│  Result Aggregator                  │
│  · 解析 stdout / JSON / Artifacts   │
│  · 生成 GasReport                   │
│  · 审计日志写入                     │
└─────────────┬───────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│  Response to Caller                 │
│  { result, gas_used, logs, artifacts }
└─────────────────────────────────────┘
```

### 9.2 核心接口定义

```rust
// crates/foreign-rt/src/lib.rs

/// 统一脚本执行请求
#[derive(Debug, Clone)]
pub struct ScriptTask {
    pub task_id: String,
    pub runtime: ForeignRuntime,
    pub source: ScriptSource,       // Inline code | File path | Prebuilt module
    pub entrypoint: String,         // "main" | "handler" | 自定义函数名
    pub input: serde_json::Value,
    pub sandbox: SandboxRequirements,
    pub capabilities: CapabilitySet,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum ForeignRuntime {
    Python,
    NodeJs,
}

#[derive(Debug, Clone)]
pub struct SandboxRequirements {
    pub min_memory_mb: usize,
    pub max_memory_mb: usize,
    pub network_allowed: bool,
    pub allowed_domains: Vec<String>,
    pub filesystem_paths: Vec<PathMapping>,
    pub gpu_allowed: bool,
}

/// 统一执行结果
#[derive(Debug, Clone)]
pub struct ScriptResult {
    pub task_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub artifacts: Vec<Artifact>,
    pub gas_report: ForeignGasReport,
    pub logs: Vec<LogEntry>,
    pub execution_time: Duration,
}

/// Foreign Runtime Manager 主入口
#[async_trait]
pub trait ForeignRuntimeManager: Send + Sync {
    async fn execute(&self, task: ScriptTask) -> Result<ScriptResult>;
    async fn prewarm(&self, runtime: ForeignRuntime) -> Result<()>;
    async fn stats(&self) -> RuntimePoolStats;
}
```

### 9.3 Gateway REST API 扩展

```http
POST /api/v1/tasks/execute-script
Content-Type: application/json
Authorization: Bearer <capability_token>

{
  "runtime": "python",
  "source": {
    "type": "inline",
    "code": "import beebotos\ndef main(input): return input['x'] + 1"
  },
  "input": { "x": 42 },
  "sandbox": {
    "max_memory_mb": 256,
    "network_allowed": false
  }
}

---

200 OK
Content-Type: application/json

{
  "success": true,
  "output": 43,
  "gas_used": {
    "compute": 125000,
    "memory": 45000,
    "total": 170000
  },
  "execution_time_ms": 45,
  "logs": [{"level": "info", "message": "..."}]
}
```

---

## 十、部署与运维

### 10.1 构建阶段

在 CI/CD 流水线中新增 `foreign-rt` 构建阶段：

```bash
# 1. 构建 Pyodide WASM 模块
./scripts/build-pyodide.sh \
  --packages numpy,pandas,requests \
  --output target/wasm/pyodide/

# 2. 构建 QuickJS WASM 模块
./scripts/build-quickjs.sh \
  --output target/wasm/quickjs/

# 3. 构建进程路径 rootfs (OCI image)
docker build -f docker/python-rootfs.Dockerfile -t beebotos/python-rootfs:3.11 .
docker build -f docker/nodejs-rootfs.Dockerfile -t beebotos/nodejs-rootfs:20 .

# 4. 提取 rootfs 为本地 overlayfs lowerdir
./scripts/extract-rootfs.sh beebotos/python-rootfs:3.11 /var/lib/beebotos/rootfs/python-3.11
```

### 10.2 运行时配置

```toml
# config/foreign_rt.toml
[foreign_rt]
enabled = true

[foreign_rt.wasm]
pyodide_module_path = "./target/wasm/pyodide/pyodide.asm.wasm"
pyodide_packages_dir = "./target/wasm/pyodide/packages/"
quickjs_module_path = "./target/wasm/quickjs/qjs.wasm"
max_wasm_memory_mb = 512
fuel_metering = true

[foreign_rt.wasm.pool]
pyodide_warm_instances = 2
quickjs_warm_instances = 4
idle_timeout_secs = 60

[foreign_rt.process]
python_rootfs = "/var/lib/beebotos/rootfs/python-3.11"
nodejs_rootfs = "/var/lib/beebotos/rootfs/nodejs-20"
nsjail_config_template = "./config/nsjail.proto"
max_process_slots = 10

[foreign_rt.process.cgroup]
parent_cgroup = "beebotos/foreign_rt"
memory_high_mb = 4096
swap_max_mb = 0

[foreign_rt.security]
seccomp_policy = "restrictive"  # restrictive | standard | permissive
audit_level = "full"            # none | syscall | full
```

### 10.3 监控指标

通过 `crates/telemetry` 暴露以下 Prometheus 指标：

```
beebotos_foreign_rt_executions_total{runtime="python",path="wasm",status="success"}
beebotos_foreign_rt_execution_duration_seconds{runtime="nodejs",path="process",quantile="0.99"}
beebotos_foreign_rt_pool_available{runtime="python",path="wasm"}
beebotos_foreign_rt_gas_used_total{runtime="python",path="wasm",resource="compute"}
beebotos_foreign_rt_sandbox_violations_total{reason="seccomp_kill",runtime="nodejs"}
```

---

## 十一、风险评估与缓解措施

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| Pyodide WASM 体积过大，导致 Kernel 启动缓慢 | 中 | 中 | AOT 编译缓存 + 懒加载；仅在使用时初始化 |
| 进程路径 seccomp 规则绕过（0-day） | 低 | 高 | 多层防御：namespaces + seccomp + AppArmor；及时更新 nsjail；安全审计 |
| npm/pip 依赖存在 RCE 漏洞 | 中 | 高 | 构建阶段漏洞扫描 + SBOM；运行时只读 rootfs；禁止运行时安装 |
| GPU 透传导致宿主机 CUDA 崩溃 | 低 | 高 | 独立 GPU 分区 (MIG) 或 vGPU；超时强制 kill；Health Check |
| 脚本通过 A2A Bridge 发起拒绝服务 | 中 | 中 | A2A 消息速率限制；Capability 细粒度控制；消息大小上限 |
| WASM Instance Pool 状态污染 | 中 | 高 | 归还前深度重置；定期淘汰旧实例；监控异常行为 |
| 时序侧信道泄露敏感数据 | 低 | 中 | 输出 DLP 扫描；CPU time 噪声；禁止高精度时钟访问 |

---

## 十二、实施路线图（Roadmap）

### Phase 1: 基础设施（✅ 已完成）

- [x] 创建 `crates/foreign-rt` crate 骨架，接入 workspace
- [x] 实现 `RuntimePool` 和 `RuntimeRouter` 基础结构
- [x] 扩展 `kernel::capabilities` 新增 `ForeignRuntime*` 级别
- [x] 扩展 `agents::runtime::executor` 的 `TaskType` 枚举

### Phase 2: WASM 路径 MVP（✅ 完成）

- [x] 集成 Pyodide WASM 到 `wasmtime::Store`，支持基础 Python 执行
- [x] 实现 Host Functions（storage/get/put/delete/list, ipc/send/receive, llm/chat/embed, chain/call/balance, fs/read/write/exists, log, env, system/info/time）
- [x] `BackendServices` trait + `MockBackendServices` 用于测试和集成
- [x] 集成 QuickJS WASM，支持 ES2023 + console 重定向
- [x] 实现 Instance Pool 预热与回收（框架完成）
- [x] 编写 E2E 测试：`cargo test -p foreign-rt`（39 passed）

### Phase 3: 进程路径 MVP（✅ 完成）

- [x] 实现 `nsjail` 配置生成器（`ProcessSandboxConfig::to_nsjail_config()`）
- [x] 进程执行自动检测 `nsjail` 可用性，可用时优先使用，否则回退到 `unshare`
- [x] 实现 `cgroup v2` 资源控制器（实际创建 cgroup，设置 memory.max / cpu.weight / pids.max）
- [x] `ProcessSandboxExecutor::execute()` 自动创建 cgroup → 添加进程 → 读取峰值 stats → 销毁 cgroup
- [x] 实现 `seccomp-bpf` 规则生成器（restrictive / standard / permissive 三档，~330 syscall）
- [x] 实现 stdout/stderr → `ScriptResult` 解析器
- [ ] 支持 Playwright/Chromium 在 nsjail 内运行（高级场景，未来迭代）

### Phase 4: 集成与优化（✅ 完成）

- [x] Agent Layer Skill Registry 扩展 `runtime: python | nodejs`
- [x] Gateway REST API 新增 `/tasks/execute-script`、`/api/v1/runtimes`、`/api/v1/runtimes/health`
- [x] A2A Bridge 完整实现（storage/ipc/llm/chain/fs/log/env/system 9 个 namespace）
- [x] Gas 计量统一换算（compute/memory/io/network/storage 五维）
- [x] Prometheus 指标暴露（`metrics` crate：`executions_total`, `execution_duration_seconds`, `gas_used_total`）
- [ ] 上链存证对接（extension point，待 ChainService 接口稳定后接入）
- [ ] 性能基准测试与调优（目标：热启动 < 100ms，待 Pyodide/QuickJS WASM 模块就绪后测试）

### Phase 5: 安全加固与发布（2 周）

- [ ] 渗透测试：沙箱逃逸尝试、侧信道攻击
- [ ] 安全审计：依赖漏洞扫描、SBOM 生成
- [ ] 文档完善：开发者指南、运维手册
- [ ] 社区发布：示例 Skill（Python 数据分析、Node.js 爬虫）

---

## 十三、附录

### A. 相关文档索引

| 文档 | 路径 | 说明 |
|------|------|------|
| Kernel WASM 运行时 | `crates/kernel/src/wasm/` | 现有 WASM 基础设施 |
| Capability 系统 | `crates/kernel/src/capabilities/` | 权限模型 |
| Agent Runtime | `crates/agents/src/runtime/` | 任务执行器 |
| 安全沙箱 | `crates/kernel/src/security/sandbox/` | 沙箱配置 |

### B. 术语表

| 术语 | 说明 |
|------|------|
| Pyodide | CPython 的 WASM 移植版，支持科学计算栈 |
| QuickJS | Fabrice Bellard 的轻量级 ES2023 引擎 |
| nsjail | Linux 进程隔离工具（namespaces + seccomp + cgroup） |
| seccomp-bpf | 内核系统调用过滤机制 |
| AOT | Ahead-of-Time 编译，指 wasmtime 的预编译产物 |
| Foreign Runtime | 指非 Rust/WASM 的外部语言运行时 |

### C. 参考实现

- [Pyodide](https://pyodide.org/) - CPython in WASM
- [QuickJS](https://bellard.org/quickjs/) - Embeddable JS Engine
- [nsjail](https://nsjail.dev/) - Process isolation
- [wasmtime](https://wasmtime.dev/) - WASM runtime used by BeeBotOS
- [gVisor](https://gvisor.dev/) - Alternative process sandbox (future)

---

*本文档遵循 BeeBotOS 技术设计规范。如有变更，请通过 PR 更新并同步修改本文件头部版本号。*
