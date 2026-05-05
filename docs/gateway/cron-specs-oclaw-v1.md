
根据官方文档和社区资料，OpenClaw Web 页面的**定时任务栏（定时器/Scheduler）**是 Gateway 内置的调度系统，用于让 AI Agent 实现自主定时工作。以下是详细功能说明：

---

## 一、核心架构

**运行位置**：定时任务运行在 **Gateway 网关内部**（而非模型内部），不依赖外部 cron 服务 。

**持久化存储**：所有任务定义存储在 `~/.openclaw/cron/jobs.json`，运行时状态存储在 `jobs-state.json`。重启 Gateway 后计划不会丢失 。

---

## 二、三种调度类型

| 类型 | 说明 | 适用场景 |
|------|------|----------|
| **`at`** | 一次性任务，在指定 ISO 8601 时间点执行 | 提醒、单次未来事件 |
| **`every`** | 固定间隔重复，如 `30m`、`1h`、`4h`、`1d` | 健康检查、定期扫描 |
| **`cron`** | 标准 5 字段 Cron 表达式，支持时区 | 每日晨报、每周总结 |

示例：
```
# 每天早上7点执行
0 7 * * *

# 每30分钟执行一次
*/30 * * * *

# 工作日早上9点
0 9 * * 1-5
```

---

## 三、两种执行模式

### 1. 主会话模式（Main Session）
- 将系统事件排入主会话队列，在**下一次心跳时**运行
- Agent 能看到完整的对话上下文和历史记录
- 适合需要了解当前工作进展的自动化任务

### 2. 隔离会话模式（Isolated Session）
- 在独立的 `cron:<jobId>` 会话中运行，**无历史上下文**
- 每次执行都是全新的 Agent 轮次
- 适合频繁任务、后台监控、晨报生成等**不干扰主对话**的场景
- 默认开启结果投递（`--announce`），可用 `--no-deliver` 关闭 

---

## 四、唤醒机制（Wakeups）

定时任务支持两种唤醒策略，这是一等公民功能 ：

- **`--wake now`**：任务到期时**立即唤醒** Agent 执行
- **默认/下次心跳**：将事件排入队列，等待下一次心跳周期处理

---

## 五、结果投递（Delivery）

任务执行完成后，可选择将结果自动发送到多个渠道 ：

| 渠道 | 配置示例 |
|------|----------|
| Slack | `--channel slack --to "channel:C1234567890"` |
| Discord | `--channel discord` |
| Telegram | `--channel telegram --to "123456789"` |
| 邮件 | `--channel email` |
| Webhook | `delivery.mode = "webhook"` |
| WhatsApp | `--channel whatsapp` |

**智能投递**：Agent 回复 `HEARTBEAT_OK` 表示无需关注的内容会自动被抑制，只投递真正的告警和有价值信息 。

---

## 六、Web 页面管理功能

通过 Web UI 可以完成以下操作：

| 功能 | 说明 |
|------|------|
| **任务列表** | 查看所有定时任务的状态、下次执行时间、最近运行结果 |
| **创建任务** | 填写名称、选择调度类型（at/every/cron）、设置执行模式、填写提示词 |
| **编辑任务** | 修改调度表达式、提示词、投递渠道等（注意：编辑时可能触发 `nextRunAtMs` 重置问题，见下方） |
| **手动执行** | 点击"立即运行"强制触发某任务 |
| **查看历史** | 查看任务的执行记录、输出日志、成功/失败状态 |
| **启用/禁用** | 临时暂停某个任务而不删除 |

---

## 七、CLI 命令对照

Web 页面的功能对应以下 CLI 命令：

```bash
# 查看状态
openclaw cron status
openclaw cron list

# 创建一次性提醒
openclaw cron add \
  --name "Reminder" \
  --at "2026-05-07T09:00:00Z" \
  --session main \
  --system-event "提醒：检查今日待办" \
  --wake now \
  --delete-after-run

# 创建周期性隔离任务并投递到 Slack
openclaw cron add \
  --name "Morning brief" \
  --cron "0 7 * * *" \
  --tz "Asia/Shanghai" \
  --session isolated \
  --message "总结昨晚的更新" \
  --announce \
  --channel slack \
  --to "channel:C1234567890"

# 手动运行
openclaw cron run <job-id> --force

# 查看执行历史
openclaw cron runs --id <job-id>

# 删除任务
openclaw cron remove <job-id>
```

---

## 八、高级特性

### 1. 自动错峰执行
为避免全球大量实例在整点同时触发造成流量峰值，OpenClaw 从 v2026.2.17 起对整点任务自动应用最多 5 分钟的随机错峰窗口 。
- 强制精确时间：`--exact`
- 自定义错峰：`--stagger 30s`

### 2. 错误重试机制
连续失败时采用指数退避策略：`30s → 1m → 5m → 15m → 60m`，下次成功后恢复正常调度 。

### 3. 超时处理
隔离任务到达 `timeoutSeconds` 后，Cron 会中止底层 Agent 运行并预留短暂清理窗口 。

### 4. 数据保留策略
- `cron.sessionRetention`（默认 24h）：清理已完成的隔离运行会话
- `cron.runLog.maxBytes` + `keepLines`：控制运行日志大小 

---

## 九、已知问题与注意事项

1. **编辑后调度重置**：通过 `openclaw cron edit` 编辑任务时，`nextRunAtMs` 会被重置为 0，导致任务被立即调度或投递被跳过。建议编辑后手动检查调度时间 。
2. **时区必须显式设置**：OpenClaw 不会继承系统默认时区，需在任务中通过 `--tz` 明确指定 。
3. **心跳 vs Cron 选择**：如果任务需要 Agent 主动感知状态变化，考虑使用 Heartbeat 机制；纯时间驱动则用 Cron 。

---

## 十、典型应用场景

- **每日晨报**：每天早上 8 点自动抓取新闻、GitHub Trending、技术博客，生成摘要推送到飞书/Slack
- **价格监控**：每小时检查商品价格，达到目标价时通知
- **博客更新监控**：每天两次检查关注的技术博客，有新文章时推送标题和链接
- **系统健康检查**：每 30 分钟检查服务器状态，异常时告警
- **招聘监控**：每天三次监控目标公司招聘页，匹配关键词时通知

---

如需了解具体某个功能（如 Webhook 投递配置、JSON Schema 定义、或与其他系统的集成），可以进一步说明。
