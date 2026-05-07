

以下是 **Hermes Agent 系统** 核心功能的详细说明，涵盖用户提问处理、System Prompt 组装、Function Calling、Skills/MCP/Workflow 匹配，以及复杂问题的 Planning 推理机制。

---

## 一、用户提问处理流程

当用户通过任意入口（CLI、Telegram、Discord、Slack、Web UI 等）提交问题时，系统按以下流程处理 ：

```
用户输入 → Gateway 网关 → 会话路由 → AIAgent 实例化 → 构建 System Prompt → 
LLM 推理 → 解析意图/工具调用 → 执行工具 → 循环直至完成 → 输出格式化 → 持久化存储
```

**关键设计特点：**
- **多平台入口统一**：15+ 平台（CLI/TUI/消息平台/Cron/批处理/Web）共享同一个 `AIAgent` 核心，通过 Gateway 适配器将不同平台消息归一化为统一格式 
- **会话连续性**：Gateway 根据用户身份派生稳定的 `session_key`，从 SQLite SessionDB 中恢复历史上下文，确保跨平台对话不中断 
- **六种执行后端**：Local、Docker（推荐）、SSH、Daytona、Modal、Singularity，实现"思考层"与"执行层"物理隔离 

---

## 二、System Prompt 组装机制

System Prompt 不是静态文本，而是**动态模块化组装**的结果，由 `prompt_builder.py` 在每次会话初始化时构建（一次会话只构建一次）：

| 组件 | 来源 | 作用 |
|------|------|------|
| **SOUL.md** | 配置文件 | 定义 Agent 人格、行为准则、语气风格 |
| **MEMORY.md** | 持久记忆 | 跨会话事实（项目细节、技术决策、用户偏好） |
| **USER.md** | 用户建模 | 用户沟通风格、专业领域、工作模式 |
| **Skills 文档** | 按需加载 | 当前任务相关的技能说明书（SKILL.md） |
| **上下文文件** | 项目目录 | AGENTS.md、.hermes.md 等本地上下文 |
| **工具使用指南** | 系统生成 | 当前可用 Tools 的描述与调用规范 |
| **模型特定指令** | 自动适配 | 针对不同 LLM（GPT/Claude/Hermes 等）的优化提示 |

**设计原则**：通过编辑 Markdown 文件而非编写代码即可定制 Agent 行为，实现"提示即配置" 。

---

## 三、Function Calling 与工具调度系统

### 1. 自注册工具注册表（Self-registering Registry）

Hermes 采用**自发现机制**而非集中式清单：
- 工具通过装饰器或文件约定自动向 `Tools Registry` 注册
- 支持四种 API 模式自动检测：`chat_completions`、`codex_responses`、`anthropic_messages`、`bedrock_converse` 
- Agent 级工具（如 `execute_code`）可将多步管道压缩为单次推理调用

### 2. 工具集（Toolsets）与分层防御

| 层级 | 机制 | 说明 |
|------|------|------|
| **注册层** | 自注册 + Toolsets | 工具按功能域分组（如 `web_tools`、`code_tools`） |
| **执行层** | 6 种后端环境 | Docker 默认，支持降权、PID 限制、无特权升级 |
| **安全层** | 工具审批 | 破坏性操作（写文件、执行命令）需人工确认 |
| **输入层** | 不可信输入隔离 | 网页抓取等第三方内容视为不可信，禁止直接触发邮件/删除等操作  |

### 3. MCP（Model Context Protocol）集成

Hermes 同时作为 **MCP 客户端** 和 **MCP 服务器**：
- **客户端模式**：连接外部 MCP 服务器扩展工具能力（如文件系统、数据库、API 包装器）
- **服务器模式**：将 Hermes 自身工具暴露给 MCP 生态（如 Cursor、Claude Desktop）
- 内置 FastMCP 技能，支持构建、测试、部署 MCP 服务器，并自动配置到 `~/.hermes/config.yaml` 

---

## 四、Skills 系统（核心差异化功能）

Skills 是 Hermes 的"杀手级特性"，是可移植、可共享、可自改进的能力单元 ：

### 1. Skill 结构
每个 Skill 是一个目录，包含：
- **SKILL.md**：程序式指南（何时使用、步骤、示例、边界情况）
- **Templates/**：代码模板（如 REST API → MCP 服务器的脚手架）
- **Scripts/**：辅助脚本

### 2. 渐进式披露（3 级加载）

| 级别 | 触发条件 | 加载内容 |
|------|----------|----------|
| **L1 - 索引** | 会话初始化 | Skill 名称 + 一句话描述（进入 System Prompt） |
| **L2 - 摘要** | LLM 表达兴趣 | 关键概念 + 触发条件（约 200 字） |
| **L3 - 完整** | 确认使用时 | 完整 SKILL.md + 模板 + 脚本（可能数千字） |

这种设计控制 Token 消耗：只有被使用的 Skill 才会完整加载 。

### 3. 条件激活与触发

- **关键词触发**：用户消息匹配 Skill 描述中的触发词
- **LLM 自选择**：LLM 根据意图判断需要加载哪些 Skill
- **Cron 预加载**：定时任务可指定 `skills` 列表，在运行前注入 

### 4. 自改进机制（`skill_manage` 工具）

Agent 在任务完成后可调用 `skill_manage`：
- **创建新 Skill**：将解决某类问题的经验固化为可复用文档
- **改进现有 Skill**：根据执行反馈更新 SKILL.md 中的步骤或示例
- **Skills Hub 共享**：符合 agentskills.io 标准，可社区共享 

**内置 Skill 示例**（118 个精选安全扫描）：
- `github-pr-workflow`：PR 生命周期管理
- `kanban-orchestrator`：任务分解 + 专员分配
- `touchdesigner-mcp`：实时视觉控制
- `audiocraft-audio-generation`：文本生成音乐

---

## 五、复杂问题的 Planning 与推理机制

对于多步骤复杂任务，Hermes 采用**显式规划循环**而非单步反应：

### 1. 规划触发条件
- 用户问题涉及多个子任务（如"分析竞品并生成报告"）
- 工具调用链预计超过 3 步
- 涉及跨领域协作（需要多个 Skill）

### 2. 规划执行流程

```
1. 意图解析：LLM 分析用户输入，提取目标与约束
2. 任务分解：将复杂目标拆分为原子步骤（子任务）
3. 资源分配：为每个子任务匹配 Skill、Tool、MCP Server
4. 子代理委派（Subagent Delegation）：生成隔离的 sub-agent 并行执行 
5. 执行监控：跟踪每步结果，处理失败重试（指数退避）
6. 结果聚合：合并子任务输出，生成最终响应
7. 经验固化：将成功路径写入 Skill 或 Memory
```

### 3. 推理可视化

- **TUI 界面**：显示 `ToolTrail` 树状可视化，实时展示思考链与工具调用轨迹 
- **推理标签**：支持 `<REASONING_SCRATCHPAD>` 标签或原生 thinking tokens，用于批处理时提取训练数据 

### 4. 记忆增强的 Planning

规划过程可利用三层记忆 ：
- **MEMORY.md**：类似问题的历史解决方案
- **SessionDB (FTS5)**：全文本搜索过往会话，LLM 摘要相关片段
- **Honcho 用户建模**：深层用户偏好推断（如"该用户喜欢表格而非段落"）

---

## 六、Cron 调度与 Workflow 自动化

Hermes 内置**自然语言调度系统**（非外部依赖）：

| 特性 | 说明 |
|------|------|
| **存储位置** | `~/.hermes/cron/jobs.json`（非 SQLite） |
| **触发方式** | 自然语言（"每 30 分钟"）、5 字段 Cron、ISO 时间戳、一次性 |
| **执行环境** | 每次 tick 创建**全新的无历史 AIAgent**，预加载指定 Skills |
| **交付目标** | `local`（仅记录）、`origin`（回传创建平台）、`platform:chat_id`（指定群聊） |
| **输出持久化** | `~/.hermes/cron/output/{job_id}/{timestamp}.md` |

**Workflow 示例**（n8n 集成模式）：
```
Webhook 触发 → AI Agent 节点（LLM + 工具 + 记忆）→ 输出处理 → 多平台交付
```

---

## 七、安全与成本控制机制

| 维度 | 机制 | 效果 |
|------|------|------|
| **Prompt 缓存** | 组装后的 System Prompt 缓存友好，重复利用降低 40-60% Token 消耗  |
| **上下文压缩** | 长对话自动摘要，超过窗口时压缩历史消息  |
| **工具输出截断** | 防止长返回值撑爆上下文 |
| **最大轮次** | `max_rounds` 防止死循环 |
| **只读范围** | 数据库工具使用只读用户，MCP 文件系统限制特定路径  |
| **CVE 记录** | 2026 年零 Agent 相关 CVE（对比 OpenClaw 的 9 个 CVE） |

---

## 总结架构图

```
┌─────────────────────────────────────────────────────────────┐
│                     用户入口层 (Gateway)                        │
│  CLI / TUI / Web / Telegram / Discord / Slack / Cron / ...  │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              AIAgent 核心编排引擎 (同步循环)                    │
│  ┌─────────────┐  ┌────────────┐  ┌─────────────┐          │
│  │ System Prompt │  │   LLM 推理  │  │ 工具调度    │          │
│  │   动态组装    │→ │ Function    │→ │ 执行环境    │          │
│  │ SOUL+Memory   │  │ Calling     │  │ Local/Docker│          │
│  │ +Skills+Tools │  │ 意图匹配    │  │ /SSH/Modal  │          │
│  └─────────────┘  └────────────┘  └─────────────┘          │
│         ↑                              │                    │
│         └──────── 工具结果反馈 ←─────────┘                    │
│                           ↓                                  │
│              ┌─────────────────────┐                        │
│              │  Planning 规划引擎   │                        │
│              │ (任务分解+子代理委派) │                        │
│              └─────────────────────┘                        │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              持久化层 (SQLite + Markdown)                     │
│  SessionDB (FTS5) / MEMORY.md / USER.md / Cron Output       │
└─────────────────────────────────────────────────────────────┘
```

这套架构的核心设计哲学是 **"Agent-first"**：所有入口都是 Agent 的薄适配层，而非 Gateway 包装 Agent。这使得 Hermes 能够实现真正的跨会话学习、技能自改进和深度个性化 。


