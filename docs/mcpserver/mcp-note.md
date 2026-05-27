
在beebotos-web模块里面，增加一个MCP Server控制子项目；

1、在beebotos-web的主页面的左边“控制"栏下面的”技能市场“的下面，增加名称“MCP Server”的子项目；
2、MCP Server的主页，正确工作状态下，参考 /root/beebotos/docs/mcpserver/main-OK.PNG 这个内容；

3、MCP Server的主页，不正确工作状态下，参考 /root/beebotos/docs/mcpserver/main-error.PNG 这个内容；

4、在MCP Server的主页的右上角的导入配置按钮，实现内容参考：/root/beebotos/docs/mcpserver/input.PNG

5、在MCP Server的主页的左下边，工具（tool）按钮，参考/root/beebotos/docs/mcpserver/tool.PNG

6、在MCP Server的主页的左下边，编辑（edit）按钮，参考
/root/beebotos/docs/mcpserver/edit.PNG.

&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&77

已全部完成！在 `beebotos-web` 模块中成功添加了 **MCP Server** 控制子项目，编译通过（`cargo check --lib --target wasm32-unknown-unknown` 无错误）。

## 修改内容汇总

### 新增文件
1. **`apps/web/src/api/mcp_server.rs`** — MCP Server API 服务模块
   - 数据模型：`McpServerConfig`、`McpServer`、`McpTool`、`McpTransport` 等
   - 本地存储管理（基于 `localStorage`）：增删改查、导入 JSON 配置
   - 模拟连接/断开/获取工具等异步操作

2. **`apps/web/src/pages/mcp_server.rs`** — MCP Server 页面组件
   - 主页面：标题、搜索框、刷新/导入配置按钮、服务器卡片列表
   - 支持 **已连接**（绿色状态条）和 **错误**（红色状态条 + 错误信息）两种状态
   - 卡片展示：名称、Transport、Enabled、命令参数、操作按钮
   - **导入配置模态框**：JSON 编辑器，支持 `mcpServers` 格式
   - **工具模态框**：未连接时显示错误提示，已连接时显示工具列表
   - **编辑模态框**：JSON 编辑器，可修改服务器配置

### 修改文件
| 文件 | 修改内容 |
|------|---------|
| `src/api/mod.rs` | 导出 MCP Server API 模块 |
| `src/pages/mod.rs` | 导出 `McpServerPage` |
| `src/lib.rs` | 添加 `/mcp-server` 路由、页面标题 |
| `src/components/sidebar.rs` | 在"控制"栏"技能市场"下方添加 **MCP Server** 导航项（🔧） |
| `src/i18n.rs` | 添加 `nav-mcp-server` 中英文翻译 |
| `src/state/app.rs` | 添加 `mcp_server_service()` 方法 |
| `style/main.css` | 添加 MCP Server 页面完整样式（卡片、状态条、模态框、编辑器、响应式） |

### 演示数据
首次访问页面时，会自动创建一个 `metatrader` 示例配置（与参考图片一致），默认状态为 **已连接**，方便直接体验 OK 状态的界面效果。用户可通过"断开"/"连接"按钮切换状态，或删除后通过"导入配置"添加自己的 MCP 配置。



