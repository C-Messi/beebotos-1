use serde::{Deserialize, Serialize};

use crate::api::{ApiClient, ApiError};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateVideoTaskRequest {
    pub product: String,
    pub platform: String,
    pub version: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoTaskResponse {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub message: String,
    pub preview_url: Option<String>,
}

#[derive(Clone)]
pub struct AiStoreManagerService {
    client: ApiClient,
}

impl AiStoreManagerService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn create_video_task(
        &self,
        req: &CreateVideoTaskRequest,
    ) -> Result<VideoTaskResponse, ApiError> {
        self.client.post("/ai-store-manager/video-tasks", req).await
    }

    pub async fn get_video_task(&self, id: &str) -> Result<VideoTaskResponse, ApiError> {
        self.client
            .get(&format!("/ai-store-manager/video-tasks/{}", id))
            .await
    }
}
