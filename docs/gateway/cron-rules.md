# BeeBotOS Cron 定时任务规则文档

> 版本: v1.0  
> 日期: 2026-05-05  
> 作者: BeeBotOS Team  

## 1. 概述

BeeBotOS 的独立 Cron 定时任务系统（非 Workflow Trigger Cron）提供了一套完整的定时任务管理能力，允许用户创建、调度、监控和管理周期性执行的自动化任务。

### 1.1 设计目标

- **独立性**: 与 Workflow Trigger Cron 完全解耦，独立存储、独立调度
- **灵活性**: 支持三种调度方式（Cron 表达式、固定间隔、定时一次）
- **可观测性**: 完整的执行历史、状态追踪、日志记录
- **安全性**: 基于现有认证体系，仅授权用户可操作

### 1.2 与 Workflow Trigger Cron 的区别

| 特性 | Workflow Trigger Cron | 独立 Cron 定时任务 |
|------|----------------------|-------------------|
| 用途 | 触发 Workflow 执行 | 直接向 Agent 发送 Prompt |
| 存储 | Workflow 定义中 | 独立 `cron_jobs` 表 |
| 调度器 | 共享 `tokio-cron-scheduler` | 共享 `tokio-cron-scheduler` |
| 上下文 | Workflow 实例上下文 | 独立会话或主会话共享 |
| 目标 | Workflow 步骤 | Agent 直接执行 |

## 2. 数据模型

### 2.1 Cron Job 表 (`cron_jobs`)

```sql
CREATE TABLE cron_jobs (
    id TEXT PRIMARY KEY,           -- UUID v4
    name TEXT NOT NULL,            -- 任务名称
    description TEXT DEFAULT '',   -- 任务描述
    schedule_type TEXT NOT NULL,   -- at | every | cron
    schedule_expr TEXT NOT NULL,   -- 调度表达式
    timezone TEXT DEFAULT 'UTC',   -- 时区
    prompt TEXT NOT NULL,          -- 执行提示词
    enabled INTEGER DEFAULT 1,     -- 是否启用
    context_mode TEXT DEFAULT 'isolated', -- main | isolated
    delivery_channel TEXT DEFAULT '',     -- 投递频道
    delivery_target TEXT DEFAULT '',      -- 投递目标
    max_runs INTEGER,              -- 最大运行次数（NULL=无限制）
    run_count INTEGER DEFAULT 0,   -- 已运行次数
    last_run_at TEXT,              -- 上次执行时间
    next_run_at TEXT,              -- 下次执行时间
    created_by TEXT NOT NULL,      -- 创建者用户ID
    created_at TEXT NOT NULL,      -- 创建时间
    updated_at TEXT NOT NULL       -- 更新时间
);
```

### 2.2 Cron Job Run 表 (`cron_job_runs`)

```sql
CREATE TABLE cron_job_runs (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES cron_jobs(id) ON DELETE CASCADE,
    status TEXT NOT NULL,          -- running | success | failed
    output TEXT DEFAULT '',        -- 执行输出
    error TEXT DEFAULT '',         -- 错误信息
    started_at TEXT NOT NULL,      -- 开始时间
    completed_at TEXT,             -- 完成时间
    triggered_by TEXT DEFAULT ''   -- 触发方式：cron | manual
);
```

## 3. 调度规则

### 3.1 调度类型

#### 3.1.1 Cron 表达式 (`cron`)

标准 5 字段 Unix Cron 表达式：

```
┌───────────── 分钟 (0 - 59)
│ ┌───────────── 小时 (0 - 23)
│ │ ┌───────────── 日期 (1 - 31)
│ │ │ ┌───────────── 月份 (1 - 12)
│ │ │ │ ┌───────────── 星期 (0 - 7, 0/7=周日)
│ │ │ │ │
* * * * *
```

**常用示例**:

| 表达式 | 含义 |
|--------|------|
| `*/5 * * * *` | 每 5 分钟 |
| `0 * * * *` | 每小时整点 |
| `0 9 * * *` | 每天上午 9:00 |
| `0 9 * * 1-5` | 工作日每天上午 9:00 |
| `0 0 * * 0` | 每周日午夜 |
| `0 0 1 * *` | 每月 1 日午夜 |

#### 3.1.2 固定间隔 (`every`)

简化的时间间隔格式：

| 格式 | 含义 |
|------|------|
| `30s` | 每 30 秒 |
| `5m` | 每 5 分钟 |
| `30m` | 每 30 分钟 |
| `1h` | 每小时 |
| `4h` | 每 4 小时 |
| `1d` | 每天 |

**转换规则**:
- `< 60m` → `*/N * * * *` (每分钟触发)
- `>= 60m` → `N */H * * *` (每小时触发)
- `h` → `0 */N * * *`
- `d` → `0 0 */N * *`

#### 3.1.3 定时一次 (`at`)

ISO 8601 格式的时间点：

```
2026-05-06T09:00:00Z
2026-05-06T09:00:00+08:00
```

**注意**: `at` 类型任务不会被注册到 tokio-cron-scheduler 的循环调度器中，需要由单独的机制处理（当前版本暂不自动执行一次性任务，建议创建后手动运行）。

### 3.2 时区处理

- 默认时区: `UTC`
- Cron 表达式计算时，先转换到目标时区，再匹配字段
- 所有存储时间均使用 UTC (RFC 3339)
- 前端显示时应转换到用户本地时区

### 3.3 执行约束

#### 3.3.1 最大运行次数 (`max_runs`)

- `NULL`: 无限制，持续执行
- `N > 0`: 最多执行 N 次，达到后自动跳过
- 达到上限的任务不会被自动禁用，仅跳过执行

#### 3.3.2 启用/禁用

- 禁用任务会从 tokio-cron-scheduler 中移除
- 启用任务会重新注册到调度器
- 状态变更立即生效

## 4. API 规范

### 4.1 路由列表

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/v1/cron/jobs` | 列出所有任务 |
| POST | `/api/v1/cron/jobs` | 创建任务 |
| GET | `/api/v1/cron/jobs/:id` | 获取任务详情 |
| PUT | `/api/v1/cron/jobs/:id` | 更新任务 |
| DELETE | `/api/v1/cron/jobs/:id` | 删除任务 |
| POST | `/api/v1/cron/jobs/:id/toggle` | 切换启用状态 |
| POST | `/api/v1/cron/jobs/:id/run` | 手动触发执行 |
| GET | `/api/v1/cron/jobs/:id/runs` | 获取执行历史 |

### 4.2 请求/响应示例

#### 创建任务

```http
POST /api/v1/cron/jobs
Content-Type: application/json

{
  "name": "每日晨报",
  "description": "每天早上 9 点生成晨报",
  "schedule_type": "cron",
  "schedule_expr": "0 9 * * *",
  "timezone": "Asia/Shanghai",
  "prompt": "请生成一份今日早报，包含天气、新闻摘要和待办事项",
  "enabled": true,
  "context_mode": "isolated",
  "max_runs": null
}
```

#### 手动触发

```http
POST /api/v1/cron/jobs/:id/run
```

响应:
```json
{
  "success": true,
  "run_id": "uuid",
  "message": "Job triggered"
}
```

## 5. 执行流程

### 5.1 自动触发流程

```
┌─────────────┐     ┌──────────────────┐     ┌──────────────┐
│ tokio-cron- │────▶│ 检查 max_runs    │────▶│ record_run_  │
│ scheduler   │     │ 和 enabled       │     │ start()      │
└─────────────┘     └──────────────────┘     └──────────────┘
                                                      │
                                                      ▼
┌─────────────┐     ┌──────────────────┐     ┌──────────────┐
│ record_run_ │◀────│ 执行 Prompt 通过 │◀────│ Message      │
│ complete()  │     │ MessageProcessor │     │ Processor    │
└─────────────┘     └──────────────────┘     └──────────────┘
```

### 5.2 手动触发流程

1. 用户点击"运行"按钮
2. 后端立即创建 run 记录（status=running）
3. 异步 spawn 执行任务
4. 任务完成后更新 run 记录
5. 前端通过轮询或刷新查看结果

## 6. 上下文模式

### 6.1 独立会话 (`isolated`)

- 每次执行创建独立的 channel/session
- 不共享历史上下文
- 适合无状态、幂等的任务

### 6.2 主会话共享 (`main`)

- 使用共享的 cron channel（如 `cron:{job_id}`）
- 保留执行历史上下文
- 适合需要记忆连续状态的任务

## 7. 前端界面

### 7.1 页面路径

- `/cron-jobs` — 定时任务管理页面

### 7.2 功能清单

- [x] 任务列表（名称、调度方式、表达式、状态、运行次数、下次执行）
- [x] 新建/编辑任务（模态框表单）
- [x] 启用/禁用切换
- [x] 手动运行
- [x] 查看执行历史
- [x] 删除任务
- [x] 自动轮询刷新

### 7.3 Sidebar 入口

位于"工作流"下方，图标 ⏰，标签"定时任务"。

## 8. 安全与权限

- 所有 API 需要认证（`user` 或 `admin` 角色）
- 使用现有 `AuthGuard` 中间件
- 创建者信息记录在 `created_by` 字段
- 当前版本不做额外的创建者权限校验（所有认证用户可管理全部任务）

## 9. 部署与迁移

### 9.1 数据库迁移

执行 `migrations_sqlite/018_add_cron_jobs.sql`：

```bash
# 自动（Gateway 启动时）
cargo run -p beebotos-gateway

# 手动
sqlite3 data/beebotos.db < migrations_sqlite/018_add_cron_jobs.sql
```

### 9.2 启动流程

1. Gateway 启动时初始化 `CronJobService`
2. 读取所有 `enabled = 1` 的任务
3. 排除 `schedule_type = 'at'` 的任务
4. 将剩余任务注册到 `tokio-cron-scheduler`
5. 启动调度器

## 10. 高级功能

### 10.1 `at` 类型一次性任务自动执行

`at` 类型任务不注册到 `tokio-cron-scheduler` 的循环调度器中，而是由独立的后台检查循环处理：

- **检查间隔**：每 30 秒检查一次数据库
- **执行条件**：`enabled = 1` AND `schedule_type = 'at'` AND `next_run_at <= NOW()`
- **执行后处理**：自动禁用任务（`enabled = 0`），避免重复执行
- **启动位置**：`main.rs` 中随 Gateway 启动而启动

### 10.2 执行超时控制

所有 cron job 执行均受 60 秒超时保护：

```rust
tokio::time::timeout(Duration::from_secs(60), execute_cron_job_inner(...))
```

- 超时后记录状态为 `timeout`
- 超时同样触发通知投递
- 防止 Agent 长时间挂起占用资源

### 10.3 失败重试（指数退避）

自动触发（cron/every 类型）失败后的重试策略：

| 失败次数 | 退避时间 |
|---------|---------|
| 1st     | 2 分钟  |
| 2nd     | 4 分钟  |
| 3rd     | 8 分钟  |

- **重试窗口**：仅统计最近 24 小时内的失败次数
- **最大重试**：3 次，超过后自动禁用任务
- **重试方式**：更新 `next_run_at` 字段，由下次调度器触发时执行
- **手动触发**：不触发自动重试（用户手动重试不受限制）

### 10.4 通知投递

任务执行完成后，根据 `delivery_channel` 和 `delivery_target` 发送结果通知：

| 投递频道 | `delivery_target` 含义 | 说明 |
|---------|----------------------|------|
| `webchat` | WebSocket 频道名（默认 `webchat`） | 通过 WebSocket 广播到订阅客户端 |
| `webhook` | HTTP POST URL | 发送 JSON payload，超时 10 秒 |
| （空）   | — | 不发送通知 |

**Webhook Payload 格式**：
```json
{
  "job_id": "uuid",
  "job_name": "任务名称",
  "status": "success|failed|timeout",
  "output": "执行输出",
  "error": "错误信息",
  "timestamp": "2026-05-05T12:00:00Z"
}
```

## 11. 未来扩展

- [ ] 更丰富的投递频道支持（飞书、钉钉、Telegram、邮件等）
- [ ] 任务分类/标签
- [ ] 执行统计图表
- [ ] 任务依赖链（DAG）
- [ ] 运行时动态修改调度表达式
