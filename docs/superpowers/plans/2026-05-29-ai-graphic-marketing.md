# AI Graphic Marketing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build AI 店长图文营销 MVP: generate a structured graphic marketing package and generate a poster image through an OpenAI-compatible image relay.

**Architecture:** Keep the browser behind BeeBotOS Gateway. The web app calls `/api/v1/ai-store-manager/*`; Gateway owns image relay configuration, payload construction, auth, error mapping, and response normalization. The first content package generator is deterministic and testable, with the image path wired to a real OpenAI-compatible HTTP endpoint.

**Tech Stack:** Rust, Axum, Reqwest, Serde, Leptos, existing BeeBotOS API client, CSS in `apps/web/style/main.css`.

---

## File Structure

- Modify `apps/gateway/src/config.rs`: add `ImageGenerationConfig`, defaults, env overlay, and config tests.
- Modify `config/beebotos.toml`: add a safe empty `[image_generation]` section.
- Modify `apps/gateway/src/handlers/http/ai_store_manager.rs`: add graphic package structs, deterministic package generation, image payload helpers, image relay client, handlers, and tests.
- Modify `apps/gateway/src/main.rs`: register three AI graphic marketing API routes near existing AI Store Manager routes.
- Modify `apps/web/src/api/ai_store_manager.rs`: add graphic request/response types and client methods.
- Modify `apps/web/src/api/mod.rs`: re-export new graphic API types.
- Create `apps/web/src/pages/ai_graphic_marketing.rs`: new Leptos page.
- Modify `apps/web/src/pages/mod.rs`: export the new page and include it in page export tests.
- Modify `apps/web/src/pages/ai_store_manager.rs`: make the AI 图文营销 card link to `/ai-store-manager/graphic-marketing`.
- Modify `apps/web/src/lib.rs`: import and route the new page before the generic `/ai-store-manager` route.
- Modify `apps/web/style/main.css`: add responsive AI graphic marketing layout styles.

## Task 1: Gateway Image Generation Config

**Files:**
- Modify: `apps/gateway/src/config.rs`
- Modify: `config/beebotos.toml`

- [ ] **Step 1: Write the failing config tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `apps/gateway/src/config.rs`:

```rust
#[test]
fn default_image_generation_config_is_empty_and_safe() {
    let config = ImageGenerationConfig::default();

    assert_eq!(config.base_url, "https://api.openai.com/v1");
    assert!(config.api_key.is_none());
    assert_eq!(config.model, "gpt-image-1");
    assert_eq!(config.timeout_seconds, 180);
}

#[test]
fn image_generation_config_reads_env_override() {
    std::env::set_var("IMAGE_GENERATION_BASE_URL", "https://relay.example/v1");
    std::env::set_var("IMAGE_GENERATION_API_KEY", "img-test-key");
    std::env::set_var("IMAGE_GENERATION_MODEL", "gpt-image-2");

    let mut config = BeeBotOSConfig::default();
    config.apply_image_generation_env();

    assert_eq!(config.image_generation.base_url, "https://relay.example/v1");
    assert_eq!(
        config.image_generation.api_key.as_deref(),
        Some("img-test-key")
    );
    assert_eq!(config.image_generation.model, "gpt-image-2");

    std::env::remove_var("IMAGE_GENERATION_BASE_URL");
    std::env::remove_var("IMAGE_GENERATION_API_KEY");
    std::env::remove_var("IMAGE_GENERATION_MODEL");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p beebotos-gateway image_generation_config
```

Expected: fail because `ImageGenerationConfig`, `BeeBotOSConfig::image_generation`, and `apply_image_generation_env` do not exist.

- [ ] **Step 3: Add config fields and defaults**

In `apps/gateway/src/config.rs`, add this field to `BeeBotOSConfig` near `models`:

```rust
#[serde(default)]
pub image_generation: ImageGenerationConfig,
```

Add this struct near `ModelProviderConfig`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageGenerationConfig {
    #[serde(default = "default_image_generation_base_url")]
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_image_generation_model")]
    pub model: String,
    #[serde(default = "default_image_generation_timeout")]
    pub timeout_seconds: u64,
}

impl Default for ImageGenerationConfig {
    fn default() -> Self {
        Self {
            base_url: default_image_generation_base_url(),
            api_key: None,
            model: default_image_generation_model(),
            timeout_seconds: default_image_generation_timeout(),
        }
    }
}

fn default_image_generation_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_image_generation_model() -> String {
    "gpt-image-1".to_string()
}

fn default_image_generation_timeout() -> u64 {
    180
}
```

Update `impl Default for BeeBotOSConfig`:

```rust
image_generation: ImageGenerationConfig::default(),
```

Add this method inside `impl BeeBotOSConfig`:

```rust
fn apply_image_generation_env(&mut self) {
    if let Ok(base_url) = std::env::var("IMAGE_GENERATION_BASE_URL") {
        if !base_url.trim().is_empty() {
            self.image_generation.base_url = base_url;
        }
    }

    if let Ok(api_key) = std::env::var("IMAGE_GENERATION_API_KEY") {
        if !api_key.trim().is_empty() {
            self.image_generation.api_key = Some(api_key);
        }
    }

    if let Ok(model) = std::env::var("IMAGE_GENERATION_MODEL") {
        if !model.trim().is_empty() {
            self.image_generation.model = model;
        }
    }
}
```

Call it in `BeeBotOSConfig::load()` immediately after `let mut cfg: Self = config.try_deserialize()?;`:

```rust
cfg.apply_image_generation_env();
```

Add `IMAGE_GENERATION_` to the `prefixes` array in `migrate_env_vars()` only if BEE-prefixed nested env support is needed. Keep direct `IMAGE_GENERATION_*` support through `apply_image_generation_env`.

- [ ] **Step 4: Add default TOML config**

Append to `config/beebotos.toml`:

```toml
[image_generation]
base_url = "https://api.openai.com/v1"
api_key = ""
model = "gpt-image-1"
timeout_seconds = 180
```

- [ ] **Step 5: Run tests to verify config passes**

Run:

```bash
cargo test -p beebotos-gateway image_generation_config
```

Expected: both tests pass.

- [ ] **Step 6: Commit config work**

```bash
git add apps/gateway/src/config.rs config/beebotos.toml
git commit -m "feat: add image generation config"
```

## Task 2: Gateway Graphic Package API

**Files:**
- Modify: `apps/gateway/src/handlers/http/ai_store_manager.rs`
- Modify: `apps/gateway/src/main.rs`

- [ ] **Step 1: Write failing package tests**

Add these tests to `apps/gateway/src/handlers/http/ai_store_manager.rs`:

```rust
#[test]
fn graphic_package_has_required_marketing_assets() {
    let req = CreateGraphicPackageRequest {
        product: "云柑礼盒".to_string(),
        selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
        audience: "25-40 岁办公室人群".to_string(),
        price_range: "99-199 元".to_string(),
        platform: "小红书".to_string(),
        goal: "新品种草".to_string(),
        style: "真实测评".to_string(),
    };

    let package = create_graphic_package(&req);

    assert_eq!(package.title_options.len(), 3);
    assert!(package.body.contains("云柑礼盒"));
    assert!(package.moments_copy.contains("顺丰冷链"));
    assert!(package.poster_copy.contains("99-199 元"));
    assert!(package.comment_guide.contains("评论"));
    assert!(package.image_prompt.contains("小红书"));
    assert!(package.checks.iter().any(|check| check.label == "商品卖点完整"));
}

#[test]
fn graphic_package_changes_by_platform() {
    let mut req = CreateGraphicPackageRequest {
        product: "云柑礼盒".to_string(),
        selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
        audience: "25-40 岁办公室人群".to_string(),
        price_range: "99-199 元".to_string(),
        platform: "小红书".to_string(),
        goal: "新品种草".to_string(),
        style: "真实测评".to_string(),
    };
    let xhs = create_graphic_package(&req);

    req.platform = "朋友圈".to_string();
    let moments = create_graphic_package(&req);

    assert_ne!(xhs.title_options[0], moments.title_options[0]);
    assert!(moments.image_prompt.contains("朋友圈"));
}
```

- [ ] **Step 2: Run package tests to verify they fail**

Run:

```bash
cargo test -p beebotos-gateway graphic_package
```

Expected: fail because `CreateGraphicPackageRequest` and `create_graphic_package` do not exist.

- [ ] **Step 3: Add package structs and deterministic generator**

Add this code to `apps/gateway/src/handlers/http/ai_store_manager.rs` after `VideoTaskResponse`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateGraphicPackageRequest {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub price_range: String,
    pub platform: String,
    pub goal: String,
    pub style: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GraphicMarketingCheck {
    pub label: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GraphicPackageResponse {
    pub title_options: Vec<String>,
    pub body: String,
    pub moments_copy: String,
    pub poster_copy: String,
    pub comment_guide: String,
    pub image_prompt: String,
    pub checks: Vec<GraphicMarketingCheck>,
}

pub fn create_graphic_package(req: &CreateGraphicPackageRequest) -> GraphicPackageResponse {
    let platform_prefix = if req.platform == "朋友圈" {
        "朋友圈私域"
    } else {
        "小红书种草"
    };

    GraphicPackageResponse {
        title_options: vec![
            format!("{}｜{}也会想收藏的{}", platform_prefix, req.audience, req.product),
            format!("{}真实体验：{}值不值得入手", req.product, req.selling_points),
            format!("{}，{}场景里的体面选择", req.product, req.goal),
        ],
        body: format!(
            "这次推荐{}，面向{}，主打{}。价格区间{}，适合{}内容风格。先讲真实使用场景，再补充购买理由，最后引导用户评论咨询。",
            req.product, req.audience, req.selling_points, req.price_range, req.style
        ),
        moments_copy: format!(
            "{}适合{}，{}。需要了解规格或到手时间，可以直接留言。",
            req.product, req.audience, req.selling_points
        ),
        poster_copy: format!(
            "{}\n{}\n{}\n{}",
            req.product, req.selling_points, req.price_range, req.goal
        ),
        comment_guide: format!(
            "评论区可以引导用户留下使用场景，例如：想了解{}适不适合你，评论告诉我预算和用途。",
            req.product
        ),
        image_prompt: format!(
            "为{}生成{}营销海报，平台是{}，目标人群是{}，突出{}，价格区间{}，风格{}。画面干净真实，商品主体清晰，适合中文电商内容发布。",
            req.product,
            req.goal,
            req.platform,
            req.audience,
            req.selling_points,
            req.price_range,
            req.style
        ),
        checks: vec![
            GraphicMarketingCheck {
                label: "商品卖点完整".to_string(),
                status: "已覆盖".to_string(),
            },
            GraphicMarketingCheck {
                label: "平台风格匹配".to_string(),
                status: "已覆盖".to_string(),
            },
            GraphicMarketingCheck {
                label: "人工审核".to_string(),
                status: "待确认".to_string(),
            },
        ],
    }
}
```

Add handler:

```rust
pub async fn create_graphic_package_handler(
    user: AuthUser,
    Json(req): Json<CreateGraphicPackageRequest>,
) -> Result<Json<GraphicPackageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(create_graphic_package(&req)))
}
```

- [ ] **Step 4: Register package route**

In `apps/gateway/src/main.rs`, near existing AI Store Manager routes, add before admin config routes:

```rust
.route(
    "/api/v1/ai-store-manager/graphic-packages",
    post(handlers::http::ai_store_manager::create_graphic_package_handler),
)
```

- [ ] **Step 5: Run package tests**

Run:

```bash
cargo test -p beebotos-gateway graphic_package
```

Expected: package tests pass.

- [ ] **Step 6: Commit package API**

```bash
git add apps/gateway/src/handlers/http/ai_store_manager.rs apps/gateway/src/main.rs
git commit -m "feat: add graphic marketing package api"
```

## Task 3: Gateway Image Relay API

**Files:**
- Modify: `apps/gateway/src/handlers/http/ai_store_manager.rs`
- Modify: `apps/gateway/src/main.rs`

- [ ] **Step 1: Write failing image payload tests**

Add these tests to `apps/gateway/src/handlers/http/ai_store_manager.rs`:

```rust
#[test]
fn image_payload_uses_configured_model_and_prompt() {
    let req = CreateGraphicImageRequest {
        product: "云柑礼盒".to_string(),
        platform: "小红书".to_string(),
        prompt: "生成海报".to_string(),
        size: "1024x1536".to_string(),
        quality: "medium".to_string(),
    };

    let payload = build_image_generation_payload("gpt-image-1", &req);

    assert_eq!(payload.model, "gpt-image-1");
    assert_eq!(payload.prompt, "生成海报");
    assert_eq!(payload.size, "1024x1536");
    assert_eq!(payload.quality, "medium");
    assert_eq!(payload.n, 1);
}

#[test]
fn image_result_normalizes_base64_response() {
    let response = OpenAIImageResponse {
        data: vec![OpenAIImageData {
            b64_json: Some("abc123".to_string()),
            url: None,
        }],
    };

    let result = image_response_to_graphic_result("云柑礼盒", "小红书", response).unwrap();

    assert_eq!(result.provider, "openai-compatible-image");
    assert_eq!(result.status, "completed");
    assert_eq!(result.image_url, None);
    assert_eq!(result.b64_json.as_deref(), Some("abc123"));
}

#[test]
fn image_result_rejects_empty_response() {
    let response = OpenAIImageResponse { data: vec![] };

    let result = image_response_to_graphic_result("云柑礼盒", "小红书", response);

    assert!(result.is_err());
}
```

- [ ] **Step 2: Run image tests to verify they fail**

Run:

```bash
cargo test -p beebotos-gateway image_
```

Expected: fail because image request/response structs and helpers do not exist.

- [ ] **Step 3: Add image structs and helpers**

Add this code to `apps/gateway/src/handlers/http/ai_store_manager.rs` after graphic package code:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateGraphicImageRequest {
    pub product: String,
    pub platform: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GraphicImageResponse {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub message: String,
    pub image_url: Option<String>,
    pub b64_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct OpenAIImageRequest {
    model: String,
    prompt: String,
    size: String,
    quality: String,
    n: u8,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct OpenAIImageResponse {
    data: Vec<OpenAIImageData>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct OpenAIImageData {
    b64_json: Option<String>,
    url: Option<String>,
}

fn build_image_generation_payload(
    model: &str,
    req: &CreateGraphicImageRequest,
) -> OpenAIImageRequest {
    OpenAIImageRequest {
        model: model.to_string(),
        prompt: req.prompt.clone(),
        size: req.size.clone(),
        quality: req.quality.clone(),
        n: 1,
    }
}

fn image_response_to_graphic_result(
    product: &str,
    platform: &str,
    response: OpenAIImageResponse,
) -> Result<GraphicImageResponse, GatewayError> {
    let first = response
        .data
        .into_iter()
        .next()
        .ok_or_else(|| GatewayError::internal("图片生成结果为空"))?;

    if first.b64_json.is_none() && first.url.is_none() {
        return Err(GatewayError::internal("图片生成结果为空"));
    }

    Ok(GraphicImageResponse {
        id: format!("graphic-image-{}-{}", platform, product).replace(' ', "-"),
        provider: "openai-compatible-image".to_string(),
        status: "completed".to_string(),
        message: "图片已生成。".to_string(),
        image_url: first.url,
        b64_json: first.b64_json,
    })
}

fn image_generation_url(base_url: &str) -> String {
    format!("{}/images/generations", base_url.trim_end_matches('/'))
}
```

- [ ] **Step 4: Add image relay handler**

Add required imports at the top:

```rust
use crate::config::BeeBotOSConfig;
```

Add handler:

```rust
pub async fn create_graphic_image(
    user: AuthUser,
    Json(req): Json<CreateGraphicImageRequest>,
) -> Result<Json<GraphicImageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let config = BeeBotOSConfig::load()
        .map_err(|err| GatewayError::internal(format!("加载图片生成配置失败: {}", err)))?;
    let api_key = config
        .image_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| GatewayError::bad_request("图片生成未配置"))?;

    let payload = build_image_generation_payload(&config.image_generation.model, &req);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.image_generation.timeout_seconds,
        ))
        .build()
        .map_err(|err| GatewayError::internal(format!("图片生成客户端创建失败: {}", err)))?;

    let response = client
        .post(image_generation_url(&config.image_generation.base_url))
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("图片生成请求失败: {}", err)))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(GatewayError::bad_request("图片生成鉴权失败"));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::internal(format!(
            "图片模型不可用: HTTP {} {}",
            status, body
        )));
    }

    let image_response = response
        .json::<OpenAIImageResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("图片生成响应解析失败: {}", err)))?;
    let result = image_response_to_graphic_result(&req.product, &req.platform, image_response)?;

    Ok(Json(result))
}

pub async fn get_graphic_image(
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<GraphicImageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(GraphicImageResponse {
        id,
        provider: "openai-compatible-image".to_string(),
        status: "completed".to_string(),
        message: "同步图片任务已完成。".to_string(),
        image_url: None,
        b64_json: None,
    }))
}
```

- [ ] **Step 5: Register image routes**

In `apps/gateway/src/main.rs`, add near the graphic package route:

```rust
.route(
    "/api/v1/ai-store-manager/graphic-images",
    post(handlers::http::ai_store_manager::create_graphic_image),
)
.route(
    "/api/v1/ai-store-manager/graphic-images/:id",
    get(handlers::http::ai_store_manager::get_graphic_image),
)
```

- [ ] **Step 6: Run image tests**

Run:

```bash
cargo test -p beebotos-gateway image_
```

Expected: image helper tests pass.

- [ ] **Step 7: Commit image relay API**

```bash
git add apps/gateway/src/handlers/http/ai_store_manager.rs apps/gateway/src/main.rs
git commit -m "feat: add graphic image relay api"
```

## Task 4: Web API Client and Graphic Page

**Files:**
- Modify: `apps/web/src/api/ai_store_manager.rs`
- Modify: `apps/web/src/api/mod.rs`
- Create: `apps/web/src/pages/ai_graphic_marketing.rs`
- Modify: `apps/web/src/pages/mod.rs`

- [ ] **Step 1: Write failing web unit tests**

In `apps/web/src/pages/ai_graphic_marketing.rs`, start with these tests and the public helper names they require:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_graphic_task_targets_xhs() {
        let task = default_graphic_marketing_task();

        assert_eq!(task.product, "云柑礼盒");
        assert_eq!(task.platform, "小红书");
        assert_eq!(task.size, "1024x1536");
    }

    #[test]
    fn fallback_package_contains_prompt_and_titles() {
        let task = default_graphic_marketing_task();
        let package = fallback_graphic_package(&task);

        assert_eq!(package.title_options.len(), 3);
        assert!(package.body.contains("云柑礼盒"));
        assert!(package.image_prompt.contains("小红书"));
    }
}
```

In `apps/web/src/pages/mod.rs`, update `test_page_exports()` to include:

```rust
let _ = AiGraphicMarketingPage;
```

- [ ] **Step 2: Run web tests to verify they fail**

Run:

```bash
cargo test -p beebotos-web ai_graphic
```

Expected: fail because the new page and helpers do not exist.

- [ ] **Step 3: Add API request and response types**

Append to `apps/web/src/api/ai_store_manager.rs`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGraphicPackageRequest {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub price_range: String,
    pub platform: String,
    pub goal: String,
    pub style: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicMarketingCheck {
    pub label: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicPackageResponse {
    pub title_options: Vec<String>,
    pub body: String,
    pub moments_copy: String,
    pub poster_copy: String,
    pub comment_guide: String,
    pub image_prompt: String,
    pub checks: Vec<GraphicMarketingCheck>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGraphicImageRequest {
    pub product: String,
    pub platform: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicImageResponse {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub message: String,
    pub image_url: Option<String>,
    pub b64_json: Option<String>,
}
```

Add methods inside `impl AiStoreManagerService`:

```rust
pub async fn create_graphic_package(
    &self,
    req: &CreateGraphicPackageRequest,
) -> Result<GraphicPackageResponse, ApiError> {
    self.client
        .post("/ai-store-manager/graphic-packages", req)
        .await
}

pub async fn create_graphic_image(
    &self,
    req: &CreateGraphicImageRequest,
) -> Result<GraphicImageResponse, ApiError> {
    self.client
        .post("/ai-store-manager/graphic-images", req)
        .await
}
```

Update `apps/web/src/api/mod.rs` re-export:

```rust
pub use ai_store_manager::{
    AiStoreManagerService, CreateGraphicImageRequest, CreateGraphicPackageRequest,
    CreateVideoTaskRequest, GraphicImageResponse, GraphicMarketingCheck, GraphicPackageResponse,
    VideoTaskResponse,
};
```

- [ ] **Step 4: Create page helpers and component**

Create `apps/web/src/pages/ai_graphic_marketing.rs` with this structure:

```rust
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::Title;

use crate::api::{
    create_ai_store_manager_service, create_client, CreateGraphicImageRequest,
    CreateGraphicPackageRequest, GraphicImageResponse, GraphicMarketingCheck,
    GraphicPackageResponse,
};
use crate::utils::event_target_value;

#[derive(Clone, PartialEq, Eq)]
pub struct GraphicMarketingTask {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub price_range: String,
    pub platform: String,
    pub goal: String,
    pub style: String,
    pub size: String,
    pub quality: String,
}

pub fn default_graphic_marketing_task() -> GraphicMarketingTask {
    GraphicMarketingTask {
        product: "云柑礼盒".to_string(),
        selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
        audience: "25-40 岁办公室人群".to_string(),
        price_range: "99-199 元".to_string(),
        platform: "小红书".to_string(),
        goal: "新品种草".to_string(),
        style: "真实测评".to_string(),
        size: "1024x1536".to_string(),
        quality: "medium".to_string(),
    }
}

pub fn fallback_graphic_package(task: &GraphicMarketingTask) -> GraphicPackageResponse {
    GraphicPackageResponse {
        title_options: vec![
            format!("{}真实体验，{}也会想收藏", task.product, task.audience),
            format!("{}｜{}入手前先看这篇", task.product, task.selling_points),
            format!("{}场景里的{}", task.goal, task.product),
        ],
        body: format!(
            "{}适合{}，主打{}，价格区间{}。内容按{}风格展开，先讲场景，再讲卖点，最后引导评论咨询。",
            task.product, task.audience, task.selling_points, task.price_range, task.style
        ),
        moments_copy: format!(
            "{}到了，{}。想了解规格和到手时间可以留言。",
            task.product, task.selling_points
        ),
        poster_copy: format!(
            "{}\n{}\n{}",
            task.product, task.selling_points, task.price_range
        ),
        comment_guide: format!("评论告诉我你的使用场景，我帮你判断{}是否合适。", task.product),
        image_prompt: format!(
            "为{}生成{}海报，平台{}，突出{}，风格{}，商品主体清晰。",
            task.product, task.goal, task.platform, task.selling_points, task.style
        ),
        checks: vec![
            GraphicMarketingCheck {
                label: "商品卖点完整".to_string(),
                status: "已覆盖".to_string(),
            },
            GraphicMarketingCheck {
                label: "人工审核".to_string(),
                status: "待确认".to_string(),
            },
        ],
    }
}

#[component]
pub fn AiGraphicMarketingPage() -> impl IntoView {
    let (task, set_task) = signal(default_graphic_marketing_task());
    let (package, set_package) = signal(fallback_graphic_package(&task.get_untracked()));
    let (image, set_image) = signal::<Option<GraphicImageResponse>>(None);
    let (status, set_status) = signal("待生成".to_string());
    let (error, set_error) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);
    let service = StoredValue::new(create_ai_store_manager_service(create_client()));

    let generate_package = move || {
        let service = service.get_value();
        let current = task.get();
        let req = CreateGraphicPackageRequest {
            product: current.product.clone(),
            selling_points: current.selling_points.clone(),
            audience: current.audience.clone(),
            price_range: current.price_range.clone(),
            platform: current.platform.clone(),
            goal: current.goal.clone(),
            style: current.style.clone(),
        };

        set_loading.set(true);
        set_error.set(None);
        set_status.set("图文包生成中".to_string());

        spawn_local(async move {
            match service.create_graphic_package(&req).await {
                Ok(result) => {
                    set_package.set(result);
                    set_status.set("待生成图片".to_string());
                }
                Err(err) => {
                    set_error.set(Some(err.to_string()));
                    set_status.set("生成失败".to_string());
                }
            }
            set_loading.set(false);
        });
    };

    let generate_image = move || {
        let service = service.get_value();
        let current = task.get();
        let current_package = package.get();
        let req = CreateGraphicImageRequest {
            product: current.product.clone(),
            platform: current.platform.clone(),
            prompt: current_package.image_prompt,
            size: current.size.clone(),
            quality: current.quality.clone(),
        };

        set_loading.set(true);
        set_error.set(None);
        set_image.set(None);
        set_status.set("图片生成中".to_string());

        spawn_local(async move {
            match service.create_graphic_image(&req).await {
                Ok(result) => {
                    set_image.set(Some(result));
                    set_status.set("图片已生成".to_string());
                }
                Err(err) => {
                    set_error.set(Some(err.to_string()));
                    set_status.set("生成失败".to_string());
                }
            }
            set_loading.set(false);
        });
    };

    view! {
        <Title text="AI 图文营销 - BeeBotOS" />
        <div class="page ai-graphic-marketing-page">
            <div class="page-header ai-video-marketing-header">
                <div>
                    <h2>"AI 图文营销"</h2>
                    <p>"生成小红书和朋友圈可发布的图文营销包与海报素材。"</p>
                </div>
                <div class="ai-store-manager-actions">
                    <a class="btn btn-secondary" href="/ai-store-manager">"返回 AI 店长"</a>
                    <button class="btn btn-primary" disabled=move || loading.get() on:click=move |_| generate_package()>
                        "生成图文包"
                    </button>
                    <button class="btn btn-secondary" disabled=move || loading.get() on:click=move |_| generate_image()>
                        "生成图片"
                    </button>
                </div>
            </div>
            <section class="ai-graphic-workspace">
                <div class="ai-video-marketing-panel">
                    <div class="section-title compact"><h2>"任务配置"</h2></div>
                    <div class="ai-video-status-row"><span>"当前状态"</span><strong>{move || status.get()}</strong></div>
                    <GraphicTaskForm task=task set_task=set_task />
                </div>
                <div class="ai-video-marketing-panel">
                    <div class="section-title compact"><h2>"图文营销包"</h2></div>
                    <GraphicPackageView package=package />
                </div>
                <div class="ai-video-marketing-panel">
                    <div class="section-title compact"><h2>"图片素材"</h2></div>
                    <GraphicImageView package=package image=image error=error />
                </div>
            </section>
        </div>
    }
}
```

Add compact helper components below the main component:

```rust
#[component]
fn GraphicTaskForm(
    task: ReadSignal<GraphicMarketingTask>,
    set_task: WriteSignal<GraphicMarketingTask>,
) -> impl IntoView {
    view! {
        <div class="ai-video-form-grid">
            <TextField label="商品" value=Signal::derive(move || task.get().product) on_input=move |value| set_task.update(|task| task.product = value) />
            <TextField label="核心卖点" value=Signal::derive(move || task.get().selling_points) on_input=move |value| set_task.update(|task| task.selling_points = value) />
            <TextField label="目标人群" value=Signal::derive(move || task.get().audience) on_input=move |value| set_task.update(|task| task.audience = value) />
            <TextField label="价格区间" value=Signal::derive(move || task.get().price_range) on_input=move |value| set_task.update(|task| task.price_range = value) />
            <SelectField label="平台" value=Signal::derive(move || task.get().platform) options=vec!["小红书", "朋友圈"] on_change=move |value| set_task.update(|task| task.platform = value) />
            <SelectField label="营销目标" value=Signal::derive(move || task.get().goal) options=vec!["新品种草", "促销转化", "老客复购"] on_change=move |value| set_task.update(|task| task.goal = value) />
            <SelectField label="内容风格" value=Signal::derive(move || task.get().style) options=vec!["真实测评", "情绪种草", "价格促销"] on_change=move |value| set_task.update(|task| task.style = value) />
            <SelectField label="图片尺寸" value=Signal::derive(move || task.get().size) options=vec!["1024x1536", "1024x1024", "1536x1024"] on_change=move |value| set_task.update(|task| task.size = value) />
        </div>
    }
}
```

Reuse `TextField` and `SelectField` implementations from `ai_video_marketing.rs` by copying them into this file to keep the page self-contained.

Add `GraphicPackageView` and `GraphicImageView`:

```rust
#[component]
fn GraphicPackageView(package: ReadSignal<GraphicPackageResponse>) -> impl IntoView {
    view! {
        <div class="ai-video-result-list">
            {move || package.get().title_options.into_iter().map(|title| view! {
                <article class="ai-video-result-item"><span>"标题"</span><p>{title}</p></article>
            }).collect_view()}
            <article class="ai-video-result-item"><span>"正文"</span><p>{move || package.get().body}</p></article>
            <article class="ai-video-result-item"><span>"朋友圈"</span><p>{move || package.get().moments_copy}</p></article>
            <article class="ai-video-result-item"><span>"海报文案"</span><p>{move || package.get().poster_copy}</p></article>
            <article class="ai-video-result-item"><span>"评论引导"</span><p>{move || package.get().comment_guide}</p></article>
        </div>
    }
}

#[component]
fn GraphicImageView(
    package: ReadSignal<GraphicPackageResponse>,
    image: ReadSignal<Option<GraphicImageResponse>>,
    error: ReadSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="ai-graphic-image-panel">
            <article class="ai-video-result-item"><span>"图片 Prompt"</span><p>{move || package.get().image_prompt}</p></article>
            {move || if let Some(err) = error.get() {
                view! { <div class="ai-video-task-card error">{err}</div> }.into_any()
            } else if let Some(result) = image.get() {
                let src = result.image_url.or_else(|| result.b64_json.map(|b64| format!("data:image/png;base64,{}", b64)));
                view! {
                    <article class="ai-graphic-preview">
                        <p>{result.message}</p>
                        {src.map(|url| view! { <img src=url alt="AI 图文营销海报" /> })}
                    </article>
                }.into_any()
            } else {
                view! { <div class="ai-video-task-card">"生成图片后会在这里预览。"</div> }.into_any()
            }}
        </div>
    }
}
```

- [ ] **Step 5: Register page module exports**

Modify `apps/web/src/pages/mod.rs`:

```rust
pub mod ai_graphic_marketing;
pub use ai_graphic_marketing::AiGraphicMarketingPage;
```

Add `let _ = AiGraphicMarketingPage;` to `test_page_exports()`.

- [ ] **Step 6: Run web helper tests**

Run:

```bash
cargo test -p beebotos-web ai_graphic
```

Expected: tests pass.

- [ ] **Step 7: Commit web page and API client**

```bash
git add apps/web/src/api/ai_store_manager.rs apps/web/src/api/mod.rs apps/web/src/pages/ai_graphic_marketing.rs apps/web/src/pages/mod.rs
git commit -m "feat: add graphic marketing web page"
```

## Task 5: Web Routing, Entry, and Styles

**Files:**
- Modify: `apps/web/src/pages/ai_store_manager.rs`
- Modify: `apps/web/src/lib.rs`
- Modify: `apps/web/style/main.css`

- [ ] **Step 1: Write failing route and entry tests**

Update `apps/web/src/pages/ai_store_manager.rs` test `store_manager_modules_cover_marketing_channels` to assert the graphic module has the correct link:

```rust
let graphic = ai_store_manager_modules()
    .iter()
    .find(|module| module.title_key == "ai-store-manager-graphic-marketing")
    .expect("graphic marketing module exists");
assert_eq!(graphic.href, Some("/ai-store-manager/graphic-marketing"));
```

Add a small route export assertion in `apps/web/src/pages/mod.rs` test:

```rust
let _ = AiGraphicMarketingPage;
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p beebotos-web store_manager_modules_cover_marketing_channels
```

Expected: fail because graphic module `href` is `None`.

- [ ] **Step 3: Link the AI 店长 card**

Modify the graphic module in `apps/web/src/pages/ai_store_manager.rs`:

```rust
StoreManagerModule {
    title_key: "ai-store-manager-graphic-marketing",
    summary_key: "ai-store-manager-graphic-desc",
    status_key: "ai-store-manager-graphic-core",
    icon: "🖼️",
    action_key: "ai-store-manager-create-graphic",
    href: Some("/ai-store-manager/graphic-marketing"),
},
```

- [ ] **Step 4: Register the frontend route**

Modify imports in `apps/web/src/lib.rs`:

```rust
use pages::{
    AgentDetail, AgentsPage, AiCommercePage, AiGraphicMarketingPage, AiStoreManagerPage,
    AiVideoMarketingPage, ChannelsPage, DaoPage, Home, LlmConfigPage, LlmSettingsPage, LoginPage,
    McpServerPage, NotFound, RegisterPage, SettingsPage, SetupPage, SkillInstancesPage, SkillsPage,
    TreasuryPage, TreasuryTransactionsPage, WorkflowDashboardPage, WorkflowDetailPage,
};
```

Add route before `/ai-store-manager/video-marketing` and `/ai-store-manager`:

```rust
<Route
    path=(StaticSegment("ai-store-manager"), StaticSegment("graphic-marketing"))
    view=move || view! {
        <AuthGuard>
            <AiGraphicMarketingPage />
        </AuthGuard>
    }
/>
```

- [ ] **Step 5: Add responsive styles**

Append near the AI video marketing CSS in `apps/web/style/main.css`:

```css
.ai-graphic-marketing-page {
    display: flex;
    flex-direction: column;
    gap: 24px;
}

.ai-graphic-workspace {
    display: grid;
    grid-template-columns: 340px minmax(0, 1fr) 360px;
    gap: 20px;
    align-items: start;
}

.ai-graphic-image-panel {
    display: grid;
    gap: 12px;
}

.ai-graphic-preview {
    background: var(--bg-glass);
    border: 1px solid var(--border-light);
    border-radius: var(--radius-sm);
    padding: 14px;
    display: grid;
    gap: 12px;
}

.ai-graphic-preview p {
    color: var(--text-secondary);
    font-size: 13px;
    margin: 0;
}

.ai-graphic-preview img {
    width: 100%;
    max-height: 560px;
    object-fit: contain;
    border-radius: var(--radius-sm);
    background: var(--bg-card);
}

@media (max-width: 1280px) {
    .ai-graphic-workspace {
        grid-template-columns: 340px minmax(0, 1fr);
    }

    .ai-graphic-workspace > :last-child {
        grid-column: 1 / -1;
    }
}

@media (max-width: 900px) {
    .ai-graphic-workspace {
        grid-template-columns: 1fr;
    }

    .ai-graphic-workspace > :last-child {
        grid-column: auto;
    }
}
```

- [ ] **Step 6: Run web route and compile checks**

Run:

```bash
cargo test -p beebotos-web store_manager_modules_cover_marketing_channels
cargo check -p beebotos-web --target wasm32-unknown-unknown
```

Expected: test passes and wasm check completes.

- [ ] **Step 7: Commit routing and styles**

```bash
git add apps/web/src/pages/ai_store_manager.rs apps/web/src/lib.rs apps/web/style/main.css
git commit -m "feat: wire graphic marketing route"
```

## Task 6: Final Verification

**Files:**
- Verify all files changed in Tasks 1-5.

- [ ] **Step 1: Run focused Gateway tests**

```bash
cargo test -p beebotos-gateway ai_store_manager
```

Expected: AI Store Manager tests pass.

- [ ] **Step 2: Run focused Web tests**

```bash
cargo test -p beebotos-web ai_graphic
cargo test -p beebotos-web store_manager_modules_cover_marketing_channels
```

Expected: both commands pass.

- [ ] **Step 3: Run Web compile check**

```bash
cargo check -p beebotos-web --target wasm32-unknown-unknown
```

Expected: compile check passes.

- [ ] **Step 4: Inspect final diff**

```bash
git diff --stat HEAD~5..HEAD
git status --short
```

Expected: commits contain only AI 图文营销 MVP files and the working tree is clean.

- [ ] **Step 5: Manual runtime check**

Start or restart BeeBotOS services using the project startup flow, then open:

```text
http://127.0.0.1:8090/ai-store-manager
http://127.0.0.1:8090/ai-store-manager/graphic-marketing
```

Expected:

- AI 图文营销 card opens the new page.
- Clicking 生成图文包 shows a structured package.
- Without `IMAGE_GENERATION_API_KEY`, clicking 生成图片 shows “图片生成未配置”.
- With a valid relay config, clicking 生成图片 shows a generated image preview.

- [ ] **Step 6: Commit verification fixes if needed**

Only if verification exposes a code defect, make the minimal fix and commit:

```bash
git add apps/gateway/src/config.rs config/beebotos.toml apps/gateway/src/handlers/http/ai_store_manager.rs apps/gateway/src/main.rs apps/web/src/api/ai_store_manager.rs apps/web/src/api/mod.rs apps/web/src/pages/ai_graphic_marketing.rs apps/web/src/pages/mod.rs apps/web/src/pages/ai_store_manager.rs apps/web/src/lib.rs apps/web/style/main.css
git commit -m "fix: stabilize graphic marketing mvp"
```
