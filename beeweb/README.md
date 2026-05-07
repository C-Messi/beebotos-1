# BeeWeb Server - BeeBotOS 远程软件升级服务

BeeWeb Server 是 BeeBotOS 的远程软件升级 (OTA) 服务端，为 Gateway、CLI、Web 三个应用提供统一的版本检查、升级包分发和升级状态上报服务。

## 功能特性

- **版本检查 API**: `GET /api/v1/updates/check`
- **升级包下载**: `GET /api/v1/updates/download/{package_id}` (支持断点续传)
- **升级状态上报**: `POST /api/v1/updates/report`
- **Prometheus 指标**: `GET /metrics`
- **Ed25519 签名验证**: 内置签名验证服务
- **多平台支持**: Windows、Linux、macOS、WASM
- **多频道管理**: stable、beta、nightly

## 快速开始

### 构建

```bash
cd /root/beebotos
cargo build -p beebotos-beeweb
```

### 运行

```bash
# 默认端口 8080
cargo run -p beebotos-beeweb

# 自定义端口
BEEWEB_PORT=9000 cargo run -p beebotos-beeweb
```

### API 示例

#### 检查更新

```bash
curl "http://localhost:8080/api/v1/updates/check?app=gateway&version=1.0.0&channel=stable"
```

#### 上报升级状态

```bash
curl -X POST "http://localhost:8080/api/v1/updates/report" \
  -H "Content-Type: application/json" \
  -d '{
    "app_name": "gateway",
    "device_id": "dev_001",
    "current_version": "1.0.0",
    "target_version": "1.1.0",
    "status": "completed",
    "duration_secs": 120
  }'
```

#### 下载升级包 (支持断点续传)

```bash
curl -H "Range: bytes=0-1048575" \
  "http://localhost:8080/api/v1/updates/download/gateway-1.1.0-linux-amd64" \
  -o package.bin
```

#### 查看 Prometheus 指标

```bash
curl "http://localhost:8080/metrics"
```

## 项目结构

```
beebotos/beeweb/
├── Cargo.toml          # 项目配置
├── README.md           # 本文档
└── src/
    ├── main.rs         # 服务入口、路由配置
    ├── models.rs       # 数据模型 (VersionInfo, PackageInfo, UpdateState 等)
    ├── handlers.rs     # HTTP 请求处理
    ├── storage.rs      # 数据存储层 (内存存储 + DashMap)
    ├── signature.rs    # Ed25519 签名验证服务
    └── metrics.rs      # Prometheus 指标收集
```

## 数据模型

### VersionInfo

| 字段 | 类型 | 说明 |
|------|------|------|
| version | SemVer | 语义化版本号 |
| released_at | DateTime | 发布日期 |
| mandatory | bool | 是否强制更新 |
| min_supported_version | SemVer | 最低支持版本 |
| priority | UpdatePriority | 更新优先级 |
| release_notes | HashMap | 多语言发布说明 |
| packages | Vec<PackageInfo> | 升级包列表 |
| metadata | UpdateMetadata | 扩展元数据 |

### PackageInfo

| 字段 | 类型 | 说明 |
|------|------|------|
| id | String | 包唯一标识 |
| platform | Platform | 目标平台 |
| package_type | PackageType | 包类型 (Full/Delta/Patch) |
| download_url | String | 下载地址 |
| hash | String | SHA-256 哈希 |
| size | u64 | 包大小 (字节) |
| signature | String | Ed25519 数字签名 |
| base_version | Option<SemVer> | 增量更新基准版本 |

## 环境变量

| 变量名 | 说明 | 默认值 |
|--------|------|--------|
| BEEWEB_PORT | 服务监听端口 | 8080 |
| BEEWEB_PACKAGES_DIR | 升级包存储目录 | /data/packages |

## 监控指标

| 指标名 | 类型 | 说明 |
|--------|------|------|
| update_check_total | Counter | 版本检查次数 |
| update_available_total | Counter | 检测到新版本次数 |
| update_download_bytes_total | Counter | 总下载字节数 |
| update_download_duration_seconds | Histogram | 下载耗时 |
| update_install_duration_seconds | Histogram | 安装耗时 |
| update_success_total | Counter | 升级成功次数 |
| update_failure_total | Counter | 升级失败次数 |
| update_rollback_total | Counter | 回滚次数 |
| update_current_version | Gauge | 当前版本号 |

## 与设计文档的对应关系

本实现严格遵循 `docs/evolution/remote-software-upgrade-design.md` 的设计要求：

- [x] Version API - 版本检查服务
- [x] Package API - 升级包分发 (支持 Range 断点续传)
- [x] Signature - Ed25519 签名验证服务
- [x] Update Metrics - Prometheus 指标和升级统计服务
- [x] 数据模型 - VersionInfo、PackageInfo、UpdateState 等
- [x] 错误处理 - UpdateError 枚举及 HTTP 状态码映射
