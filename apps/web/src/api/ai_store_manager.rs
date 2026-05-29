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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicImageResponse {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub message: String,
    pub image_url: Option<String>,
    pub b64_json: Option<String>,
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

    pub async fn create_graphic_image_edit(
        &self,
        req: &CreateGraphicImageEditRequest,
    ) -> Result<GraphicImageResponse, ApiError> {
        self.client
            .post("/ai-store-manager/graphic-image-edits", req)
            .await
    }
}
