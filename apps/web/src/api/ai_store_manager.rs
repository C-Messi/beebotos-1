use serde::{Deserialize, Serialize};

use crate::api::{ApiClient, ApiError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceImageRequest {
    pub mime_type: String,
    pub data_url: String,
    pub file_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateVideoTaskRequest {
    pub product: String,
    pub platform: String,
    pub prompt: String,
    pub model: Option<String>,
    pub duration_seconds: u8,
    pub resolution: String,
    pub ratio: String,
    pub generate_audio: bool,
    pub watermark: bool,
    pub reference_images: Vec<ReferenceImageRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoTaskResponse {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub message: String,
    pub preview_url: Option<String>,
    pub resolution: Option<String>,
    pub ratio: Option<String>,
    pub duration_seconds: Option<u8>,
    pub queue_position: Option<u32>,
    pub submitted_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub reference_image_count: Option<u8>,
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
    #[serde(default)]
    pub history_id: Option<String>,
    pub title_options: Vec<String>,
    pub body: String,
    pub moments_copy: String,
    pub poster_copy: String,
    pub comment_guide: String,
    pub image_prompt: String,
    pub checks: Vec<GraphicMarketingCheck>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateVideoPackageRequest {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub goal: String,
    pub platform: String,
    pub style: String,
    pub duration_seconds: Option<u8>,
    pub ratio: Option<String>,
    pub generate_audio: Option<bool>,
    pub reference_images: Vec<ReferenceImageRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoPackageResponse {
    pub title: String,
    pub hook: String,
    pub oral_script: String,
    pub storyboard: Vec<String>,
    pub subtitles: Vec<String>,
    pub shot_prompts: Vec<String>,
    pub tags: Vec<String>,
    pub video_prompt: String,
    pub checks: Vec<GraphicMarketingCheck>,
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGraphicImageRequest {
    pub product: String,
    pub platform: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
    #[serde(default)]
    pub package_id: Option<String>,
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
    #[serde(default)]
    pub package_id: Option<String>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicMarketingHistoryItem {
    pub id: String,
    pub product: String,
    pub platform: String,
    pub package: Option<GraphicPackageResponse>,
    pub image: Option<GraphicImageResponse>,
    pub image_prompt: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub source_image_filename: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
            .invalidate_cache(&format!("GET:/ai-store-manager/video-tasks/{}", id));
        self.client
            .get(&format!("/ai-store-manager/video-tasks/{}", id))
            .await
    }

    pub async fn list_video_tasks(&self) -> Result<Vec<VideoTaskResponse>, ApiError> {
        self.client
            .invalidate_cache("GET:/ai-store-manager/video-tasks");
        self.client.get("/ai-store-manager/video-tasks").await
    }

    pub async fn create_video_package(
        &self,
        req: &CreateVideoPackageRequest,
    ) -> Result<VideoPackageResponse, ApiError> {
        self.client
            .post("/ai-store-manager/video-packages", req)
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

    pub async fn list_graphic_history(&self) -> Result<Vec<GraphicMarketingHistoryItem>, ApiError> {
        self.client
            .invalidate_cache("GET:/ai-store-manager/graphic-history");
        self.client.get("/ai-store-manager/graphic-history").await
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
