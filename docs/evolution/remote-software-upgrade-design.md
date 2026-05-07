
# BeeBotOS 远程软件升级系统设计文档

## 1. 概述

### 1.1 文档信息

| 属性 | 值 |
|------|-----|
| 文档版本 | 1.0.0 |
| 创建日期 | 2026-04-13 |
| 适用范围 | Gateway, CLI, Web 三个应用 |
| 关联系统 | BeeWeb Server |

### 1.2 背景与目标

BeeBotOS 项目包含三个主要应用：
- **Gateway**: API网关服务，后台长期运行
- **CLI**: 命令行工具，用户本地执行
- **Web**: WebAssembly前端应用，浏览器中运行

为满足持续交付需求，需要设计统一的远程软件升级(Over-The-Air, OTA)系统。

### 1.3 核心原则

- **安全第一**: 所有升级包必须数字签名验证
- **最小侵入**: 升级过程不影响业务运行
- **可观测性**: 升级过程全程可监控、可审计
- **向后兼容**: 新版本必须兼容旧配置和数据


---

## 2. 系统架构

### 2.1 整体架构

```
BeeWeb Update Server
├── Version API     (版本检查服务)
├── Package API     (升级包分发)
├── Signature       (签名验证服务)
└── Update Metrics  (升级统计服务)

Gateway Application
├── Update Manager  (升级调度器)
├── Download Agent  (下载代理)
├── Signature Verifier (签名验证器)
├── Service Restarter  (服务重启器)
└── Backup Store    (备份存储)

CLI Application
├── Self-Update     (自升级模块)
├── Binary Swap     (二进制替换)
└── Config Migration (配置迁移)

Web Application
├── WASM Loader     (WASM加载器)
├── Cache Update    (缓存更新)
└── Hot Reload      (热重载)
```

### 2.2 组件职责

| 组件 | 位置 | 职责 |
|------|------|------|
| update-client | crates/update-client | 核心升级客户端库 |
| gateway-updater | apps/gateway/src/updater | Gateway升级逻辑 |
| cli-updater | apps/cli/src/commands/update.rs | CLI自升级命令 |
| web-updater | apps/web/src/utils/updater.rs | Web升级工具 |



---

## 3. 数据模型

### 3.1 版本信息 (VersionInfo)

```rust
pub struct VersionInfo {
    // 版本号 (语义化版本)
    pub version: SemVer,
    // 发布日期
    pub released_at: DateTime<Utc>,
    // 是否为强制更新
    pub mandatory: bool,
    // 最低支持版本
    pub min_supported_version: Option<SemVer>,
    // 更新优先级: Critical, High, Medium, Low
    pub priority: UpdatePriority,
    // 发布说明 (多语言)
    pub release_notes: HashMap<String, String>,
    // 更新包列表
    pub packages: Vec<PackageInfo>,
    // 元数据
    pub metadata: UpdateMetadata,
}
```

### 3.2 更新包信息 (PackageInfo)

```rust
pub struct PackageInfo {
    // 包ID
    pub id: String,
    // 目标平台: Windows/Linux/MacOS/WASM
    pub platform: Platform,
    // 包类型: Full/Delta/Patch
    pub package_type: PackageType,
    // 下载URL
    pub download_url: String,
    // 包哈希 (SHA-256)
    pub hash: String,
    // 包大小 (字节)
    pub size: u64,
    // 数字签名
    pub signature: String,
    // 增量更新基准版本
    pub base_version: Option<SemVer>,
}
```

### 3.3 更新状态 (UpdateState)

```rust
pub struct UpdateState {
    // 当前状态
    pub status: UpdateStatus,
    // 当前版本
    pub current_version: SemVer,
    // 目标版本
    pub target_version: Option<SemVer>,
    // 下载进度 (0-100)
    pub download_progress: u8,
    // 错误信息
    pub error: Option<String>,
    // 重试次数
    pub retry_count: u32,
}

pub enum UpdateStatus {
    Idle,           // 空闲
    Checking,       // 检查更新中
    Downloading,    // 下载中
    Verifying,      // 验证中
    Installing,     // 安装中
    Restarting,     // 重启中
    Completed,      // 完成
    Failed,         // 失败
    RolledBack,     // 已回滚
}
```



---

## 4. API 设计

### 4.1 BeeWeb Server API

#### 检查版本更新
```
GET /api/v1/updates/check

请求:
{
  "app_name": "gateway",
  "current_version": "1.0.0",
  "platform": "linux_amd64",
  "channel": "stable"
}

响应:
{
  "has_update": true,
  "version_info": {
    "version": "1.1.0",
    "mandatory": false,
    "priority": "high",
    "packages": [...]
  }
}
```

#### 下载更新包
```
GET /api/v1/updates/download/{package_id}

请求头:
- Range: bytes=0-1048575 (断点续传)
- Authorization: Bearer {token}

响应: 二进制数据流
```

#### 上报升级状态
```
POST /api/v1/updates/report

请求体:
{
  "app_name": "gateway",
  "device_id": "dev_xxx",
  "current_version": "1.0.0",
  "target_version": "1.1.0",
  "status": "completed",
  "duration_secs": 120
}
```

### 4.2 Gateway 内部 API

- GET /api/v1/system/updates/status - 查询更新状态
- POST /api/v1/system/updates/check - 触发更新检查
- POST /api/v1/system/updates/apply - 执行升级
- POST /api/v1/system/updates/rollback - 执行回滚



---

## 5. 升级流程

### 5.1 Gateway 升级流程

```
1. 版本检查
   └── 定时任务或手动触发
   └── 向 BeeWeb Server 发送当前版本信息

2. 下载包
   └── 根据平台选择正确的包
   └── 支持断点续传
   └── 并发下载优化

3. 安全验证
   └── SHA-256 哈希校验
   └── Ed25519 数字签名验证
   └── 证书链验证

4. 备份当前版本
   └── 复制当前可执行文件到备份目录
   └── 备份配置文件
   └── 创建还原点

5. 应用更新
   └── 启动新进程
   └── 优雅关闭旧进程
   └── 使用 Unix domain socket 协调

6. 健康检查
   └── 新进程启动后执行健康检查
   └── 验证 API 可用性
   └── 检查数据库连接

7. 回滚机制 (失败时)
   └── 健康检查失败自动回滚
   └── 手动触发回滚 API
   └── 保留最近3个版本备份
```

### 5.2 CLI 升级流程

```
1. 检查更新
2. 用户确认 (非强制更新)
3. 下载到临时目录
4. 验证签名
5. 获取当前可执行文件路径
6. 备份当前二进制
7. 替换二进制
   - Linux/macOS: renameat2 原子替换
   - Windows: MoveFileEx + 重启任务
8. 验证新版本
9. 清理临时文件
```

### 5.3 Web 应用升级流程

```
1. 页面加载时检查版本
2. 通过 Service Worker 拦截请求
3. 获取更新清单
4. 下载新 WASM (Streaming instantiation)
5. 仅下载变更的模块 (增量更新)
6. WebSocket 通知用户刷新页面
7. 用户确认后热重载
```



---

## 6. 安全设计

### 6.1 签名机制

```
Build Server (CI/CD)
    │
    ▼
Sign Package (Ed25519)
    │
    ├── 计算包 SHA-256 哈希
    ├── 使用私钥签名
    └── 生成签名数据
    │
    ▼
Upload to BeeWeb Server
```

签名格式:
```json
{
  "version": 1,
  "algorithm": "ed25519",
  "public_key": "base64_encoded_public_key",
  "signature": "base64_encoded_signature"
}
```

### 6.2 安全策略

| 安全措施 | 说明 |
|---------|------|
| HTTPS 强制 | 所有 API 通信必须使用 TLS 1.3 |
| 证书固定 | 客户端内置服务器证书公钥指纹 |
| 签名验证 | 所有更新包必须 Ed25519 签名验证 |
| 哈希校验 | SHA-256 完整性校验 |
| 降级防护 | 禁止降级到低于 min_supported_version 的版本 |
| 回滚保护 | 保留版本签名链，防止回滚攻击 |

### 6.3 验证流程

```rust
pub fn verify_package(package_path: &Path, signature: &str) -> Result<bool> {
    // 1. 计算包哈希
    let hash = sha256_file(package_path)?;
    
    // 2. 解析签名
    let sig = base64_decode(signature)?;
    
    // 3. 验证签名
    public_key.verify(&hash, &sig)?;
    
    Ok(true)
}
```



---

## 7. 错误处理与回滚

### 7.1 错误类型

```rust
pub enum UpdateError {
    Network(String),            // 网络错误
    Verification(String),       // 验证失败
    Installation(String),       // 安装失败
    Rollback(String),           // 回滚失败
    VersionNotSupported,        // 版本不支持
    InsufficientSpace,          // 磁盘空间不足
    PermissionDenied,           // 权限拒绝
    Timeout,                    // 超时
}
```

### 7.2 自动回滚策略

```
升级流程:
    │
    ├── 创建还原点
    │
    ├── 执行升级
    │   │
    │   ├── 成功 ──▶ 健康检查
    │   │               │
    │   │           成功 ──▶ 清理旧版本
    │   │               │
    │   │           失败 ──▶ 自动回滚
    │   │
    │   └── 失败 ──▶ 自动回滚
    │
    └── 回滚失败 ──▶ 告警 + 人工介入
```

### 7.3 回滚触发条件

| 条件 | 动作 |
|------|------|
| 健康检查失败 | 自动回滚 |
| 进程启动超时 | 自动回滚 |
| 用户手动触发 | 立即回滚 |
| 签名验证失败 | 拒绝升级，不执行 |

### 7.4 保留策略

- 保留最近 3 个版本的备份
- 自动清理超过 30 天的旧备份
- 强制更新包不占用备份配额



### 8.2 Cargo.toml 依赖

crates/update-client/Cargo.toml:
```toml
[package]
name = "beebotos-update-client"
version = "1.0.0"
edition = "2021"

[dependencies]
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio = { version = "1.36", features = ["fs", "process"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
ed25519-dalek = "2.1"
sha2 = "0.10"
base64 = "0.22"
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"
semver = "1.0"

[target."cfg(unix)".dependencies]
nix = { version = "0.28", features = ["process"] }

[target."cfg(windows)".dependencies]
windows-sys = { version = "0.52", features = ["Win32_System_Threading"] }
```


---

## 9. 配置示例

### 9.1 Gateway 配置 (config.toml)

```toml
[update]
enabled = true
server_url = "https://beeweb.beeagentos.ai"
channel = "stable"  # stable, beta, nightly

[update.schedule]
check_cron = "0 0 3 * * *"  # 每天凌晨 3 点检查
auto_download = true
auto_install = false

[update.security]
public_key_path = "data/certs/update_key.pub"
allow_downgrade = false
min_supported_version = "1.0.0"

[update.proxy]
enabled = false
# http_proxy = "http://proxy.company.com:8080"
```

### 9.2 CLI 配置

```toml
[update]
server_url = "https://beeweb.beeagentos.ai"
channel = "stable"
auto_check = true
```

### 9.3 环境变量

| 变量名 | 说明 | 默认值 |
|--------|------|--------|
| BEEBOTOS_UPDATE_SERVER | 升级服务器URL | - |
| BEEBOTOS_UPDATE_CHANNEL | 更新频道 | stable |
| BEEBOTOS_UPDATE_DISABLED | 禁用自动更新 | false |
| BEEBOTOS_UPDATE_PUBLIC_KEY | 公钥内容（覆盖文件） | - |
| HTTP_PROXY | 代理设置 | - |


---

## 10. 监控与指标

### 10.1 Prometheus 指标

| 指标名 | 类型 | 说明 |
|--------|------|------|
| update_check_total | Counter | 版本检查次数 |
| update_available_total | Counter | 检测到新版本次数 |
| update_download_bytes_total | Counter | 总下载字节数 |
| update_download_duration_seconds | Histogram | 下载耗时 |
| update_install_duration_seconds | Histogram | 安装耗时 |
| update_success_total | Counter | 升级成功次数 |
| update_failure_total | Counter | 升级失败次数（按错误类型区分） |
| update_rollback_total | Counter | 回滚次数 |
| update_current_version | Gauge | 当前版本号（标签形式） |

### 10.2 日志格式

```json
{
  "timestamp": "2026-04-13T06:15:21.355Z",
  "level": "INFO",
  "component": "update-client",
  "event": "update_check",
  "current_version": "1.0.0",
  "target_version": "1.1.0",
  "has_update": true,
  "mandatory": false
}
```

### 10.3 告警规则

| 告警名 | 条件 | 级别 |
|--------|------|------|
| UpdateCheckFailed | 连续3次检查失败 | Warning |
| UpdateInstallFailed | 升级失败率 > 10% | Critical |
| UpdateRollbackOccurred | 发生回滚 | Warning |
| MandatoryUpdatePending | 强制更新超过24小时未安装 | Critical |
| UpdateServerUnreachable | 服务器不可达超过1小时 | Critical |


---

## 11. 代码实现示例

### 11.1 核心升级客户端接口

文件: crates/update-client/src/lib.rs

```rust
use std::path::Path;
use async_trait::async_trait;

/// 升级客户端 trait
#[async_trait]
pub trait UpdateClient: Send + Sync {
    /// 检查是否有可用更新
    async fn check_update(&self) -> Result<Option<VersionInfo>, UpdateError>;
    
    /// 下载更新包
    async fn download(&self, package: &PackageInfo, progress: DownloadProgress) 
        -> Result<PathBuf, UpdateError>;
    
    /// 验证包完整性
    async fn verify(&self, package_path: &Path, package: &PackageInfo) 
        -> Result<bool, UpdateError>;
    
    /// 执行安装
    async fn install(&self, package_path: &Path, info: &VersionInfo) 
        -> Result<(), UpdateError>;
    
    /// 执行回滚
    async fn rollback(&self) -> Result<(), UpdateError>;
    
    /// 获取当前状态
    fn state(&self) -> UpdateState;
}

/// 下载进度回调
pub trait DownloadProgress: Send + Sync {
    fn on_progress(&self, downloaded: u64, total: u64);
    fn on_complete(&self, path: &Path);
    fn on_error(&self, error: &UpdateError);
}
```

### 11.2 Gateway 升级服务

文件: apps/gateway/src/updater/service.rs

```rust
use beebotos_update_client::{UpdateClient, UpdateConfig};

pub struct GatewayUpdateService {
    client: Arc<dyn UpdateClient>,
    config: UpdateConfig,
    state: Arc<RwLock<UpdateState>>,
    backup_manager: BackupManager,
}

impl GatewayUpdateService {
    pub async fn new(config: UpdateConfig) -> Result<Self> {
        let client = create_update_client(&config).await?;
        Ok(Self {
            client,
            config,
            state: Arc::new(RwLock::new(UpdateState::default())),
            backup_manager: BackupManager::new(),
        })
    }
    
    /// 启动定时检查任务
    pub async fn start_scheduler(&self) {
        let cron = self.config.check_cron.clone();
        let client = self.client.clone();
        
        tokio::spawn(async move {
            loop {
                // 解析 cron 表达式并等待
                let next = parse_cron(&cron);
                tokio::time::sleep_until(next).await;
                
                // 执行检查
                if let Err(e) = Self::check_and_maybe_update(client.clone()).await {
                    tracing::error!("Update check failed: {}", e);
                }
            }
        });
    }
    
    /// 检查并可能执行更新
    async fn check_and_maybe_update(client: Arc<dyn UpdateClient>) -> Result<()> {
        if let Some(info) = client.check_update().await? {
            if info.mandatory || client.config().auto_install {
                Self::perform_update(client, info).await?;
            }
        }
        Ok(())
    }
    
    /// 执行更新流程
    async fn perform_update(
        client: Arc<dyn UpdateClient>, 
        info: VersionInfo
    ) -> Result<()> {
        // 1. 创建备份
        let backup = self.backup_manager.create_backup().await?;
        
        // 2. 下载
        let package = select_package(&info.packages)?;
        let path = client.download(&package, ProgressHandler).await?;
        
        // 3. 验证
        if !client.verify(&path, &package).await? {
            return Err(UpdateError::VerificationFailed);
        }
        
        // 4. 安装
        client.install(&path, &info).await?;
        
        // 5. 健康检查
        if !health_check().await? {
            // 回滚
            client.rollback().await?;
            return Err(UpdateError::HealthCheckFailed);
        }
        
        // 6. 清理
        self.backup_manager.cleanup_old_backups(3).await?;
        
        Ok(())
    }
}
```


### 11.3 CLI 升级命令

文件: apps/cli/src/commands/update.rs

```rust
use clap::Args;
use beebotos_update_client::{UpdateClient, ConsoleProgress};

#[derive(Args)]
pub struct UpdateArgs {
    /// 强制更新，跳过确认
    #[arg(long)]
    force: bool,
    
    /// 检查更新但不安装
    #[arg(long)]
    check: bool,
    
    /// 回滚到上一版本
    #[arg(long)]
    rollback: bool,
    
    /// 指定目标版本
    #[arg(long)]
    version: Option<String>,
}

pub async fn execute(args: UpdateArgs) -> Result<()> {
    let config = load_update_config()?;
    let client = create_update_client(config).await?;
    
    if args.rollback {
        return perform_rollback(client).await;
    }
    
    // 检查更新
    println!("Checking for updates...");
    let info = match client.check_update().await? {
        Some(info) => info,
        None => {
            println!("Already up to date!");
            return Ok(());
        }
    };
    
    print_update_info(&info);
    
    if args.check {
        return Ok(());
    }
    
    // 确认更新
    if !args.force && !info.mandatory {
        if !confirm_update(&info)? {
            println!("Update cancelled.");
            return Ok(());
        }
    }
    
    // 执行更新
    perform_self_update(client, info).await
}

async fn perform_self_update(client: impl UpdateClient, info: VersionInfo) -> Result<()> {
    // 选择适合当前平台的包
    let package = select_package(&info)?;
    
    // 下载
    println!("Downloading update...");
    let progress = ConsoleProgress::new();
    let temp_path = client.download(&package, progress).await?;
    
    // 验证
    println!("Verifying package...");
    if !client.verify(&temp_path, &package).await? {
        return Err(anyhow!("Package verification failed"));
    }
    
    // 获取当前可执行文件路径
    let current_exe = env::current_exe()?;
    let backup_path = get_backup_path(&current_exe);
    
    // 备份
    println!("Creating backup...");
    fs::copy(&current_exe, &backup_path).await?;
    
    // 替换二进制
    println!("Installing update...");
    replace_binary(&temp_path, &current_exe).await?;
    
    // 清理
    fs::remove_file(&temp_path).await?;
    
    println!("Update completed successfully!");
    println!("New version: {}", info.version);
    
    Ok(())
}

#[cfg(unix)]
async fn replace_binary(source: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    
    // 设置可执行权限
    let mut perms = fs::metadata(source).await?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(source, perms).await?;
    
    // 原子替换
    fs::rename(source, target).await?;
    
    Ok(())
}

#[cfg(windows)]
async fn replace_binary(source: &Path, target: &Path) -> Result<()> {
    // Windows: 需要特殊处理，因为文件可能被锁定
    // 方案1: 使用 MoveFileEx + MOVEFILE_DELAY_UNTIL_REBOOT
    // 方案2: 创建更新任务，在进程退出后替换
    
    // 这里使用方案2
    let update_script = create_update_script(source, target)?;
    
    // 启动脚本后退出当前进程
    std::process::Command::new("powershell")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-File")
        .arg(&update_script)
        .spawn()?;
    
    // 提醒用户重启 CLI
    println!("Please restart beebot to complete the update.");
    
    Ok(())
}
```


### 11.4 Web 升级工具

文件: apps/web/src/utils/updater.rs

```rust
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, Response, ServiceWorkerRegistration};
use leptos::*;

/// Web 应用升级管理器
pub struct WebUpdater {
    current_version: String,
    registration: Option<ServiceWorkerRegistration>,
}

impl WebUpdater {
    pub async fn new() -> Result<Self> {
        let window = window().ok_or("No window available")?;
        let navigator = window.navigator();
        
        // 获取 Service Worker 注册
        let registration = if let Ok(sw) = navigator.service_worker() {
            JsFuture::from(sw.ready()?).await
                .ok()
                .and_then(|r| r.dyn_into::<ServiceWorkerRegistration>().ok())
        } else {
            None
        };
        
        Ok(Self {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            registration,
        })
    }
    
    /// 检查更新
    pub async fn check_update(&self) -> Result<Option<VersionInfo>> {
        let manifest_url = format!(
            "{}/api/v1/updates/check?app=web&version={}",
            self.config.server_url,
            self.current_version
        );
        
        let response = gloo_net::http::Request::get(&manifest_url)
            .send()
            .await?;
        
        if !response.ok() {
            return Ok(None);
        }
        
        let info: VersionInfo = response.json().await?;
        
        if info.version.to_string() != self.current_version {
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }
    
    /// 应用更新 (通知用户刷新)
    pub fn prompt_update(&self, info: &VersionInfo) {
        // 显示更新通知 UI
        // 用户点击后刷新页面加载新版本
        
        let (show, set_show) = create_signal(true);
        
        view! {
            {move || show.get().then(|| view! {
                <div class="update-notification">
                    <p>"New version available: " {info.version.to_string()}</p>
                    <p>{info.release_notes.get("zh").cloned().unwrap_or_default()}</p>
                    <button on:click=move |_| {
                        set_show.set(false);
                        // 刷新页面
                        if let Some(window) = window() {
                            let _ = window.location().reload();
                        }
                    }>
                        "Update Now"
                    </button>
                    <button on:click=move |_| set_show.set(false)>
                        "Later"
                    </button>
                </div>
            })}
        }
    }
    
    /// 通过 Service Worker 预缓存新版本
    pub async fn precache_update(&self, info: &VersionInfo) -> Result<()> {
        if let Some(reg) = &self.registration {
            // 发送消息给 Service Worker 预缓存资源
            if let Some(sw) = reg.active() {
                let _ = sw.post_message(&JsValue::from_str(
                    &format!("{{\"action\":\"precache\",\"version\":\"{}\"}}", 
                        info.version)
                ));
            }
        }
        Ok(())
    }
}

/// 在应用启动时检查更新
pub async fn init_updater() {
    let updater = WebUpdater::new().await.ok()?;
    
    // 检查更新
    if let Ok(Some(info)) = updater.check_update().await {
        if info.mandatory {
            // 强制更新：立即刷新
            if let Some(window) = window() {
                let _ = window.location().reload();
            }
        } else {
            // 可选更新：显示通知
            updater.prompt_update(&info);
            // 后台预缓存
            let _ = updater.precache_update(&info).await;
        }
    }
}
```


---

## 12. 测试策略

### 12.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version_comparison() {
        let v1 = SemVer::parse("1.0.0").unwrap();
        let v2 = SemVer::parse("1.1.0").unwrap();
        assert!(v1 < v2);
    }
    
    #[test]
    fn test_signature_verification() {
        let verifier = SignatureVerifier::new(TEST_PUBLIC_KEY);
        let result = verifier.verify_package(TEST_PACKAGE_PATH, TEST_SIGNATURE);
        assert!(result.unwrap());
    }
    
    #[tokio::test]
    async fn test_download_with_resume() {
        let client = TestUpdateClient::new();
        let partial = create_partial_file().await;
        
        let result = client.download_with_resume(
            TEST_URL, 
            &partial, 
            TEST_TOTAL_SIZE
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(partial.metadata().await.unwrap().len(), TEST_TOTAL_SIZE);
    }
}
```

### 12.2 集成测试

```rust
#[tokio::test]
async fn test_full_update_flow() {
    // 启动 mock 更新服务器
    let mock_server = MockServer::start().await;
    
    // 配置更新客户端
    let config = UpdateConfig {
        server_url: mock_server.uri(),
        ..Default::default()
    };
    
    let client = UpdateClientImpl::new(config).await.unwrap();
    
    // Mock 响应
    Mock::given(method("GET"))
        .and(path("/api/v1/updates/check"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(json!({
                "has_update": true,
                "version_info": {
                    "version": "1.1.0",
                    "packages": [{
                        "id": "test_pkg",
                        "download_url": format!("{}/download", mock_server.uri()),
                        "hash": "abc123",
                        "signature": "sig456"
                    }]
                }
            })))
        .mount(&mock_server)
        .await;
    
    // 执行完整流程
    let info = client.check_update().await.unwrap().unwrap();
    assert_eq!(info.version.to_string(), "1.1.0");
    
    // ... 继续测试下载、验证、安装
}
```

### 12.3 E2E 测试

| 测试场景 | 步骤 | 预期结果 |
|----------|------|----------|
| 正常升级 | 1.启动旧版本 2.触发升级 3.验证新版本 | 成功升级到目标版本 |
| 断网恢复 | 1.开始下载 2.断开网络 3.恢复网络 | 从断点继续下载 |
| 签名验证失败 | 1.篡改包内容 2.尝试安装 | 拒绝安装，保留原版本 |
| 健康检查失败 | 1.安装损坏的包 2.启动失败 | 自动回滚到原版本 |
| 手动回滚 | 1.升级后触发回滚 | 成功回滚到上一版本 |
| 强制更新 | 1.发布强制更新 2.等待检查 | 自动下载并提示安装 |


---

## 13. 部署计划

### 13.1 阶段一: 基础组件 (Week 1-2)

- [ ] 创建 crates/update-client 项目结构
- [ ] 实现版本检查和比较逻辑
- [ ] 实现 HTTP 下载客户端（支持断点续传）
- [ ] 实现 Ed25519 签名验证
- [ ] 编写单元测试

**交付物:**
- update-client crate (可独立使用)
- 完整的测试覆盖

### 13.2 阶段二: Gateway 集成 (Week 3-4)

- [ ] 创建 apps/gateway/src/updater 模块
- [ ] 实现 GatewayUpdateService
- [ ] 添加 HTTP API 接口
- [ ] 实现定时任务调度器
- [ ] 实现优雅重启机制
- [ ] 集成到 Gateway 主流程

**交付物:**
- Gateway 自动更新功能
- REST API: /api/v1/system/updates/*
- WebSocket 实时推送

### 13.3 阶段三: CLI 集成 (Week 5)

- [ ] 实现 beebot update 子命令
- [ ] 实现自升级功能
- [ ] 平台特定处理 (Unix/Windows)
- [ ] 进度显示和交互
- [ ] 配置迁移支持

**交付物:**
- beebot update 命令
- beebot update --rollback 命令
- 跨平台支持

### 13.4 阶段四: Web 集成 (Week 6)

- [ ] 实现 WebUpdater 工具
- [ ] Service Worker 集成
- [ ] 增量更新支持
- [ ] 更新通知 UI
- [ ] 热重载机制

**交付物:**
- Web 自动更新检测
- 用户友好的更新提示

### 13.5 阶段五: BeeWeb Server 对接 (Week 7)

- [ ] 与 BeeWeb 团队对接 API
- [ ] 集成版本检查接口
- [ ] 集成包下载接口
- [ ] 实现上报接口
- [ ] 生产环境测试

**交付物:**
- 完整的端到端流程
- 生产环境验证

注意：需要开发beeweb server 代码，把beeweb server代码放在位置：/root/beebotos/beeweb

---

## 14. 风险与缓解措施

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| 升级失败导致服务不可用 | 高 | 中 | 1. 自动回滚机制 2. 健康检查 3. 备份保留 |
| 网络问题导致更新中断 | 中 | 高 | 1. 断点续传 2. 重试机制 3. 超时控制 |
| 签名密钥泄露 | 高 | 低 | 1. HSM 存储私钥 2. 定期轮换密钥 3. 证书链验证 |
| 更新服务器被攻击 | 高 | 低 | 1. 证书固定 2. 多镜像源 3. 本地缓存验证 |
| 用户数据丢失 | 高 | 低 | 1. 配置自动备份 2. 数据库迁移脚本 3. 回滚保留数据 |
| 版本兼容性问题 | 中 | 中 | 1. 自动化兼容性测试 2. Canary 发布 3. 版本兼容性矩阵 |

---

## 15. 总结

### 15.1 设计亮点

1. **统一架构**: 三个应用共享 update-client 核心库，保持一致性
2. **安全第一**: 多重验证机制确保升级包可信
3. **平滑升级**: Gateway 支持零停机升级，CLI 支持自升级，Web 支持热重载
4. **可观测**: 完整的监控指标和日志支持
5. **容错**: 自动回滚和手动回滚双重保障

### 15.2 关键决策

| 决策项 | 选择 | 理由 |
|--------|------|------|
| 签名算法 | Ed25519 | 安全性高，性能优秀 |
| 传输协议 | HTTPS + TLS 1.3 | 业界标准，安全可靠 |
| 增量更新 | 基于 bsdiff | 成熟方案，压缩率高 |
| 平台抽象 | trait-based | 灵活性高，易于测试 |
| 调度框架 | cron expression | 通用标准，表达力强 |

### 15.3 后续优化方向

1. **P2P 分发**: 大型网络中节点间共享更新包
2. **A/B 测试**: 支持灰度发布，按用户分组推送
3. **AI 预测**: 预测最佳升级时间窗口
4. **差分压缩**: 更高效的增量更新算法
5. **多方签名**: 多重签名增强安全性

---

## 附录

### A. 术语表

| 术语 | 说明 |
|------|------|
| OTA | Over-The-Air，远程无线升级 |
| Delta Update | 增量更新，只传输变更部分 |
| Canary | 金丝雀发布，小范围测试 |
| Rollback | 回滚，恢复到之前版本 |
| Graceful Restart | 优雅重启，不中断服务 |

### B. 参考文档

- [Semantic Versioning 2.0.0](https://semver.org/)
- [Ed25519](https://ed25519.cr.yp.to/)
- [The Update Framework (TUF)](https://theupdateframework.io/)









