
apps/cli模块，全面检查cli命令是否有缺失，不完整？因为beebotos项目新增很多新代码，全面检查，输出一份完善优化的技术方案，放在beebotos/docs/evolution目录下面。

用户要求我全面检查 beebotos 项目的 apps/cli 模块，看看 CLI 命令是否有缺失或不完整，因为项目新增了很多代码。然后输出一份完善优化的技术方案，放在 beebotos/docs/evolution 目录下。
之前 explore agent 已经对 apps/cli 做了初步探索，但用户这次要求更全面的检查。我需要：
1. 重新全面检查 apps/cli 的代码
2. 对比项目中其他模块（agents、gateway 等）的功能
3. 找出缺失的 CLI 命令
4. 撰写技术方案
让我先再次全面探索 apps/cli，同时对比其他模块的功能。

&&&&&&&&&&&&&&&&&&&&&&&&&&&&

技术方案已完成并保存到：

**`beebotos/docs/evolution/cli-audit-and-improvement-plan.md`**

---

## 审计核心发现

### 🔴 最严重问题：大量 CLI 命令调用"幻想端点"

CLI 中约 **40% 的命令**调用的 API 端点在 Gateway 中**根本不存在**，运行时必定返回 404。例如：

| CLI 命令 | 调用的路径 | 实际存在？ |
|---------|-----------|-----------|
| `brain status` | `GET /brain/status` | ❌ 不存在 |
| `gateway start` | `POST /gateway/start` | ❌ 不存在 |
| `security scan` | `POST /security/scan` | ❌ 不存在 |
| `session create` | `POST /sessions` | ❌ 不存在 |
| `memory index` | `POST /memory/index` | ❌ 不存在 |
| `infer text` | `POST /infer/text` | ❌ 不存在 |
| `deploy` / `propose` / `vote` | 全部 | ❌ 纯空壳 |

### 🔴 核心子系统零覆盖

| 子系统 | Gateway 端点 | CLI 命令 |
|--------|-------------|---------|
| **Workflow** | ✅ 10+ | ❌ **无** |
| **Cron Jobs** | ✅ 8 | ❌ **无** |
| **MCP** | ✅ 4 | ❌ **无** |
| **Planning** | ❌ 无独立端点 | ❌ **无** |
| **Composition** | ✅ 5 | ❌ **无** |
| **Webchat** | ✅ 16 | ❌ **无** |
| **Task Monitor** | ✅ 5 | ❌ **无** |
| **State Machine** | ✅ 9 | ❌ **无** |

### 🟡 路径不匹配

| CLI 调用 | Gateway 真实路径 |
|---------|-----------------|
| `POST /agents/{id}/pause` | `POST /api/v1/agents/:id/pause` |
| `GET /agents/{id}/logs` | `GET /api/v1/agents/:id/logs` |
| `POST /browser/eval` | `POST /api/v1/browser/evaluate` |

---

## 方案要点

文档提供了完整的 **4 阶段实施路线图**：

| 阶段 | 周期 | 目标 | 关键任务 |
|------|------|------|---------|
| **Phase 1** | 1-2 周 | P0 紧急修复 | 统一 API 路径、隐藏/删除 stub 命令 |
| **Phase 2** | 2-3 周 | P1 核心补充 | 新增 `workflow`、`cron`、`mcp`、`webchat` 等命令 |
| **Phase 3** | 1-2 周 | P2 增强补充 | 新增 `task`、`state`、`system`、`browser` 重构 |
| **Phase 4** | 后续迭代 | P3 高级功能 | `plan`、`evolution`、`auth` 等 |

**预期效果：** Phase 1 + Phase 2 完成后，CLI 的 Gateway 端点覆盖度从 **~27%** 提升到 **~80%**。


