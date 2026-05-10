---
name: Skill Inventory
description: 查询并列出本机（小蜜蜂 BeeAgentOS）所有已注册的技能（Skills）清单，包括内置知识技能、MCP桥接技能等。当用户想了解系统有哪些技能、能做什么任务时调用。
---

# Skill Inventory

## When to use
- 用户询问"你有哪些技能"
- 用户问"你会做什么"
- 用户要求"列出所有技能"
- 用户说"show me your skills"
- 用户想了解系统支持哪些任务类型
- 用户问"你有什么能力"

## When not to use
- 用户想执行某个具体技能（应直接描述需求，系统会自动匹配技能）
- 用户询问底层工具（应使用 tool_inventory）
- 用户询问定时任务（应使用 schedule_inventory）

## Capabilities
- 返回本机所有已注册技能的序号、名称、分类、描述说明和使用次数
- 以 Markdown 表格格式呈现
- 包含内置技能和 MCP 桥接技能
- 区分技能类型：内置 / 知识 / MCP

## Usage
直接调用，无需参数。系统将自动查询并返回技能清单。
