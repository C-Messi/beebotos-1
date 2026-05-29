use axum::extract::Path;
use axum::Json;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};

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

pub async fn get_video_task(
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(mock_video_task_status(&id)))
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
}
