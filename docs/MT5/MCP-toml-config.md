# MT5 MCP TOML 配置说明

BeeBotOS 读取 `config/beebotos.toml`、`config/local.toml` 和 `BEE__*` 环境变量，不使用 Claude Desktop / Cursor 常见的 `mcpServers` JSON 配置格式。

推荐把环境相关的 MT5 参数放在 `config/local.toml`，避免提交真实账号和密码。也可以在 `config/beebotos.toml` 中取消注释示例后填写，但不要把真实凭据提交到仓库。

```toml
[[mcp.servers]]
name = "metatrader"
transport = "stdio"
command = "metatrader-mcp-server"
args = [
    "--login", "YOUR_MT5_LOGIN",
    "--password", "YOUR_MT5_PASSWORD",
    "--server", "YOUR_MT5_SERVER",
    "--transport", "stdio",
    "--path", "C:\\Program Files\\MetaTrader 5\\terminal64.exe",
]
```

字段对应关系：

| JSON 字段 | BeeBotOS TOML 字段 |
| --- | --- |
| `mcpServers.metatrader` | `[[mcp.servers]]` + `name = "metatrader"` |
| `command` | `command` |
| `args` | `args` |
| `--transport stdio` | `transport = "stdio"` 与 `args` 中的 MCP server 参数 |

注意事项：

- 先安装 MCP server：`pip install metatrader-mcp-server`。
- Windows 路径中的反斜杠需要写成 `\\`，例如 `C:\\Program Files\\MetaTrader 5\\terminal64.exe`。
- `config/local.toml` 会覆盖 `config/beebotos.toml` 的同名配置，适合放本机或部署环境参数。
- 如果启用了 `allowed_commands` 白名单，需要包含 `metatrader-mcp-server`，例如：

```toml
[mcp]
allowed_commands = ["metatrader-mcp-server"]
```
