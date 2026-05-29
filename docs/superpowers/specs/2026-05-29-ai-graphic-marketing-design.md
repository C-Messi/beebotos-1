# AI 图文营销 MVP 设计

## 目标

在 AI 店长中新增 AI 图文营销能力，让用户输入商品信息后生成可人工发布的小红书/朋友圈图文营销包，并通过 OpenAI 兼容图片中转站生成海报图。

第一版聚焦真实可用的“文案 + 图片生成”，不做自动发布、评论采集、线索分析和复杂素材库。

## 范围

包含：

- AI 店长首页图文营销入口可点击。
- 新增 `/ai-store-manager/graphic-marketing` 页面。
- 用户录入商品、卖点、人群、价格、平台、营销目标和内容风格。
- 生成图文营销包。
- 生成图片 prompt。
- Gateway 调图片中转站生成图片。
- 前端展示生成状态、图文结果和图片预览。

不包含：

- 自动发布到平台。
- 批量任务。
- 评论采集和复盘闭环。
- 完整内容资产库。
- 前端保存或直连图片 API key。

## 用户流程

1. 用户进入 AI 店长。
2. 点击 AI 图文营销。
3. 填写商品信息和营销目标。
4. 点击生成图文包。
5. 系统返回标题、正文、朋友圈文案、海报文案、评论引导和图片 prompt。
6. 用户点击生成图片。
7. Gateway 调用图片中转站。
8. 页面展示图片预览和生成状态。
9. 用户人工审核、复制文案或下载图片发布。

## 前端设计

新增页面 `apps/web/src/pages/ai_graphic_marketing.rs`。

页面分三块：

- 任务配置：商品名、核心卖点、目标人群、价格区间、平台、营销目标、内容风格。
- 图文营销包：标题、正文、朋友圈文案、海报文案、评论引导、发布前检查。
- 图片素材：图片 prompt、生成按钮、生成状态、图片预览、错误提示。

AI 店长首页中图文营销卡片的 `href` 改为 `/ai-store-manager/graphic-marketing`。

路由新增在 `/ai-store-manager` 固定路由之前：

```text
/ai-store-manager/graphic-marketing
```

## 后端设计

扩展 `apps/gateway/src/handlers/http/ai_store_manager.rs`，新增图文营销接口。

```text
POST /api/v1/ai-store-manager/graphic-packages
POST /api/v1/ai-store-manager/graphic-images
GET  /api/v1/ai-store-manager/graphic-images/:id
```

`graphic-packages` 第一版可以先用确定性规则生成结构化文案包，后续再接入 LLM。

`graphic-images` 调 OpenAI 兼容图片中转站，生成单张图片并返回图片数据或可预览 URL。

`graphic-images/:id` 第一版可返回同步生成结果的状态结构，后续再替换为异步任务查询。

## 数据结构

图文任务请求：

```json
{
  "product": "云柑礼盒",
  "selling_points": "当季鲜果、顺丰冷链、送礼体面",
  "audience": "25-40 岁办公室人群",
  "price_range": "99-199 元",
  "platform": "小红书",
  "goal": "新品种草",
  "style": "真实测评"
}
```

图文营销包响应：

```json
{
  "title_options": ["标题 1", "标题 2", "标题 3"],
  "body": "正文",
  "moments_copy": "朋友圈文案",
  "poster_copy": "海报文案",
  "comment_guide": "评论区引导话术",
  "image_prompt": "图片生成 prompt",
  "checks": [
    { "label": "商品卖点完整", "status": "已覆盖" }
  ]
}
```

图片生成请求：

```json
{
  "product": "云柑礼盒",
  "platform": "小红书",
  "prompt": "图片生成 prompt",
  "size": "1024x1536",
  "quality": "medium"
}
```

图片生成响应：

```json
{
  "id": "graphic-image-xhs-001",
  "provider": "openai-compatible-image",
  "status": "completed",
  "message": "图片已生成。",
  "image_url": null,
  "b64_json": "BASE64_IMAGE_DATA"
}
```

## 配置

新增独立图片生成配置，避免和聊天模型混用。

```toml
[image_generation]
base_url = "https://image-relay.example/v1"
api_key = ""
model = "gpt-image-1"
```

优先级：

1. 环境变量。
2. `config/local.toml`。
3. `config/beebotos.toml` 默认值。

建议环境变量：

```text
IMAGE_GENERATION_BASE_URL
IMAGE_GENERATION_API_KEY
IMAGE_GENERATION_MODEL
```

## 图片调用

Gateway 使用 Rust HTTP client 调中转站：

```text
POST {base_url}/images/generations
Authorization: Bearer {api_key}
Content-Type: application/json
```

请求体：

```json
{
  "model": "gpt-image-1",
  "prompt": "为云柑礼盒生成小红书封面图，突出当季鲜果、冷链配送和送礼场景。",
  "size": "1024x1536",
  "quality": "medium",
  "n": 1
}
```

第一版优先兼容 `b64_json` 返回。若中转站返回 URL，前端同样可展示。

## 错误处理

- 配置缺失：返回“图片生成未配置”。
- 中转站 401/403：返回“图片生成鉴权失败”。
- 模型不可用：返回“图片模型不可用”。
- 超时：返回“图片生成超时，请重试”。
- 响应无图片：返回“图片生成结果为空”。

错误只展示给当前用户，不写入前端配置或日志中的明文 key。

## 测试

后端：

- 图文包生成覆盖不同版本。
- 图片请求 payload 包含 model、prompt、size、quality。
- 缺少配置时返回明确错误。
- 中转站错误能映射为用户可读错误。

前端：

- AI 店长图文入口跳转正确。
- 默认图文任务能生成结果。
- 图片生成 loading、成功、失败状态可展示。

验证命令：

```bash
cargo test -p beebotos-gateway ai_store_manager
cargo check -p beebotos-web --target wasm32-unknown-unknown
```

## 后续扩展

- 接入 LLM 生成更自然的文案。
- 图片保存到 `data/media/commerce` 并返回稳定 URL。
- 写入 `commerce_content_assets`。
- 接入 AI 电商的人工发布记录。
- 接入评论采集和线索复盘。
