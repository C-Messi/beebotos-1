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

pub async fn create_video_task(
    user: AuthUser,
    Json(req): Json<CreateVideoTaskRequest>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    Ok(Json(create_mock_video_task(&req)))
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
}
