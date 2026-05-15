# Foreign Runtime 运行时资源下载与构建指南

## 1. Pyodide WASM 模块

### 方案 A：官方 CDN 直接下载（推荐测试用）

```bash
mkdir -p /opt/beebotos/wasm-modules

# Pyodide 核心 WASM 模块（约 10-15MB）
wget -O /opt/beebotos/wasm-modules/pyodide.asm.wasm \
  https://cdn.jsdelivr.net/pyodide/v0.25.1/full/pyodide.asm.wasm

# 可选：Python 标准库包（需要联网时下载）
wget -O /opt/beebotos/wasm-modules/pyodide-core-0.25.1.tar.bz2 \
  https://cdn.jsdelivr.net/pyodide/v0.25.1/full/pyodide-core-0.25.1.tar.bz2

# 解压标准库
mkdir -p /opt/beebotos/pyodide-packages
tar -xjf /opt/beebotos/wasm-modules/pyodide-core-0.25.1.tar.bz2 \
  -C /opt/beebotos/pyodide-packages
```

### 方案 B：GitHub Releases 下载

```bash
# 官方 GitHub 仓库
# https://github.com/pyodide/pyodide/releases

wget -O /tmp/pyodide-0.25.1.tar.bz2 \
  https://github.com/pyodide/pyodide/releases/download/0.25.1/pyodide-0.25.1.tar.bz2

tar -xjf /tmp/pyodide-0.25.1.tar.bz2 -C /opt/beebotos/wasm-modules
```

### 方案 C：使用 Docker 快速获取

```bash
docker run --rm -v /opt/beebotos/wasm-modules:/output \
  pyodide/pyodide-env:20240101 \
  bash -c "cp /src/pyodide.asm.wasm /output/"
```

---

## 2. QuickJS WASM 模块

⚠️ **QuickJS 官方没有预构建的 WASI 版本，必须自行编译。**

### 方案 A：使用 wasi-sdk 编译（推荐）

```bash
# 1. 安装 wasi-sdk
cd /tmp
wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-21/wasi-sdk-21.0-linux.tar.gz
tar -xzf wasi-sdk-21.0-linux.tar.gz
sudo mv wasi-sdk-21.0 /opt/wasi-sdk

# 2. 下载 QuickJS 源码
git clone https://github.com/bellard/quickjs.git /tmp/quickjs
cd /tmp/quickjs

# 3. 创建 WASM 编译 Makefile 补丁
cat > Makefile.wasi << 'EOF'
# QuickJS WASI build
WASI_SDK=/opt/wasi-sdk
CC=$(WASI_SDK)/bin/clang
AR=$(WASI_SDK)/bin/llvm-ar
CFLAGS=-O2 -DCONFIG_VERSION=\"2024-01-13\" -DCONFIG_BIGNUM
LDFLAGS=-Wl,--no-entry -Wl,--export-dynamic

all: qjs.wasm

qjs.wasm: quickjs.c libquickjs.a
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ quickjs.c libquickjs.a \
		-Wl,--export=main -Wl,--allow-undefined

libquickjs.a: quickjs.o libregexp.o libunicode.o cutils.o quickjs-libc.o
	$(AR) rcs $@ $^

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f *.o *.a *.wasm
EOF

# 4. 编译
make -f Makefile.wasi

# 5. 复制到目标目录
mkdir -p /opt/beebotos/wasm-modules
cp /tmp/quickjs/qjs.wasm /opt/beebotos/wasm-modules/
```

### 方案 B：使用 quickjs-emscripten（更简单但非 WASI）

```bash
# 使用 npm 安装预构建版本（基于 Emscripten，需要适配）
npm install -g quickjs-emscripten

# 查找安装路径
find $(npm root -g)/quickjs-emscripten -name "*.wasm" 2>/dev/null

# 注意：此版本使用 Emscripten 而非 WASI，可能需要修改 executor 的 WASI 初始化逻辑
```

### 方案 C：使用 QuickJS-ng + wasm32-wasi target

```bash
# 1. 安装 Rust wasm32-wasi target
rustup target add wasm32-wasi

# 2. 下载 QuickJS-ng（社区维护的 QuickJS 分支）
git clone https://github.com/quickjs-ng/quickjs.git /tmp/quickjs-ng
cd /tmp/quickjs-ng

# 3. 使用 cargo wasi 构建（需要安装 cargo-wasi）
cargo install cargo-wasi

# 或者手动用 clang 编译
cat > build_wasi.sh << 'EOF'
#!/bin/bash
set -e

WASI_SDK=/opt/wasi-sdk
export CC="$WASI_SDK/bin/clang"
export AR="$WASI_SDK/bin/llvm-ar"
export CFLAGS="-O2 -DCONFIG_VERSION=\"2024-02-14\""
export LDFLAGS="-Wl,--no-entry -Wl,--export-dynamic"

# 编译静态库
$CC $CFLAGS -c quickjs.c -o quickjs.o
$CC $CFLAGS -c libregexp.c -o libregexp.o
$CC $CFLAGS -c libunicode.c -o libunicode.o
$CC $CFLAGS -c cutils.c -o cutils.o

$AR rcs libquickjs.a quickjs.o libregexp.o libunicode.o cutils.o

# 编译可执行 WASM
$CC $CFLAGS $LDFLAGS -o qjs.wasm \
    qjs.c repl.c libquickjs.a \
    -Wl,--export=main \
    -Wl,--allow-undefined
EOF

chmod +x build_wasi.sh
./build_wasi.sh

cp qjs.wasm /opt/beebotos/wasm-modules/
```

---

## 3. Process Path Rootfs

### 方案 A：使用系统解释器快速测试（⚠️ 非生产安全）

```bash
# 最简单的测试方法，直接使用宿主系统的 Python/Node.js
# 但失去了隔离性，仅用于功能验证

mkdir -p /var/lib/beebotos/rootfs/python/opt/python/bin
ln -sf $(which python3) /var/lib/beebotos/rootfs/python/opt/python/bin/python3

mkdir -p /var/lib/beebotos/rootfs/nodejs/opt/nodejs/bin
ln -sf $(which node) /var/lib/beebotos/rootfs/nodejs/opt/nodejs/bin/node
```

### 方案 B：Docker 导出（推荐）

```bash
# Python rootfs
docker run --rm -v /var/lib/beebotos/rootfs/python:/output \
  python:3.11-slim \
  bash -c "
    mkdir -p /output/opt/python && 
    cp -r /usr/bin/python3 /output/opt/python/bin/ 2>/dev/null || 
    cp -r /usr/local/bin/python3 /output/opt/python/bin/ 2>/dev/null || 
    cp -r /usr/bin/python* /output/opt/python/bin/ 2>/dev/null
  "

# Node.js rootfs
docker run --rm -v /var/lib/beebotos/rootfs/nodejs:/output \
  node:20-slim \
  bash -c "
    mkdir -p /output/opt/nodejs/bin && 
    cp -r /usr/local/bin/node /output/opt/nodejs/bin/
  "
```

### 方案 C：debootstrap 创建最小 rootfs（Linux 专用）

```bash
# 安装 debootstrap
sudo apt-get install -y debootstrap

# 创建最小 Debian rootfs（约 200MB）
sudo debootstrap --variant=minbase bookworm \
  /var/lib/beebotos/rootfs/python \
  http://deb.debian.org/debian

# 在 rootfs 内安装 Python
sudo chroot /var/lib/beebotos/rootfs/python \
  apt-get install -y python3 python3-pip

# 创建符合代码预期的目录结构
sudo mkdir -p /var/lib/beebotos/rootfs/python/opt/python/bin
sudo ln -sf /usr/bin/python3 /var/lib/beebotos/rootfs/python/opt/python/bin/python3
```

### 方案 D：使用 Alpine minirootfs（最轻量，约 5MB）

```bash
# 下载 Alpine minirootfs
wget https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/alpine-minirootfs-3.19.0-x86_64.tar.gz

# 解压为 Python rootfs
mkdir -p /var/lib/beebotos/rootfs/python
tar -xzf alpine-minirootfs-3.19.0-x86_64.tar.gz \
  -C /var/lib/beebotos/rootfs/python

# 安装 Python
sudo chroot /var/lib/beebotos/rootfs/python apk add python3

# 创建符合预期的目录结构
sudo mkdir -p /var/lib/beebotos/rootfs/python/opt/python/bin
sudo ln -sf /usr/bin/python3 /var/lib/beebotos/rootfs/python/opt/python/bin/python3
```

---

## 4. nsjail 安装（可选但推荐）

```bash
# 方案 A：从源码编译
cd /tmp
git clone https://github.com/google/nsjail.git
cd nsjail
make
sudo cp nsjail /usr/local/bin/

# 方案 B：Docker 运行
docker pull ghostry/nsjail

# 方案 C：Ubuntu/Debian 包（如果可用）
sudo apt-get install -y nsjail  # 部分版本可用
```

---

## 5. 一键配置脚本

```bash
#!/bin/bash
set -e

echo "=== BeeBotOS Foreign Runtime 资源准备 ==="

# 创建目录
mkdir -p /opt/beebotos/wasm-modules
mkdir -p /var/lib/beebotos/rootfs/{python,nodejs}
mkdir -p /opt/beebotos/pyodide-packages

# 下载 Pyodide
echo "[1/4] 下载 Pyodide WASM..."
if [ ! -f /opt/beebotos/wasm-modules/pyodide.asm.wasm ]; then
    wget -q --show-progress -O /opt/beebotos/wasm-modules/pyodide.asm.wasm \
        https://cdn.jsdelivr.net/pyodide/v0.25.1/full/pyodide.asm.wasm
else
    echo "  Pyodide WASM 已存在，跳过"
fi

# QuickJS 需要自行编译，这里仅创建占位
echo "[2/4] QuickJS WASM 需要手动编译（见文档）"
echo "  请运行: rustup target add wasm32-wasi"
echo "  然后编译 QuickJS 源码"

# 创建测试用的 rootfs（使用系统解释器）
echo "[3/4] 创建测试 rootfs..."
if command -v python3 &> /dev/null; then
    mkdir -p /var/lib/beebotos/rootfs/python/opt/python/bin
    ln -sf $(which python3) /var/lib/beebotos/rootfs/python/opt/python/bin/python3
    echo "  Python rootfs: /var/lib/beebotos/rootfs/python"
fi

if command -v node &> /dev/null; then
    mkdir -p /var/lib/beebotos/rootfs/nodejs/opt/nodejs/bin
    ln -sf $(which node) /var/lib/beebotos/rootfs/nodejs/opt/nodejs/bin/node
    echo "  Node.js rootfs: /var/lib/beebotos/rootfs/nodejs"
fi

# 检查 nsjail
echo "[4/4] 检查 nsjail..."
if command -v nsjail &> /dev/null; then
    echo "  nsjail 已安装: $(which nsjail)"
else
    echo "  nsjail 未安装，将使用 unshare 回退"
fi

echo ""
echo "=== 配置完成 ==="
echo "请修改 Gateway 配置，指向以下路径："
echo "  WASM Pyodide: /opt/beebotos/wasm-modules/pyodide.asm.wasm"
echo "  WASM QuickJS: /opt/beebotos/wasm-modules/qjs.wasm (需手动编译)"
echo "  Process Python: /var/lib/beebotos/rootfs/python"
echo "  Process Node.js: /var/lib/beebotos/rootfs/nodejs"
```

---

## 6. 配置 Gateway 使用下载的资源

修改 `apps/gateway/src/main.rs` 中的初始化代码：

```rust
// 初始化 Foreign Runtime Manager
let foreign_rt_manager = {
    let mut config = beebotos_foreign_rt::ForeignRuntimeConfig::default();
    
    // WASM 路径配置
    config.wasm.pyodide_module_path = 
        Some("/opt/beebotos/wasm-modules/pyodide.asm.wasm".into());
    // config.wasm.quickjs_module_path = 
    //     Some("/opt/beebotos/wasm-modules/qjs.wasm".into());
    
    // Process 路径配置
    config.process.python_rootfs = 
        Some("/var/lib/beebotos/rootfs/python".into());
    config.process.nodejs_rootfs = 
        Some("/var/lib/beebotos/rootfs/nodejs".into());
    
    match beebotos_foreign_rt::DefaultForeignRuntimeManager::new(config) {
        Ok(manager) => {
            info!("✅ ForeignRuntimeManager initialized");
            Some(Arc::new(manager))
        }
        Err(e) => {
            warn!("⚠️ Failed to initialize ForeignRuntimeManager: {}", e);
            None
        }
    }
};
```


&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&

文档已保存到：
📄 `/root/beebotos/docs/evolution/python-nodjs/runtime-assets-download.md`

---

## 直接可用的下载命令

### 1. Pyodide WASM（Python 运行时）— 直接下载

```bash
mkdir -p /opt/beebotos/wasm-modules

wget -O /opt/beebotos/wasm-modules/pyodide.asm.wasm \
  https://cdn.jsdelivr.net/pyodide/v0.25.1/full/pyodide.asm.wasm
```

**来源：** [Pyodide 官方 CDN](https://pyodide.org/en/stable/usage/downloading-and-deploying.html)  
**大小：** 约 10-15MB

---

### 2. QuickJS WASM（Node.js 运行时）— **必须自己编译**

QuickJS 官方没有预构建的 WASI 版本。最快的方法：

```bash
# 1. 安装 wasi-sdk
cd /tmp
wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-21/wasi-sdk-21.0-linux.tar.gz
tar -xzf wasi-sdk-21.0-linux.tar.gz
sudo mv wasi-sdk-21.0 /opt/wasi-sdk

# 2. 下载源码并编译
git clone https://github.com/bellard/quickjs.git /tmp/quickjs
cd /tmp/quickjs

/opt/wasi-sdk/bin/clang -O2 \
  -DCONFIG_VERSION=\"2024-01-13\" \
  -Wl,--no-entry -Wl,--export-dynamic \
  -o qjs.wasm \
  quickjs.c libregexp.c libunicode.c cutils.c qjs.c repl.c \
  -Wl,--export=main -Wl,--allow-undefined

cp qjs.wasm /opt/beebotos/wasm-modules/
```

**来源：** [bellard/quickjs](https://github.com/bellard/quickjs)  
**替代：** [quickjs-ng/quickjs](https://github.com/quickjs-ng/quickjs)（社区活跃分支）

---

### 3. Process Rootfs — 用 Docker 最快

```bash
mkdir -p /var/lib/beebotos/rootfs/{python,nodejs}

# Python rootfs（从 Docker 导出）
docker run --rm -v /var/lib/beebotos/rootfs/python:/output \
  python:3.11-slim \
  bash -c "mkdir -p /output/opt/python/bin && cp $(which python3) /output/opt/python/bin/"

# Node.js rootfs
docker run --rm -v /var/lib/beebotos/rootfs/nodejs:/output \
  node:20-slim \
  bash -c "mkdir -p /output/opt/nodejs/bin && cp $(which node) /output/opt/nodejs/bin/"
```

**偷懒方案**（仅测试，无隔离）：
```bash
mkdir -p /var/lib/beebotos/rootfs/python/opt/python/bin
ln -sf $(which python3) /var/lib/beebotos/rootfs/python/opt/python/bin/python3

mkdir -p /var/lib/beebotos/rootfs/nodejs/opt/nodejs/bin
ln -sf $(which node) /var/lib/beebotos/rootfs/nodejs/opt/nodejs/bin/node
```

---

## nsjail 安装

```bash
# 从源码编译
cd /tmp && git clone https://github.com/google/nsjail.git
cd nsjail && make && sudo cp nsjail /usr/local/bin/

# 或 Ubuntu 直接装
sudo apt-get install -y nsjail 2>/dev/null || echo "请从源码编译"
```

---

## 配置 Gateway 指向这些路径

修改 `apps/gateway/src/main.rs` 约第 396 行：

```rust
let mut config = beebotos_foreign_rt::ForeignRuntimeConfig::default();

// WASM 路径
config.wasm.pyodide_module_path = 
    Some("/opt/beebotos/wasm-modules/pyodide.asm.wasm".into());
config.wasm.quickjs_module_path = 
    Some("/opt/beebotos/wasm-modules/qjs.wasm".into());

// Process 路径
config.process.python_rootfs = 
    Some("/var/lib/beebotos/rootfs/python".into());
config.process.nodejs_rootfs = 
    Some("/var/lib/beebotos/rootfs/nodejs".into());
```

然后重新编译运行 Gateway 即可。

%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%

你这个 `repl.c` 报错的直接原因是：QuickJS git 源码里只有 `repl.js`，`repl.c` 是构建时由本机版 `qjsc` 生成的。

先跑：

```bash
cd /root/beebotos/tmp/quickjs
make repl.c
```

我已经跑过了，`/root/beebotos/tmp/quickjs/repl.c` 已生成。不过继续用 wasi-sdk 编译时又会遇到 upstream QuickJS 的 WASI 兼容问题：`malloc_usable_size`、`setjmp.h`、`signal.h`、`termios.h` 等，不是简单补一个 `repl.c` 就能稳定产出 `qjs.wasm`。

我已更新文档，把原来那条会误导的 `clang ... repl.c ...` 命令改掉了，并加了说明：

- [runtime-assets-download.md](/root/beebotos/docs/evolution/python-nodjs/runtime-assets-download.md:48)：说明 BeeBotOS 需要 WASI command module
- [runtime-assets-download.md](/root/beebotos/docs/evolution/python-nodjs/runtime-assets-download.md:62)：解释 `repl.c` 必须由 `make repl.c` 生成
- [runtime-assets-download.md](/root/beebotos/docs/evolution/python-nodjs/runtime-assets-download.md:85)：记录后续 WASI 兼容阻塞点
- [runtime-assets-download.md](/root/beebotos/docs/evolution/python-nodjs/runtime-assets-download.md:398)：同步修正后面重复的“直接可用命令”段落

下一步建议走已有 WASI QuickJS 封装，或给 upstream QuickJS 做专门的 WASI patch，而不是继续硬怼这条单行 clang 命令。



&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&

可以直接拿 **QuickJS-NG release 里的 WASI command 产物**，这是目前最省事的路线。

BeeBotOS 现在需要的是 command 模式：模块导出 `_start`，一次启动执行 `/tmp/script.js`。所以选：

```text
qjs-wasi.wasm
```

不要选：

```text
qjs-wasi-reactor.wasm
```

reactor 是给 host 反复调用 `qjs_init/qjs_eval` 这种嵌入模式用的，和当前 `QuickJsExecutor` 不匹配。

**直接下载：**

```bash
mkdir -p /opt/beebotos/wasm-modules

curl -L \
  -o /opt/beebotos/wasm-modules/qjs.wasm \
  https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-wasi.wasm
```

或者固定版本，比如当前 QuickJS-NG GitHub 页面显示 latest 是 `v0.14.0`：

```bash
curl -L \
  -o /opt/beebotos/wasm-modules/qjs.wasm \
  https://github.com/quickjs-ng/quickjs/releases/download/v0.14.0/qjs-wasi.wasm
```

下载后先本地验证：

```bash
echo 'console.log(JSON.stringify({ ok: true, runtime: "quickjs-wasi" }))' > /tmp/script.js

wasmtime --dir /tmp::/tmp \
  /opt/beebotos/wasm-modules/qjs.wasm \
  -- /tmp/script.js
```

预期 stdout 类似：

```json
{"ok":true,"runtime":"quickjs-wasi"}
```

依据：QuickJS-NG 文档说 GitHub release 提供预构建二进制；其 WASI command 模式产物是 `qjs-wasi.wasm`，用于标准 `main/_start` 执行；`paralin/go-quickjs-wasi` 也明确说明 command model wasm 会在 `_start()` 里跑完。来源：QuickJS-NG installation/release docs、QuickJS-NG WASI docs、paralin/go-quickjs-wasi README。


&&&&&&&&&&&&&&&&&&&&&&&

可以，但要分清楚两种“调用”。

**1. tool 里的 `process_exec` / `bash_shell` 可以直接调用 `python3` 或 `node`**

代码里 `ProcessExecTool` 的描述就是：

```text
Execute an external command ... e.g. python3 script.py, node script.js
```

它实际执行的是：

```rust
sh -c "<command>"
```

所以如果宿主环境的 `PATH` 里有 `python3` 或 `node`，tool 可以这样跑：

```bash
python3 script.py
node script.js
```

但这走的是 **宿主机命令环境**，不是你新增的 foreign runtime rootfs / WASM 环境。

**2. 新增的 Python/Node.js foreign runtime 不是自动接入 tool CLI 的**

新增运行时现在主要通过这些入口使用：

- Gateway API：`POST /api/v1/tasks/execute-script`
- Agent TaskType：`ForeignPythonWasm`、`ForeignPythonProcess`、`ForeignNodeJsWasm`、`ForeignNodeJsProcess`

也就是说，tool 的普通 CLI 命令不会自动改成走：

```text
/var/lib/beebotos/rootfs/python
/var/lib/beebotos/rootfs/nodejs
/opt/beebotos/wasm-modules/qjs.wasm
/opt/beebotos/wasm-modules/pyodide.asm.wasm
```

除非你专门加一个新的 tool，比如 `foreign_runtime_exec`，让它调用 `DefaultForeignRuntimeManager` 或 Gateway 的 `/api/v1/tasks/execute-script`。

还有一个当前代码层面的坑：Gateway 里现在初始化的是：

```rust
ForeignRuntimeConfig::default()
```

默认配置里 `pyodide_module_path`、`quickjs_module_path`、`python_rootfs`、`nodejs_rootfs` 都是 `None`。所以 foreign runtime manager 虽然初始化了，但 Python/Node.js runtime 默认不可用，必须把这些路径真正写进配置或 main 初始化逻辑里。

简短结论：**tool CLI 可以跑宿主 `python3/node`；但不能自动使用新增的隔离 Python/Node.js 环境。要走新增环境，需要通过 foreign runtime API/Task，或新增一个专门的 tool 封装。**

&&&&&&&&&&&&&&&&&&&&&&&&&&&&


