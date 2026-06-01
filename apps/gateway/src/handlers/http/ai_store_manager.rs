use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use base64::engine::general_purpose;
use base64::Engine as _;
use beebotos_agents::llm::Message as LLMMessage;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::BeeBotOSConfig;
use crate::error::GatewayError;
use crate::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateVideoTaskRequest {
    pub product: String,
    pub platform: String,
    pub version: String,
    pub prompt: String,
    #[serde(default = "default_video_task_duration")]
    pub duration_seconds: u8,
    #[serde(default = "default_video_task_resolution")]
    pub resolution: String,
    #[serde(default = "default_video_task_ratio")]
    pub ratio: String,
    #[serde(default = "default_video_task_audio")]
    pub generate_audio: bool,
    #[serde(default)]
    pub watermark: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoTaskResponse {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub message: String,
    pub preview_url: Option<String>,
}

fn default_video_task_duration() -> u8 {
    5
}

fn default_video_task_resolution() -> String {
    "720p".to_string()
}

fn default_video_task_ratio() -> String {
    "9:16".to_string()
}

fn default_video_task_audio() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct VolcengineVideoTaskRequest {
    model: String,
    content: Vec<VolcengineVideoContentInput>,
    resolution: String,
    ratio: String,
    duration: u8,
    generate_audio: bool,
    watermark: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct VolcengineVideoContentInput {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineVideoTaskCreateResponse {
    id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineVideoTaskStatusResponse {
    id: String,
    model: Option<String>,
    status: String,
    content: Option<VolcengineVideoTaskContent>,
    error: Option<VolcengineVideoTaskError>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineVideoTaskContent {
    video_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineVideoTaskError {
    message: Option<String>,
}

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

fn build_seedance_video_payload(
    model: &str,
    req: &CreateVideoTaskRequest,
) -> VolcengineVideoTaskRequest {
    VolcengineVideoTaskRequest {
        model: model.to_string(),
        content: vec![VolcengineVideoContentInput {
            kind: "text".to_string(),
            text: req.prompt.trim().to_string(),
        }],
        resolution: req.resolution.trim().to_string(),
        ratio: req.ratio.trim().to_string(),
        duration: req.duration_seconds,
        generate_audio: req.generate_audio,
        watermark: req.watermark,
    }
}

fn validate_video_task_request(req: &CreateVideoTaskRequest) -> Result<(), GatewayError> {
    if req.product.trim().is_empty() {
        return Err(GatewayError::bad_request("商品不能为空"));
    }
    if req.platform.trim().is_empty() {
        return Err(GatewayError::bad_request("平台不能为空"));
    }
    if req.version.trim().is_empty() {
        return Err(GatewayError::bad_request("素材版本不能为空"));
    }
    if req.prompt.trim().is_empty() {
        return Err(GatewayError::bad_request("视频提示词不能为空"));
    }
    if !(4..=15).contains(&req.duration_seconds) {
        return Err(GatewayError::bad_request("视频时长需在 4 到 15 秒之间"));
    }
    if !matches!(req.resolution.trim(), "480p" | "720p" | "1080p") {
        return Err(GatewayError::bad_request("不支持的视频分辨率"));
    }
    if !matches!(
        req.ratio.trim(),
        "16:9" | "4:3" | "1:1" | "3:4" | "9:16" | "21:9" | "adaptive"
    ) {
        return Err(GatewayError::bad_request("不支持的视频比例"));
    }
    Ok(())
}

fn video_task_url(base_url: &str) -> String {
    format!(
        "{}/contents/generations/tasks",
        base_url.trim_end_matches('/')
    )
}

fn video_task_status_url(base_url: &str, id: &str) -> String {
    format!("{}/{}", video_task_url(base_url), id)
}

fn video_generation_config_required_response(model: &str) -> VideoTaskResponse {
    VideoTaskResponse {
        id: "seedance2-config-required".to_string(),
        provider: "volcengine-ark".to_string(),
        model: model.to_string(),
        status: "blocked".to_string(),
        message: "火山方舟视频生成未配置 VIDEO_GENERATION_API_KEY；开通 Seedance 2.0 \
                  并配置后即可提交真实任务。"
            .to_string(),
        preview_url: None,
    }
}

fn video_upstream_unavailable_response(
    model: &str,
    status: reqwest::StatusCode,
) -> VideoTaskResponse {
    let blocked =
        matches!(status.as_u16(), 401 | 402 | 403) || status == reqwest::StatusCode::NOT_FOUND;
    VideoTaskResponse {
        id: "seedance2-upstream-unavailable".to_string(),
        provider: "volcengine-ark".to_string(),
        model: model.to_string(),
        status: if blocked { "blocked" } else { "failed" }.to_string(),
        message: if blocked {
            format!("火山方舟鉴权、余额或模型订阅未就绪: HTTP {}", status)
        } else {
            format!("火山方舟视频任务提交失败: HTTP {}", status)
        },
        preview_url: None,
    }
}

fn video_create_to_response(
    created: VolcengineVideoTaskCreateResponse,
    model: &str,
) -> VideoTaskResponse {
    VideoTaskResponse {
        id: created.id,
        provider: "volcengine-ark".to_string(),
        model: model.to_string(),
        status: "queued".to_string(),
        message: "Seedance 2.0 视频任务已提交，正在排队生成。".to_string(),
        preview_url: None,
    }
}

fn video_status_to_response(
    upstream: VolcengineVideoTaskStatusResponse,
    fallback_model: &str,
) -> VideoTaskResponse {
    let status = match upstream.status.as_str() {
        "succeeded" => "completed",
        "queued" | "running" | "failed" | "expired" | "cancelled" => upstream.status.as_str(),
        _ => "running",
    };
    let preview_url = upstream.content.and_then(|content| content.video_url);
    let message = upstream
        .error
        .and_then(|error| error.message)
        .unwrap_or_else(|| match status {
            "completed" => "Seedance 2.0 视频已生成。".to_string(),
            "queued" => "Seedance 2.0 视频任务排队中。".to_string(),
            "running" => "Seedance 2.0 视频生成中。".to_string(),
            "expired" => "Seedance 2.0 视频任务已超时。".to_string(),
            "cancelled" => "Seedance 2.0 视频任务已取消。".to_string(),
            "failed" => "Seedance 2.0 视频任务失败。".to_string(),
            _ => "Seedance 2.0 视频任务状态已更新。".to_string(),
        });

    VideoTaskResponse {
        id: upstream.id,
        provider: "volcengine-ark".to_string(),
        model: upstream.model.unwrap_or_else(|| fallback_model.to_string()),
        status: status.to_string(),
        message,
        preview_url,
    }
}

pub fn create_graphic_package(req: &CreateGraphicPackageRequest) -> GraphicPackageResponse {
    let platform_prefix = if req.platform == "朋友圈" {
        "朋友圈私域"
    } else {
        "小红书种草"
    };

    GraphicPackageResponse {
        title_options: vec![
            format!(
                "{}｜{}也会想收藏的{}",
                platform_prefix, req.audience, req.product
            ),
            format!(
                "{}真实体验：{}值不值得入手",
                req.product, req.selling_points
            ),
            format!("{}，{}场景里的体面选择", req.product, req.goal),
        ],
        body: format!(
            "这次推荐{}，面向{}，主打{}。价格区间{}，适合{}内容风格。先讲真实使用场景，\
             再补充购买理由，最后引导用户评论咨询。",
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
            "为{}生成{}营销海报，平台是{}，目标人群是{}，突出{}，价格区间{}，风格{}。\
             画面干净真实，商品主体清晰，适合中文电商内容发布。",
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

fn build_graphic_package_prompt(req: &CreateGraphicPackageRequest) -> String {
    format!(
        r#"请为 AI 店长生成一套可直接发布的图文营销包。

任务信息：
- 商品：{product}
- 核心卖点：{selling_points}
- 目标人群：{audience}
- 价格区间：{price_range}
- 发布平台：{platform}
- 营销目标：{goal}
- 内容风格：{style}

要求：
1. 文案必须贴合商品、平台、人群和目标，不要使用通用模板话术。
2. 小红书正文要有真实使用场景、购买理由和评论互动引导。
3. 朋友圈文案要短，适合私域发布。
4. 海报文案要适合放在图片上，短句优先。
5. image_prompt 用于图片模型生成营销图，必须包含商品主体、平台、卖点、风格和画面要求。
6. 只输出 JSON，不要 Markdown，不要解释。

JSON 结构：
{{
  "title_options": ["标题一", "标题二", "标题三"],
  "body": "小红书正文",
  "moments_copy": "朋友圈文案",
  "poster_copy": "海报文案",
  "comment_guide": "评论引导",
  "image_prompt": "图片生成提示词",
  "checks": [
    {{"label": "商品卖点完整", "status": "已覆盖"}},
    {{"label": "平台风格匹配", "status": "已覆盖"}},
    {{"label": "人工审核", "status": "待确认"}}
  ]
}}"#,
        product = req.product,
        selling_points = req.selling_points,
        audience = req.audience,
        price_range = req.price_range,
        platform = req.platform,
        goal = req.goal,
        style = req.style
    )
}

fn extract_json_object(raw: &str) -> Result<&str, GatewayError> {
    let trimmed = raw.trim();
    let start = trimmed
        .find('{')
        .ok_or_else(|| GatewayError::internal("图文营销包响应缺少 JSON"))?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| GatewayError::internal("图文营销包响应缺少 JSON"))?;

    if start > end {
        return Err(GatewayError::internal("图文营销包 JSON 格式错误"));
    }

    Ok(&trimmed[start..=end])
}

fn validate_graphic_package(package: &GraphicPackageResponse) -> Result<(), GatewayError> {
    if package.title_options.len() < 3 {
        return Err(GatewayError::internal("图文营销标题不足"));
    }
    if package.body.trim().is_empty()
        || package.moments_copy.trim().is_empty()
        || package.poster_copy.trim().is_empty()
        || package.comment_guide.trim().is_empty()
        || package.image_prompt.trim().is_empty()
    {
        return Err(GatewayError::internal("图文营销包内容不完整"));
    }
    Ok(())
}

fn graphic_package_from_llm_response(
    raw: &str,
    req: &CreateGraphicPackageRequest,
) -> Result<GraphicPackageResponse, GatewayError> {
    let json = extract_json_object(raw)?;
    let mut package: GraphicPackageResponse = serde_json::from_str(json)
        .map_err(|err| GatewayError::internal(format!("图文营销包 JSON 解析失败: {}", err)))?;

    package.title_options.truncate(3);
    if package.checks.is_empty() {
        package.checks = create_graphic_package(req).checks;
    }
    validate_graphic_package(&package)?;

    Ok(package)
}

async fn generate_graphic_package_with_llm(
    state: &AppState,
    req: &CreateGraphicPackageRequest,
) -> Result<GraphicPackageResponse, GatewayError> {
    let messages = vec![
        LLMMessage::system("你是资深电商内容运营，只返回符合用户 JSON schema 的中文图文营销素材。"),
        LLMMessage::user(build_graphic_package_prompt(req)),
    ];
    let response = state
        .llm_service
        .chat(messages, Some(1800), None, Some("none".to_string()), None)
        .await?;

    graphic_package_from_llm_response(&response, req)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateGraphicImageRequest {
    pub product: String,
    pub platform: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateGraphicImageEditRequest {
    pub product: String,
    pub platform: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
    pub image_b64: String,
    pub image_mime_type: String,
    pub image_filename: String,
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
        prompt: req.prompt.trim().to_string(),
        size: req.size.trim().to_string(),
        quality: req.quality.trim().to_string(),
        n: 1,
    }
}

fn validate_graphic_image_request(req: &CreateGraphicImageRequest) -> Result<(), GatewayError> {
    if req.product.trim().is_empty() {
        return Err(GatewayError::bad_request("商品不能为空"));
    }
    if req.platform.trim().is_empty() {
        return Err(GatewayError::bad_request("平台不能为空"));
    }
    if req.prompt.trim().is_empty() {
        return Err(GatewayError::bad_request("图片生成提示词不能为空"));
    }
    if req.size.trim().is_empty() {
        return Err(GatewayError::bad_request("图片尺寸不能为空"));
    }
    if req.quality.trim().is_empty() {
        return Err(GatewayError::bad_request("图片质量不能为空"));
    }

    if !matches!(req.size.trim(), "1024x1536" | "1024x1024" | "1536x1024") {
        return Err(GatewayError::bad_request("不支持的图片尺寸"));
    }
    if !matches!(req.quality.trim(), "low" | "medium" | "high" | "auto") {
        return Err(GatewayError::bad_request("不支持的图片质量"));
    }

    Ok(())
}

fn decode_graphic_image_upload(
    req: &CreateGraphicImageEditRequest,
) -> Result<Vec<u8>, GatewayError> {
    if req.image_b64.trim().is_empty() {
        return Err(GatewayError::bad_request("请先上传产品图"));
    }
    general_purpose::STANDARD
        .decode(req.image_b64.trim())
        .map_err(|_| GatewayError::bad_request("产品图数据格式错误"))
}

fn validate_graphic_image_edit_request(
    req: &CreateGraphicImageEditRequest,
) -> Result<(), GatewayError> {
    validate_graphic_image_request(&CreateGraphicImageRequest {
        product: req.product.clone(),
        platform: req.platform.clone(),
        prompt: req.prompt.clone(),
        size: req.size.clone(),
        quality: req.quality.clone(),
    })?;
    if !matches!(
        req.image_mime_type.trim(),
        "image/png" | "image/jpeg" | "image/webp"
    ) {
        return Err(GatewayError::bad_request("仅支持 PNG、JPG、WebP 产品图"));
    }
    let _ = decode_graphic_image_upload(req)?;
    Ok(())
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

fn image_edit_url(base_url: &str) -> String {
    format!("{}/images/edits", base_url.trim_end_matches('/'))
}

fn image_model_unavailable_message(status: reqwest::StatusCode) -> String {
    format!("图片模型不可用: HTTP {}", status)
}

pub async fn create_video_task(
    user: AuthUser,
    Json(req): Json<CreateVideoTaskRequest>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    validate_video_task_request(&req)?;

    let config = BeeBotOSConfig::load()
        .map_err(|err| GatewayError::internal(format!("加载视频生成配置失败: {}", err)))?;
    let model = config.video_generation.model.clone();
    let api_key = match config
        .video_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
    {
        Some(api_key) => api_key,
        None => return Ok(Json(video_generation_config_required_response(&model))),
    };

    let payload = build_seedance_video_payload(&model, &req);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.video_generation.timeout_seconds,
        ))
        .build()
        .map_err(|err| GatewayError::internal(format!("视频生成客户端创建失败: {}", err)))?;

    let response = client
        .post(video_task_url(&config.video_generation.base_url))
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("视频生成请求失败: {}", err)))?;

    let status = response.status();
    if !status.is_success() {
        return Ok(Json(video_upstream_unavailable_response(&model, status)));
    }

    let created = response
        .json::<VolcengineVideoTaskCreateResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("视频生成响应解析失败: {}", err)))?;

    Ok(Json(video_create_to_response(created, &model)))
}

pub async fn create_graphic_package_handler(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateGraphicPackageRequest>,
) -> Result<Json<GraphicPackageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let package = match generate_graphic_package_with_llm(&state, &req).await {
        Ok(package) => package,
        Err(err) => {
            warn!("AI graphic package fell back to template: {}", err);
            let mut fallback = create_graphic_package(&req);
            fallback.checks.insert(
                0,
                GraphicMarketingCheck {
                    label: "文案生成".to_string(),
                    status: "模板兜底".to_string(),
                },
            );
            fallback
        }
    };
    Ok(Json(package))
}

pub async fn create_graphic_image(
    user: AuthUser,
    Json(req): Json<CreateGraphicImageRequest>,
) -> Result<Json<GraphicImageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    validate_graphic_image_request(&req)?;

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
        return Err(GatewayError::internal(image_model_unavailable_message(
            status,
        )));
    }

    let image_response = response
        .json::<OpenAIImageResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("图片生成响应解析失败: {}", err)))?;
    let result = image_response_to_graphic_result(&req.product, &req.platform, image_response)?;

    Ok(Json(result))
}

pub async fn create_graphic_image_edit(
    user: AuthUser,
    Json(req): Json<CreateGraphicImageEditRequest>,
) -> Result<Json<GraphicImageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    validate_graphic_image_edit_request(&req)?;
    let image_bytes = decode_graphic_image_upload(&req)?;

    let config = BeeBotOSConfig::load()
        .map_err(|err| GatewayError::internal(format!("加载图片生成配置失败: {}", err)))?;
    let api_key = config
        .image_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| GatewayError::bad_request("图片生成未配置"))?;

    let image_part = reqwest::multipart::Part::bytes(image_bytes)
        .file_name(req.image_filename.clone())
        .mime_str(&req.image_mime_type)
        .map_err(|err| GatewayError::bad_request(format!("产品图格式错误: {}", err)))?;
    let form = reqwest::multipart::Form::new()
        .text("model", config.image_generation.model.clone())
        .text("prompt", req.prompt.trim().to_string())
        .text("size", req.size.trim().to_string())
        .text("quality", req.quality.trim().to_string())
        .text("n", "1")
        .part("image", image_part);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.image_generation.timeout_seconds,
        ))
        .build()
        .map_err(|err| GatewayError::internal(format!("图片编辑客户端创建失败: {}", err)))?;

    let response = client
        .post(image_edit_url(&config.image_generation.base_url))
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("图片编辑请求失败: {}", err)))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(GatewayError::bad_request("图片生成鉴权失败"));
    }
    if !status.is_success() {
        return Err(GatewayError::internal(image_model_unavailable_message(
            status,
        )));
    }

    let image_response = response
        .json::<OpenAIImageResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("图片编辑响应解析失败: {}", err)))?;
    let result = image_response_to_graphic_result(&req.product, &req.platform, image_response)?;

    Ok(Json(result))
}

pub async fn get_video_task(
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let config = BeeBotOSConfig::load()
        .map_err(|err| GatewayError::internal(format!("加载视频生成配置失败: {}", err)))?;
    let model = config.video_generation.model.clone();
    let api_key = match config
        .video_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
    {
        Some(api_key) => api_key,
        None => {
            let mut response = video_generation_config_required_response(&model);
            response.id = id;
            return Ok(Json(response));
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.video_generation.timeout_seconds,
        ))
        .build()
        .map_err(|err| GatewayError::internal(format!("视频生成客户端创建失败: {}", err)))?;

    let response = client
        .get(video_task_status_url(
            &config.video_generation.base_url,
            &id,
        ))
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务查询失败: {}", err)))?;

    let status = response.status();
    if !status.is_success() {
        let mut fallback = video_upstream_unavailable_response(&model, status);
        fallback.id = id;
        return Ok(Json(fallback));
    }

    let upstream = response
        .json::<VolcengineVideoTaskStatusResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务响应解析失败: {}", err)))?;

    Ok(Json(video_status_to_response(upstream, &model)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seedance_payload_uses_text_content_and_generation_options() {
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: "种草版".to_string(),
            prompt: "为云柑礼盒生成短视频".to_string(),
            duration_seconds: 8,
            resolution: "1080p".to_string(),
            ratio: "16:9".to_string(),
            generate_audio: false,
            watermark: true,
        };

        let payload = build_seedance_video_payload("doubao-seedance-2-0-260128", &req);

        assert_eq!(payload.model, "doubao-seedance-2-0-260128");
        assert_eq!(payload.content[0].kind, "text");
        assert_eq!(payload.content[0].text, "为云柑礼盒生成短视频");
        assert_eq!(payload.duration, 8);
        assert_eq!(payload.resolution, "1080p");
        assert_eq!(payload.ratio, "16:9");
        assert!(!payload.generate_audio);
        assert!(payload.watermark);
    }

    #[test]
    fn seedance_status_response_maps_succeeded_to_completed_preview() {
        let upstream = VolcengineVideoTaskStatusResponse {
            id: "task-1".to_string(),
            model: Some("doubao-seedance-2-0-260128".to_string()),
            status: "succeeded".to_string(),
            content: Some(VolcengineVideoTaskContent {
                video_url: Some("https://cdn.example/video.mp4".to_string()),
            }),
            error: None,
        };

        let response = video_status_to_response(upstream, "fallback-model");

        assert_eq!(response.id, "task-1");
        assert_eq!(response.provider, "volcengine-ark");
        assert_eq!(response.model, "doubao-seedance-2-0-260128");
        assert_eq!(response.status, "completed");
        assert_eq!(
            response.preview_url.as_deref(),
            Some("https://cdn.example/video.mp4")
        );
    }

    #[test]
    fn seedance_config_required_response_is_blocked() {
        let response = video_generation_config_required_response("doubao-seedance-2-0-260128");

        assert_eq!(response.provider, "volcengine-ark");
        assert_eq!(response.model, "doubao-seedance-2-0-260128");
        assert_eq!(response.status, "blocked");
        assert!(response.message.contains("VIDEO_GENERATION_API_KEY"));
        assert!(response.preview_url.is_none());
    }

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
        assert!(package
            .checks
            .iter()
            .any(|check| check.label == "商品卖点完整"));
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

    #[test]
    fn graphic_package_prompt_requires_json_and_task_fields() {
        let req = CreateGraphicPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            price_range: "99-199 元".to_string(),
            platform: "小红书".to_string(),
            goal: "新品种草".to_string(),
            style: "真实测评".to_string(),
        };

        let prompt = build_graphic_package_prompt(&req);

        assert!(prompt.contains("只输出 JSON"));
        assert!(prompt.contains("云柑礼盒"));
        assert!(prompt.contains("当季鲜果、顺丰冷链、送礼体面"));
        assert!(prompt.contains("image_prompt"));
    }

    #[test]
    fn graphic_package_from_llm_response_accepts_fenced_json() {
        let req = CreateGraphicPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            price_range: "99-199 元".to_string(),
            platform: "小红书".to_string(),
            goal: "新品种草".to_string(),
            style: "真实测评".to_string(),
        };
        let raw = r#"```json
{
  "title_options": ["标题一", "标题二", "标题三"],
  "body": "小红书正文包含云柑礼盒",
  "moments_copy": "朋友圈文案",
  "poster_copy": "海报文案",
  "comment_guide": "评论引导",
  "image_prompt": "图片提示词",
  "checks": [
    {"label": "商品卖点完整", "status": "已覆盖"},
    {"label": "平台风格匹配", "status": "已覆盖"},
    {"label": "人工审核", "status": "待确认"}
  ]
}
```"#;

        let package = graphic_package_from_llm_response(raw, &req).unwrap();

        assert_eq!(package.title_options.len(), 3);
        assert_eq!(package.body, "小红书正文包含云柑礼盒");
        assert_eq!(package.image_prompt, "图片提示词");
    }

    #[test]
    fn graphic_package_from_llm_response_rejects_incomplete_json() {
        let req = CreateGraphicPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            price_range: "99-199 元".to_string(),
            platform: "小红书".to_string(),
            goal: "新品种草".to_string(),
            style: "真实测评".to_string(),
        };
        let raw = r#"{"title_options":["标题一"],"body":"","moments_copy":"","poster_copy":"","comment_guide":"","image_prompt":"","checks":[]}"#;

        assert!(graphic_package_from_llm_response(raw, &req).is_err());
    }

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
    fn image_edit_url_uses_edits_endpoint() {
        assert_eq!(
            image_edit_url("https://image.example/"),
            "https://image.example/images/edits"
        );
    }

    #[test]
    fn image_edit_request_validation_requires_product_image() {
        let mut req = CreateGraphicImageEditRequest {
            product: "云柑礼盒".to_string(),
            platform: "小红书".to_string(),
            prompt: "生成营销图".to_string(),
            size: "1024x1024".to_string(),
            quality: "low".to_string(),
            image_b64: "".to_string(),
            image_mime_type: "image/png".to_string(),
            image_filename: "product.png".to_string(),
        };

        assert!(validate_graphic_image_edit_request(&req).is_err());

        req.image_b64 = "aGVsbG8=".to_string();
        assert!(validate_graphic_image_edit_request(&req).is_ok());
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

    #[test]
    fn image_upstream_error_message_omits_provider_body() {
        let message = image_model_unavailable_message(reqwest::StatusCode::BAD_REQUEST);

        assert_eq!(message, "图片模型不可用: HTTP 400 Bad Request");
        assert!(!message.contains("secret"));
    }

    #[test]
    fn image_request_validation_rejects_empty_and_invalid_options() {
        let mut req = CreateGraphicImageRequest {
            product: "云柑礼盒".to_string(),
            platform: "小红书".to_string(),
            prompt: "生成海报".to_string(),
            size: "1024x1536".to_string(),
            quality: "medium".to_string(),
        };
        assert!(validate_graphic_image_request(&req).is_ok());

        req.product = " ".to_string();
        assert!(validate_graphic_image_request(&req).is_err());

        req.product = "云柑礼盒".to_string();
        req.size = "512x512".to_string();
        assert!(validate_graphic_image_request(&req).is_err());

        req.size = "1024x1536".to_string();
        req.quality = "best".to_string();
        assert!(validate_graphic_image_request(&req).is_err());
    }
}
