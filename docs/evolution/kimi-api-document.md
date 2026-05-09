
Kimi K2.6 的 API 文档原文在 **Moonshot AI 官方开放平台**


## 关于思考模式（Thinking）的官方说明

根据官方文档 ，Kimi K2.6 支持通过 `thinking` 参数控制思考模式：

### 默认状态（快速模式）
不设置 `thinking` 参数时，Kimi K2.6 默认使用**非思考模式**（快速响应）。

### 启用思考模式
```json
{
  "model": "kimi-k2.6",
  "messages": [{"role": "user", "content": "hello"}],
  "thinking": {"type": "enabled"}
}
```

### 显式禁用思考模式
```json
{
  "model": "kimi-k2.6",
  "messages": [{"role": "user", "content": "hello"}],
  "thinking": {"type": "disabled"}
}
```

## 使用 Python SDK 的示例

```python
import os
import openai

client = openai.Client(
    base_url="https://api.moonshot.ai/v1",
    api_key=os.getenv("MOONSHOT_API_KEY"),
)

# 思考模式
response = client.chat.completions.create(
    model="kimi-k2.6",
    messages=[{"role": "user", "content": "请详细分析这个问题"}],
    extra_body={"thinking": {"type": "enabled"}},
    max_tokens=1024*32
)

print(response.choices[0].message.content)
```

## 重要约束（官方文档原文）

当 `thinking` 设置为 `{"type": "enabled"}` 时，有以下限制 ：

1. `tool_choice` 只能设置为 `"auto"` 或 `"none"`（默认 `"auto"`），其他值会报错
2. 多步工具调用时，必须保留当前轮次中 assistant message 的 `reasoning_content` 在上下文中
3. 内置的 `$web_search` 工具暂时不兼容 K2.6/K2.5 的思考模式，需要先禁用思考模式再使用该工具



下面是kimi api优化要求：

1、在beebotos项目，config/beebotos.toml文件默认配置kimi-k2.6 ，默认为快速模式（"thinking": {"type": "disabled"}
}）
，也可以修改设置为思考模式；

2、在kimi client 的代码中，必须加上快速模式的显示定义：
 如参考："thinking": {"type": "disabled"}

3、下面3个重要约束，在beebotos代码中要遵守：
1. `tool_choice` 只能设置为 `"auto"` 或 `"none"`（默认 `"auto"`），其他值会报错
2. 多步工具调用时，必须保留当前轮次中 assistant message 的 `reasoning_content` 在上下文中
3. 内置的 `$web_search` 工具暂时不兼容 K2.6/K2.5 的思考模式，需要先禁用思考模式再使用该工具

%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%
所有 Kimi API 优化要求已实施完毕，并通过了全工作空间编译验证。以下是修改摘要：

---

### 1. 配置文件默认 `kimi-k2.6` + 快速模式

**`config/beebotos.toml`** — 在 `[models.kimi]` 下新增：
```toml
thinking = "disabled"
```

**`apps/gateway/src/config.rs`** — `ModelProviderConfig` 新增 `thinking: Option<String>` 字段，支持从 TOML 读取。

---

### 2. Kimi Client 显式定义快速模式

**`crates/agents/src/llm/providers/kimi.rs`** 主要改动：

- 新增 `ThinkingMode` 枚举（`Enabled` / `Disabled`，默认 `Disabled`）
- `KimiConfig` 新增 `thinking` 字段
- `complete` 与 `complete_stream` 中均**显式**将 `"thinking": {"type": "disabled"}`（或 `"enabled"`）写入 `request.config.extra`，确保每次请求都带上该参数
- 支持 `KIMI_THINKING` 环境变量覆盖

---

### 3. 三个官方约束在代码中的落实

#### 约束 1：`tool_choice` 只能为 `"auto"` 或 `"none"`
在 `kimi.rs` 的 `complete` / `complete_stream` 中，当模型为 `k2.6` 时：
- 若 `tool_choice` 不是 `Auto` 或 `None`，自动重置为 `ToolChoice::Auto("auto")`
- 避免了传入 `required` 或特定工具导致的 API 报错

#### 约束 2：多步工具调用保留 `reasoning_content`
- **`http_client.rs`** — `OpenAIRequestBuilder::build_body` 在序列化消息时，新增了 `reasoning_content` 字段的输出，确保该字段会随消息上下文发送到 Kimi API
- **`client.rs`** — `chat_with_tools_react` 中的 `messages.push(choice.message.clone())` 已通过注释明确标记：克隆的 assistant message 完整保留了 `reasoning_content`，满足多步工具调用上下文要求

#### 约束 3：`$web_search` 与思考模式不兼容
在 `kimi.rs` 中新增 `has_web_search_tool` 检测：
- 当请求工具列表中包含 `$web_search` 或 `web_search`，且模型为 `k2.6` 时，**强制**将 `thinking` 设为 `Disabled`
- 防止因同时使用思考模式与内置搜索工具导致 API 报错

---

### 4. 配置传递链路

**`apps/gateway/src/services/llm_service.rs`** — 创建 `KimiProvider` 时，从 `provider_config.thinking` 解析并传入 `KimiConfig`，保证配置文件中的设置生效。

**`crates/agents/src/llm/providers/mod.rs`** — 新增 `ThinkingMode` 的 re-export，方便上层调用。

%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%
### 在kimi-k2.6 中，这个 temperature = 1.0 参数不用了，检查一下，beebotos-agents模块是否有采用？若采用要取消掉。

编译通过。已删除 `beebotos-agents` 模块中为 kimi-k2.6 强制设置 `temperature = 1.0` 的两处代码：

- `crates/agents/src/llm/providers/kimi.rs` 的 `complete` 方法
- `crates/agents/src/llm/providers/kimi.rs` 的 `complete_stream` 方法

现在 Kimi k2.6 请求不再强制覆盖 temperature，由上层配置或默认值决定。


&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&
编译通过。修复了两处 temperature 问题：

1. **`config/beebotos.toml`** — `[models.kimi]` 的 `temperature` 从 `1.0` 改为 `0.6`
2. **`crates/agents/src/llm/providers/kimi.rs`** — 在 `complete` / `complete_stream` 中增加兜底强制逻辑：

```rust
if request.config.model.contains("k2.6") {
    request.config.temperature = Some(0.6);
}
```

现在无论 gateway 的 `LlmService` 或 `AgentRuntime` 传了什么 temperature 值（如默认的 `0.7`），Kimi k2.6 请求前都会被强制覆盖为 `0.6`，不会再出现 `"invalid temperature: only 0.6 is allowed for this model"` 错误。



