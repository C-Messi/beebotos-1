---
name: Workflow Inventory
description: 查询本机所有已注册的 Workflow 定义清单。当用户想了解系统有哪些工作流、自动化流程、编排任务时调用。
---

# Workflow Inventory

## When to use
- 用户询问"有哪些工作流"
- 用户问"系统有什么自动化流程"
- 用户想了解"Workflow 列表"
- 用户说"show me workflows"
- 用户想了解系统的编排任务定义

## When not to use
- 用户想执行某个 Workflow（应使用 workflow 执行技能）
- 用户询问 Workflow 的详细步骤定义（应引导到具体 workflow 查看）
- 用户想创建/修改 Workflow（应使用 Workflow 编辑器）

## Capabilities
- 返回所有已注册 Workflow 的 ID、名称、版本
- 显示每个 Workflow 的步骤数量和触发器数量
- 显示 Workflow 的标签分类
- 以 Markdown 表格格式呈现

## Usage
直接调用，无需参数。系统将自动查询并返回 Workflow 清单。
