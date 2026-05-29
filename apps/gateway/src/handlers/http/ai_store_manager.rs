use axum::extract::Path;
use axum::Json;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};

use crate::config::BeeBotOSConfig;
use crate::error::GatewayError;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateVideoTaskRequest {
    pub product: String,
    pub platform: String,
    pub version: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoTaskResponse {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub message: String,
    pub preview_url: Option<String>,
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

pub fn create_mock_video_task(req: &CreateVideoTaskRequest) -> VideoTaskResponse {
    VideoTaskResponse {
        id: format!("seedance-mock-{}-{}", req.platform, req.version).replace(' ', "-"),
        provider: "seedance2-mock".to_string(),
        status: "queued".to_string(),
        message: format!("{} 视频生成任务已进入 Seedance 预留队列。", req.product),
        preview_url: None,
    }
}

pub fn mock_video_task_status(id: &str) -> VideoTaskResponse {
    VideoTaskResponse {
        id: id.to_string(),
        provider: "seedance2-mock".to_string(),
        status: "completed".to_string(),
        message: "mock 视频已生成，可替换为 Seedance 真实结果。".to_string(),
        preview_url: Some("/public/mock/ai-video-marketing-preview.mp4".to_string()),
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

fn image_model_unavailable_message(status: reqwest::StatusCode) -> String {
    format!("图片模型不可用: HTTP {}", status)
}

pub async fn create_video_task(
    user: AuthUser,
    Json(req): Json<CreateVideoTaskRequest>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(create_mock_video_task(&req)))
}

pub async fn create_graphic_package_handler(
    user: AuthUser,
    Json(req): Json<CreateGraphicPackageRequest>,
) -> Result<Json<GraphicPackageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(create_graphic_package(&req)))
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

pub async fn get_video_task(
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(mock_video_task_status(&id)))
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
    fn mock_seedance_task_uses_seedance_provider_name() {
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: "种草版".to_string(),
            prompt: "生成短视频".to_string(),
        };

        let task = create_mock_video_task(&req);

        assert_eq!(task.provider, "seedance2-mock");
        assert_eq!(task.status, "queued");
        assert!(task.id.contains("seedance-mock"));
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
