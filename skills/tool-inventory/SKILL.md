---
name: Tool Inventory
description: 查询并列出本机（小蜜蜂 BeeAgentOS）所有可用的底层工具（Tools）清单，包括文件操作、命令执行、网络请求、搜索、文本处理等工具。当用户想了解系统能力、工具集、功能列表时调用。
---

# Tool Inventory

## When to use
- 用户询问"你有哪些工具"
- 用户问"你有什么功能/能力"
- 用户要求"列出所有可用工具"
- 用户问"你能做什么/支持什么操作"
- 用户说"show me your tools"
- 用户想了解系统内置的工具集

## When not to use
- 用户想直接调用某个工具（应直接描述任务，系统会自动选择工具）
- 用户询问 Skill/技能（应使用 skill_inventory）
- 用户询问定时任务（应使用 schedule_inventory）

## Capabilities
- 返回本机所有工具的序号、名称、描述说明和参数概要
- 以 Markdown 表格格式呈现
- 包含 10+ 个工具：文件读写、命令执行、网络请求、文本搜索等

## Usage
直接调用，无需参数。系统将自动查询并返回工具清单。
