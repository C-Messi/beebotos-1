---
name: MCP Inventory
description: 查询本机已连接的 MCP（Model Context Protocol）服务及其可用工具。当用户想了解系统连接了哪些外部 MCP 服务、它们提供了什么工具时调用。
---

# MCP Inventory

## When to use
- 用户询问"有哪些 MCP 服务"
- 用户问"连接了哪些外部工具"
- 用户想了解"MCP 服务状态"
- 用户说"show me MCP servers"
- 用户想了解系统通过 MCP 扩展了哪些能力

## When not to use
- 用户想调用某个 MCP 工具（应通过正常的 skill/tool 调用流程）
- 用户想添加/移除 MCP 连接（应使用 MCP 管理接口）
- 用户询问非 MCP 的本地工具（应使用 tool_inventory）

## Capabilities
- 返回所有已连接的 MCP Clients 名称和初始化状态
- 显示每个 MCP Client 提供的工具数量
- 返回所有已注册的 MCP Servers
- 以 Markdown 列表格式呈现

## Usage
直接调用，无需参数。系统将自动查询并返回 MCP 服务清单。
