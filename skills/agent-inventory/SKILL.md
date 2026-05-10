---
name: Agent Inventory
description: 查询系统中所有 Agent 的状态、统计信息和运行概况。当用户想了解系统中有哪些 Agent、它们当前的状态、任务执行情况时调用。
---

# Agent Inventory

## When to use
- 用户询问"有哪些 Agent"
- 用户问"系统中有多少 Agent"
- 用户想了解"Agent 的状态"
- 用户问"某个 Agent 在做什么"
- 用户说"show me all agents"
- 用户想了解系统的 Agent 部署情况

## When not to use
- 用户询问某个具体 Agent 的详细配置（应引导到配置管理）
- 用户想创建/删除 Agent（应使用对应的管理技能）
- 用户询问 Agent 的历史对话记录（应使用记忆查询功能）

## Capabilities
- 返回所有已注册 Agent 的 ID、当前状态
- 显示每个 Agent 的任务统计（总任务、成功、失败）
- 显示 Agent 的注册时间和状态变更时间
- 以 Markdown 表格格式呈现

## Usage
直接调用，无需参数。系统将自动查询并返回 Agent 状态清单。
