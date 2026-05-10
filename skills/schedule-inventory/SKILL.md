---
name: Schedule Inventory
description: 查询并列出本机（小蜜蜂 BeeAgentOS）所有定时任务清单，包括 Workflow 中的 Cron 触发器和前端控制栏创建的定时任务。当用户想了解系统有哪些定时任务、定时器、计划任务时调用。
---

# Schedule Inventory

## When to use
- 用户询问"你有哪些定时任务"
- 用户问"有哪些定时器/计划任务"
- 用户要求"列出所有定时任务"
- 用户说"show me scheduled tasks"
- 用户想了解系统的 cron job 列表
- 用户问"有什么周期性自动执行的任务"

## When not to use
- 用户想创建/修改定时任务（应使用控制栏或 Workflow 编辑器）
- 用户询问 Agent 状态（应使用 agent_inventory）
- 用户询问 Workflow 定义（应使用 workflow_inventory）

## Capabilities
- 返回 Workflow 层面的 Cron 定时触发器
- 返回前端控制栏创建的定时任务
- 显示任务名称、定时规则、时区、运行状态、运行次数
- 以 Markdown 列表格式呈现

## Usage
直接调用，无需参数。系统将自动查询并返回定时任务清单。
