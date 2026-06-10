use std::io::ErrorKind;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use base64::engine::general_purpose;
use base64::Engine as _;
use beebotos_agents::llm::Message as LLMMessage;
use chrono::Utc;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use tracing::warn;

use crate::config::BeeBotOSConfig;
use crate::error::GatewayError;
use crate::AppState;

const VIDEO_TASK_HISTORY_LIMIT: i64 = 20;
const VIDEO_ASSET_DIR: &str = "data/ai-video-marketing/videos";
const VIDEO_ASSET_ROUTE: &str = "/api/v1/ai-store-manager/video-assets";
const GRAPHIC_HISTORY_LIMIT: i64 = 20;
const GRAPHIC_ASSET_DIR: &str = "data/ai-graphic-marketing/images";
const GRAPHIC_ASSET_ROUTE: &str = "/api/v1/ai-store-manager/graphic-assets";
const MAX_VIDEO_REFERENCE_IMAGES: usize = 1;
const MAX_VIDEO_REFERENCE_IMAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReferenceImageRequest {
    pub mime_type: String,
    pub data_url: String,
    #[serde(default)]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateVideoTaskRequest {
    pub product: String,
    pub platform: String,
    #[serde(default)]
    pub version: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
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
    #[serde(default)]
    pub reference_images: Vec<ReferenceImageRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoTaskResponse {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub message: String,
    pub preview_url: Option<String>,
    #[serde(default)]
    pub local_video_deleted: bool,
    pub resolution: Option<String>,
    pub ratio: Option<String>,
    pub duration_seconds: Option<u8>,
    pub queue_position: Option<u32>,
    pub submitted_at: Option<String>,
    pub updated_at: Option<String>,
    pub reference_image_count: Option<u8>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<VolcengineVideoImageUrlInput>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct VolcengineVideoImageUrlInput {
    url: String,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineApiErrorEnvelope {
    error: Option<VolcengineApiError>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineApiError {
    code: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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
    #[serde(default)]
    pub reference_images: Vec<ReferenceImageRequest>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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

fn build_seedance_video_payload(
    model: &str,
    req: &CreateVideoTaskRequest,
) -> VolcengineVideoTaskRequest {
    let selected_model = video_task_model(model, req);
    let image_role = seedance_reference_image_role(&selected_model);
    let mut content: Vec<VolcengineVideoContentInput> = req
        .reference_images
        .iter()
        .map(|image| VolcengineVideoContentInput {
            kind: "image_url".to_string(),
            role: Some(image_role.to_string()),
            text: None,
            image_url: Some(VolcengineVideoImageUrlInput {
                url: image.data_url.trim().to_string(),
            }),
        })
        .collect();
    content.push(VolcengineVideoContentInput {
        kind: "text".to_string(),
        role: None,
        text: Some(seedance_prompt_with_reference_guard(req)),
        image_url: None,
    });

    VolcengineVideoTaskRequest {
        model: selected_model,
        content,
        resolution: req.resolution.trim().to_string(),
        ratio: req.ratio.trim().to_string(),
        duration: req.duration_seconds,
        generate_audio: req.generate_audio,
        watermark: req.watermark,
    }
}

fn seedance_reference_image_role(model: &str) -> &'static str {
    if model.trim().starts_with("doubao-seedance-2.0") {
        "reference_image"
    } else {
        "first_frame"
    }
}

fn seedance_prompt_with_reference_guard(req: &CreateVideoTaskRequest) -> String {
    let prompt = req.prompt.trim();
    if req.reference_images.is_empty() {
        return prompt.to_string();
    }

    format!(
        "{prompt}\n参考图片约束：已随请求提供参考图片，必须以参考图片中的商品主体、包装、\
         颜色和外观为准生成视频；不要替换为同名但不同外观的商品，不要加入与参考图冲突的主体。"
    )
}

fn video_task_model(fallback_model: &str, req: &CreateVideoTaskRequest) -> String {
    req.model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(fallback_model)
        .to_string()
}

fn validate_video_task_request(req: &CreateVideoTaskRequest) -> Result<(), GatewayError> {
    if req.product.trim().is_empty() {
        return Err(GatewayError::bad_request("商品不能为空"));
    }
    if req.platform.trim().is_empty() {
        return Err(GatewayError::bad_request("平台不能为空"));
    }
    if req.prompt.trim().is_empty() {
        return Err(GatewayError::bad_request("视频提示词不能为空"));
    }
    if let Some(model) = req.model.as_deref() {
        let model = model.trim();
        if model.is_empty() || !model.starts_with("doubao-seedance-") {
            return Err(GatewayError::bad_request("不支持的视频模型"));
        }
    }
    if !(4..=15).contains(&req.duration_seconds) {
        return Err(GatewayError::bad_request("视频时长需在 4 到 15 秒之间"));
    }
    if !matches!(req.resolution.trim(), "480p" | "720p" | "1080p") {
        return Err(GatewayError::bad_request("不支持的视频分辨率"));
    }
    if !is_supported_video_ratio(req.ratio.trim()) {
        return Err(GatewayError::bad_request("不支持的视频比例"));
    }
    validate_reference_images(&req.reference_images)?;
    Ok(())
}

fn is_supported_video_ratio(ratio: &str) -> bool {
    matches!(
        ratio,
        "16:9" | "4:3" | "1:1" | "3:4" | "9:16" | "21:9" | "adaptive"
    )
}

fn validate_video_package_request(req: &CreateVideoPackageRequest) -> Result<(), GatewayError> {
    if req.product.trim().is_empty() {
        return Err(GatewayError::bad_request("商品不能为空"));
    }
    if req.selling_points.trim().is_empty() {
        return Err(GatewayError::bad_request("核心卖点不能为空"));
    }
    if req.audience.trim().is_empty() {
        return Err(GatewayError::bad_request("目标人群不能为空"));
    }
    if req.goal.trim().is_empty() {
        return Err(GatewayError::bad_request("营销目标不能为空"));
    }
    if req.platform.trim().is_empty() {
        return Err(GatewayError::bad_request("平台不能为空"));
    }
    if req.style.trim().is_empty() {
        return Err(GatewayError::bad_request("内容风格不能为空"));
    }
    if let Some(duration_seconds) = req.duration_seconds {
        if !(4..=15).contains(&duration_seconds) {
            return Err(GatewayError::bad_request("视频时长需在 4 到 15 秒之间"));
        }
    }
    if let Some(ratio) = req.ratio.as_deref() {
        let ratio = ratio.trim();
        if !ratio.is_empty() && !is_supported_video_ratio(ratio) {
            return Err(GatewayError::bad_request("不支持的视频比例"));
        }
    }
    validate_reference_images(&req.reference_images)?;
    Ok(())
}

fn validate_reference_images(images: &[ReferenceImageRequest]) -> Result<(), GatewayError> {
    if images.len() > MAX_VIDEO_REFERENCE_IMAGES {
        return Err(GatewayError::bad_request("第一版最多上传 1 张参考图片"));
    }
    for image in images {
        let mime_type = image.mime_type.trim();
        if !matches!(mime_type, "image/png" | "image/jpeg" | "image/webp") {
            return Err(GatewayError::bad_request(
                "参考图片仅支持 PNG、JPEG 或 WebP",
            ));
        }
        let data_url = image.data_url.trim();
        let prefix = format!("data:{};base64,", mime_type);
        let Some(encoded) = data_url.strip_prefix(&prefix) else {
            return Err(GatewayError::bad_request("参考图片 data URL 无效"));
        };
        let bytes = general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| GatewayError::bad_request("参考图片 data URL 无效"))?;
        if bytes.is_empty() {
            return Err(GatewayError::bad_request("参考图片不能为空"));
        }
        if bytes.len() > MAX_VIDEO_REFERENCE_IMAGE_BYTES {
            return Err(GatewayError::bad_request("参考图片不能超过 8MB"));
        }
    }
    Ok(())
}

async fn init_video_task_history_schema(db: &SqlitePool) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ai_video_marketing_tasks (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            product TEXT NOT NULL,
            platform TEXT NOT NULL,
            prompt TEXT NOT NULL,
            model TEXT NOT NULL,
            provider TEXT NOT NULL,
            status TEXT NOT NULL,
            message TEXT NOT NULL,
            preview_url TEXT,
            local_video_deleted INTEGER NOT NULL DEFAULT 0,
            resolution TEXT,
            ratio TEXT,
            duration_seconds INTEGER,
            generate_audio INTEGER NOT NULL DEFAULT 1,
            watermark INTEGER NOT NULL DEFAULT 0,
            queue_position INTEGER,
            submitted_at TEXT,
            updated_at TEXT,
            reference_image_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            last_synced_at TEXT NOT NULL
        )
        "#,
    )
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("初始化视频任务历史表失败: {}", err)))?;

    if let Err(err) = sqlx::query(
        "ALTER TABLE ai_video_marketing_tasks ADD COLUMN reference_image_count INTEGER NOT NULL \
         DEFAULT 0",
    )
    .execute(db)
    .await
    {
        if !err.to_string().contains("duplicate column name") {
            return Err(GatewayError::internal(format!(
                "初始化视频任务历史字段失败: {}",
                err
            )));
        }
    }

    if let Err(err) = sqlx::query(
        "ALTER TABLE ai_video_marketing_tasks ADD COLUMN local_video_deleted INTEGER NOT NULL \
         DEFAULT 0",
    )
    .execute(db)
    .await
    {
        if !err.to_string().contains("duplicate column name") {
            return Err(GatewayError::internal(format!(
                "初始化视频任务本地视频字段失败: {}",
                err
            )));
        }
    }

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_ai_video_marketing_tasks_user_updated
        ON ai_video_marketing_tasks(user_id, updated_at, created_at)
        "#,
    )
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("初始化视频任务历史索引失败: {}", err)))?;

    Ok(())
}

async fn insert_video_task_history(
    db: &SqlitePool,
    user_id: &str,
    req: &CreateVideoTaskRequest,
    task: &VideoTaskResponse,
) -> Result<(), GatewayError> {
    init_video_task_history_schema(db).await?;
    let now = Utc::now().to_rfc3339();
    let submitted_at = task.submitted_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = task
        .updated_at
        .clone()
        .unwrap_or_else(|| submitted_at.clone());
    let resolution = task
        .resolution
        .clone()
        .unwrap_or_else(|| req.resolution.trim().to_string());
    let ratio = task
        .ratio
        .clone()
        .unwrap_or_else(|| req.ratio.trim().to_string());
    let duration_seconds = task.duration_seconds.unwrap_or(req.duration_seconds);
    let queue_position = task.queue_position.map(|position| position as i64);
    let reference_image_count = task
        .reference_image_count
        .unwrap_or(req.reference_images.len() as u8) as i64;

    sqlx::query(
        r#"
        INSERT INTO ai_video_marketing_tasks (
            id, user_id, product, platform, prompt, model, provider, status, message,
            preview_url, local_video_deleted, resolution, ratio, duration_seconds, generate_audio,
            watermark, queue_position, submitted_at, updated_at, reference_image_count,
            created_at, last_synced_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
        ON CONFLICT(id) DO UPDATE SET
            user_id = excluded.user_id,
            product = excluded.product,
            platform = excluded.platform,
            prompt = excluded.prompt,
            model = excluded.model,
            provider = excluded.provider,
            status = excluded.status,
            message = excluded.message,
            preview_url = excluded.preview_url,
            local_video_deleted = excluded.local_video_deleted,
            resolution = excluded.resolution,
            ratio = excluded.ratio,
            duration_seconds = excluded.duration_seconds,
            generate_audio = excluded.generate_audio,
            watermark = excluded.watermark,
            queue_position = excluded.queue_position,
            submitted_at = excluded.submitted_at,
            updated_at = excluded.updated_at,
            reference_image_count = excluded.reference_image_count,
            last_synced_at = excluded.last_synced_at
        "#,
    )
    .bind(&task.id)
    .bind(user_id)
    .bind(req.product.trim())
    .bind(req.platform.trim())
    .bind(req.prompt.trim())
    .bind(&task.model)
    .bind(&task.provider)
    .bind(&task.status)
    .bind(&task.message)
    .bind(task.preview_url.as_deref())
    .bind(if task.local_video_deleted {
        1_i64
    } else {
        0_i64
    })
    .bind(resolution)
    .bind(ratio)
    .bind(duration_seconds as i64)
    .bind(if req.generate_audio { 1_i64 } else { 0_i64 })
    .bind(if req.watermark { 1_i64 } else { 0_i64 })
    .bind(queue_position)
    .bind(&submitted_at)
    .bind(&updated_at)
    .bind(reference_image_count)
    .bind(&submitted_at)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("保存视频任务历史失败: {}", err)))?;

    Ok(())
}

async fn update_video_task_history(
    db: &SqlitePool,
    user_id: &str,
    task: &VideoTaskResponse,
) -> Result<(), GatewayError> {
    init_video_task_history_schema(db).await?;
    let now = Utc::now().to_rfc3339();
    let updated_at = task.updated_at.clone().unwrap_or_else(|| now.clone());
    let submitted_at = task
        .submitted_at
        .clone()
        .unwrap_or_else(|| updated_at.clone());
    let duration_seconds = task.duration_seconds.map(|duration| duration as i64);
    let queue_position = task.queue_position.map(|position| position as i64);
    let reference_image_count = task.reference_image_count.unwrap_or(0) as i64;

    sqlx::query(
        r#"
        INSERT INTO ai_video_marketing_tasks (
            id, user_id, product, platform, prompt, model, provider, status, message,
            preview_url, local_video_deleted, resolution, ratio, duration_seconds, generate_audio,
            watermark, queue_position, submitted_at, updated_at, reference_image_count,
            created_at, last_synced_at
        )
        VALUES (?1, ?2, '', '', '', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, 0, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(id) DO UPDATE SET
            user_id = excluded.user_id,
            model = excluded.model,
            provider = excluded.provider,
            status = excluded.status,
            message = excluded.message,
            preview_url = CASE
                WHEN ai_video_marketing_tasks.local_video_deleted = 1 AND excluded.local_video_deleted = 0 THEN NULL
                ELSE excluded.preview_url
            END,
            local_video_deleted = CASE
                WHEN ai_video_marketing_tasks.local_video_deleted = 1 AND excluded.local_video_deleted = 0 THEN 1
                ELSE excluded.local_video_deleted
            END,
            resolution = COALESCE(excluded.resolution, ai_video_marketing_tasks.resolution),
            ratio = COALESCE(excluded.ratio, ai_video_marketing_tasks.ratio),
            duration_seconds = COALESCE(excluded.duration_seconds, ai_video_marketing_tasks.duration_seconds),
            queue_position = excluded.queue_position,
            submitted_at = COALESCE(ai_video_marketing_tasks.submitted_at, excluded.submitted_at),
            updated_at = excluded.updated_at,
            reference_image_count = CASE
                WHEN excluded.reference_image_count > 0 THEN excluded.reference_image_count
                ELSE ai_video_marketing_tasks.reference_image_count
            END,
            last_synced_at = excluded.last_synced_at
        "#,
    )
    .bind(&task.id)
    .bind(user_id)
    .bind(&task.model)
    .bind(&task.provider)
    .bind(&task.status)
    .bind(&task.message)
    .bind(task.preview_url.as_deref())
    .bind(if task.local_video_deleted {
        1_i64
    } else {
        0_i64
    })
    .bind(task.resolution.as_deref())
    .bind(task.ratio.as_deref())
    .bind(duration_seconds)
    .bind(queue_position)
    .bind(&submitted_at)
    .bind(&updated_at)
    .bind(reference_image_count)
    .bind(&submitted_at)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("更新视频任务历史失败: {}", err)))?;

    Ok(())
}

async fn list_video_task_history(
    db: &SqlitePool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<VideoTaskResponse>, GatewayError> {
    init_video_task_history_schema(db).await?;
    let rows = sqlx::query(
        r#"
        SELECT id, provider, model, status, message, preview_url, resolution, ratio,
               duration_seconds, queue_position, submitted_at, updated_at,
               reference_image_count, local_video_deleted
        FROM ai_video_marketing_tasks
        WHERE user_id = ?1
        ORDER BY COALESCE(updated_at, created_at) DESC, created_at DESC
        LIMIT ?2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(|err| GatewayError::internal(format!("读取视频任务历史失败: {}", err)))?;

    Ok(rows
        .into_iter()
        .map(|row| VideoTaskResponse {
            id: row.get("id"),
            provider: row.get("provider"),
            model: row.get("model"),
            status: row.get("status"),
            message: row.get("message"),
            preview_url: row.get("preview_url"),
            local_video_deleted: row.get::<i64, _>("local_video_deleted") != 0,
            resolution: row.get("resolution"),
            ratio: row.get("ratio"),
            duration_seconds: row
                .get::<Option<i64>, _>("duration_seconds")
                .map(|duration| duration as u8),
            queue_position: row
                .get::<Option<i64>, _>("queue_position")
                .map(|position| position as u32),
            submitted_at: row.get("submitted_at"),
            updated_at: row.get("updated_at"),
            reference_image_count: row
                .get::<Option<i64>, _>("reference_image_count")
                .map(|count| count as u8),
        })
        .collect())
}

async fn get_video_task_history(
    db: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<Option<VideoTaskResponse>, GatewayError> {
    init_video_task_history_schema(db).await?;
    let mut tasks = sqlx::query(
        r#"
        SELECT id, provider, model, status, message, preview_url, resolution, ratio,
               duration_seconds, queue_position, submitted_at, updated_at,
               reference_image_count, local_video_deleted
        FROM ai_video_marketing_tasks
        WHERE user_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(id)
    .fetch_all(db)
    .await
    .map_err(|err| GatewayError::internal(format!("读取视频任务历史失败: {}", err)))?
    .into_iter()
    .map(|row| VideoTaskResponse {
        id: row.get("id"),
        provider: row.get("provider"),
        model: row.get("model"),
        status: row.get("status"),
        message: row.get("message"),
        preview_url: row.get("preview_url"),
        local_video_deleted: row.get::<i64, _>("local_video_deleted") != 0,
        resolution: row.get("resolution"),
        ratio: row.get("ratio"),
        duration_seconds: row
            .get::<Option<i64>, _>("duration_seconds")
            .map(|duration| duration as u8),
        queue_position: row
            .get::<Option<i64>, _>("queue_position")
            .map(|position| position as u32),
        submitted_at: row.get("submitted_at"),
        updated_at: row.get("updated_at"),
        reference_image_count: row
            .get::<Option<i64>, _>("reference_image_count")
            .map(|count| count as u8),
    })
    .collect::<Vec<_>>();

    Ok(tasks.pop())
}

async fn remove_video_task_history(
    db: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<(), GatewayError> {
    init_video_task_history_schema(db).await?;
    sqlx::query("DELETE FROM ai_video_marketing_tasks WHERE user_id = ?1 AND id = ?2")
        .bind(user_id)
        .bind(id)
        .execute(db)
        .await
        .map_err(|err| GatewayError::internal(format!("移出视频任务队列失败: {}", err)))?;
    Ok(())
}

async fn set_video_task_local_video_deleted(
    db: &SqlitePool,
    user_id: &str,
    id: &str,
    deleted: bool,
    preview_url: Option<&str>,
) -> Result<(), GatewayError> {
    init_video_task_history_schema(db).await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE ai_video_marketing_tasks
        SET local_video_deleted = ?3,
            preview_url = ?4,
            message = CASE
                WHEN ?3 = 1 THEN '本地视频已删除。'
                WHEN status = 'completed' AND message = '本地视频已删除。' THEN 'Seedance 视频已生成。'
                ELSE message
            END,
            updated_at = ?5,
            last_synced_at = ?5
        WHERE user_id = ?1 AND id = ?2
        "#,
    )
    .bind(user_id)
    .bind(id)
    .bind(if deleted { 1_i64 } else { 0_i64 })
    .bind(preview_url)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("更新本地视频状态失败: {}", err)))?;
    Ok(())
}

fn new_graphic_record_id() -> String {
    format!("graphic-record-{}", uuid::Uuid::new_v4())
}

fn new_graphic_image_id() -> String {
    format!("graphic-image-{}", uuid::Uuid::new_v4())
}

async fn init_graphic_marketing_history_schema(db: &SqlitePool) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ai_graphic_marketing_records (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            product TEXT NOT NULL,
            selling_points TEXT NOT NULL DEFAULT '',
            audience TEXT NOT NULL DEFAULT '',
            price_range TEXT NOT NULL DEFAULT '',
            platform TEXT NOT NULL,
            goal TEXT NOT NULL DEFAULT '',
            style TEXT NOT NULL DEFAULT '',
            size TEXT,
            quality TEXT,
            package_json TEXT,
            image_id TEXT,
            image_provider TEXT,
            image_status TEXT,
            image_message TEXT,
            image_url TEXT,
            image_prompt TEXT,
            source_image_filename TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("初始化图文营销历史表失败: {}", err)))?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_ai_graphic_marketing_records_user_updated
        ON ai_graphic_marketing_records(user_id, updated_at, created_at)
        "#,
    )
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("初始化图文营销历史索引失败: {}", err)))?;

    Ok(())
}

async fn insert_graphic_package_history(
    db: &SqlitePool,
    user_id: &str,
    req: &CreateGraphicPackageRequest,
    mut package: GraphicPackageResponse,
) -> Result<GraphicPackageResponse, GatewayError> {
    init_graphic_marketing_history_schema(db).await?;
    let record_id = package
        .history_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(new_graphic_record_id);
    package.history_id = Some(record_id.clone());
    let now = Utc::now().to_rfc3339();
    let package_json = serde_json::to_string(&package)
        .map_err(|err| GatewayError::internal(format!("图文营销包序列化失败: {}", err)))?;

    sqlx::query(
        r#"
        INSERT INTO ai_graphic_marketing_records (
            id, user_id, product, selling_points, audience, price_range, platform,
            goal, style, package_json, image_prompt, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(id) DO UPDATE SET
            product = excluded.product,
            selling_points = excluded.selling_points,
            audience = excluded.audience,
            price_range = excluded.price_range,
            platform = excluded.platform,
            goal = excluded.goal,
            style = excluded.style,
            package_json = excluded.package_json,
            image_prompt = excluded.image_prompt,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&record_id)
    .bind(user_id)
    .bind(req.product.trim())
    .bind(req.selling_points.trim())
    .bind(req.audience.trim())
    .bind(req.price_range.trim())
    .bind(req.platform.trim())
    .bind(req.goal.trim())
    .bind(req.style.trim())
    .bind(package_json)
    .bind(package.image_prompt.trim())
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("保存图文营销历史失败: {}", err)))?;

    Ok(package)
}

async fn upsert_graphic_image_history(
    db: &SqlitePool,
    user_id: &str,
    req: &CreateGraphicImageRequest,
    source_image_filename: Option<&str>,
    image: GraphicImageResponse,
) -> Result<GraphicImageResponse, GatewayError> {
    init_graphic_marketing_history_schema(db).await?;
    let now = Utc::now().to_rfc3339();
    let requested_record_id = req
        .package_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());

    if let Some(record_id) = requested_record_id {
        let result = sqlx::query(
            r#"
            UPDATE ai_graphic_marketing_records
            SET product = ?3,
                platform = ?4,
                size = ?5,
                quality = ?6,
                image_id = ?7,
                image_provider = ?8,
                image_status = ?9,
                image_message = ?10,
                image_url = ?11,
                image_prompt = ?12,
                source_image_filename = ?13,
                updated_at = ?14
            WHERE id = ?1 AND user_id = ?2
            "#,
        )
        .bind(record_id)
        .bind(user_id)
        .bind(req.product.trim())
        .bind(req.platform.trim())
        .bind(req.size.trim())
        .bind(req.quality.trim())
        .bind(&image.id)
        .bind(&image.provider)
        .bind(&image.status)
        .bind(&image.message)
        .bind(image.image_url.as_deref())
        .bind(req.prompt.trim())
        .bind(source_image_filename)
        .bind(&now)
        .execute(db)
        .await
        .map_err(|err| GatewayError::internal(format!("更新图文营销图片历史失败: {}", err)))?;

        if result.rows_affected() > 0 {
            return Ok(image);
        }
    }

    let record_id = new_graphic_record_id();
    sqlx::query(
        r#"
        INSERT INTO ai_graphic_marketing_records (
            id, user_id, product, platform, size, quality, image_id, image_provider,
            image_status, image_message, image_url, image_prompt, source_image_filename,
            created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
    )
    .bind(&record_id)
    .bind(user_id)
    .bind(req.product.trim())
    .bind(req.platform.trim())
    .bind(req.size.trim())
    .bind(req.quality.trim())
    .bind(&image.id)
    .bind(&image.provider)
    .bind(&image.status)
    .bind(&image.message)
    .bind(image.image_url.as_deref())
    .bind(req.prompt.trim())
    .bind(source_image_filename)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("保存图文营销图片历史失败: {}", err)))?;

    Ok(image)
}

async fn list_graphic_marketing_history(
    db: &SqlitePool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<GraphicMarketingHistoryItem>, GatewayError> {
    init_graphic_marketing_history_schema(db).await?;
    let rows = sqlx::query(
        r#"
        SELECT id, product, platform, package_json, image_id, image_provider, image_status,
               image_message, image_url, image_prompt, size, quality, source_image_filename,
               created_at, updated_at
        FROM ai_graphic_marketing_records
        WHERE user_id = ?1
        ORDER BY COALESCE(updated_at, created_at) DESC, created_at DESC
        LIMIT ?2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(|err| GatewayError::internal(format!("读取图文营销历史失败: {}", err)))?;

    rows.into_iter()
        .map(|row| {
            let id: String = row.get("id");
            let package = row
                .get::<Option<String>, _>("package_json")
                .map(|raw| {
                    let mut package: GraphicPackageResponse =
                        serde_json::from_str(&raw).map_err(|err| {
                            GatewayError::internal(format!("图文营销历史 JSON 解析失败: {}", err))
                        })?;
                    package.history_id = Some(id.clone());
                    Ok::<GraphicPackageResponse, GatewayError>(package)
                })
                .transpose()?;
            let image_id: Option<String> = row.get("image_id");
            let image = image_id.map(|image_id| GraphicImageResponse {
                id: image_id,
                provider: row
                    .get::<Option<String>, _>("image_provider")
                    .unwrap_or_else(|| "openai-compatible-image".to_string()),
                status: row
                    .get::<Option<String>, _>("image_status")
                    .unwrap_or_else(|| "completed".to_string()),
                message: row
                    .get::<Option<String>, _>("image_message")
                    .unwrap_or_else(|| "图片已生成。".to_string()),
                image_url: row.get("image_url"),
                b64_json: None,
            });

            Ok(GraphicMarketingHistoryItem {
                id,
                product: row.get("product"),
                platform: row.get("platform"),
                package,
                image,
                image_prompt: row.get("image_prompt"),
                size: row.get("size"),
                quality: row.get("quality"),
                source_image_filename: row.get("source_image_filename"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
        })
        .collect()
}

fn merge_video_task_history_refresh(
    mut refreshed: VideoTaskResponse,
    previous: &VideoTaskResponse,
) -> VideoTaskResponse {
    if previous.local_video_deleted {
        refreshed.local_video_deleted = true;
        refreshed.preview_url = None;
    } else if refreshed.preview_url.is_none() {
        refreshed.preview_url = previous.preview_url.clone();
    }
    if refreshed.resolution.is_none() {
        refreshed.resolution = previous.resolution.clone();
    }
    if refreshed.ratio.is_none() {
        refreshed.ratio = previous.ratio.clone();
    }
    if refreshed.duration_seconds.is_none() {
        refreshed.duration_seconds = previous.duration_seconds;
    }
    if refreshed.queue_position.is_none() {
        refreshed.queue_position = previous.queue_position;
    }
    if refreshed.reference_image_count.is_none() {
        refreshed.reference_image_count = previous.reference_image_count;
    }
    if refreshed.submitted_at.is_none() {
        refreshed.submitted_at = previous.submitted_at.clone();
    }
    refreshed
}

fn video_asset_file_name(task_id: &str) -> String {
    let safe_id = task_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}.mp4", safe_id)
}

fn video_asset_url(task_id: &str) -> String {
    format!("{}/{}", VIDEO_ASSET_ROUTE, video_asset_file_name(task_id))
}

fn video_asset_path(root: &FsPath, task_id: &str) -> PathBuf {
    root.join(video_asset_file_name(task_id))
}

fn default_video_asset_root() -> PathBuf {
    PathBuf::from(VIDEO_ASSET_DIR)
}

fn graphic_asset_file_name(image_id: &str) -> String {
    let safe_id = image_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}.png", safe_id)
}

fn graphic_asset_url(image_id: &str) -> String {
    format!(
        "{}/{}",
        GRAPHIC_ASSET_ROUTE,
        graphic_asset_file_name(image_id)
    )
}

fn graphic_asset_path(root: &FsPath, image_id: &str) -> PathBuf {
    root.join(graphic_asset_file_name(image_id))
}

fn default_graphic_asset_root() -> PathBuf {
    PathBuf::from(GRAPHIC_ASSET_DIR)
}

fn clear_uncached_remote_video_preview(mut task: VideoTaskResponse) -> VideoTaskResponse {
    if task
        .preview_url
        .as_deref()
        .is_some_and(|url| url.starts_with("http"))
    {
        task.preview_url = None;
    }
    task
}

async fn cache_video_task_preview(
    root: &FsPath,
    client: &reqwest::Client,
    mut task: VideoTaskResponse,
) -> VideoTaskResponse {
    if task.status != "completed" {
        return task;
    }
    if task.local_video_deleted {
        task.preview_url = None;
        return task;
    }

    let local_url = video_asset_url(&task.id);
    if task.preview_url.as_deref() == Some(local_url.as_str()) {
        return task;
    }

    let path = video_asset_path(root, &task.id);
    if path.exists() {
        task.preview_url = Some(local_url);
        return task;
    }

    let Some(remote_url) = task
        .preview_url
        .clone()
        .filter(|url| url.starts_with("http"))
    else {
        return task;
    };

    let response = match client.get(&remote_url).send().await {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            warn!(
                task_id = task.id,
                status = %response.status(),
                "video asset cache skipped"
            );
            return clear_uncached_remote_video_preview(task);
        }
        Err(err) => {
            warn!(
                task_id = task.id,
                error = %err,
                "video asset cache failed"
            );
            return clear_uncached_remote_video_preview(task);
        }
    };

    let bytes = match response.bytes().await {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return clear_uncached_remote_video_preview(task),
        Err(err) => {
            warn!(
                task_id = task.id,
                error = %err,
                "video asset cache read failed"
            );
            return clear_uncached_remote_video_preview(task);
        }
    };

    if let Err(err) = tokio::fs::create_dir_all(root).await {
        warn!(
            task_id = task.id,
            error = %err,
            "video asset cache directory creation failed"
        );
        return clear_uncached_remote_video_preview(task);
    }
    if let Err(err) = tokio::fs::write(&path, bytes).await {
        warn!(
            task_id = task.id,
            error = %err,
            "video asset cache write failed"
        );
        return clear_uncached_remote_video_preview(task);
    }

    task.preview_url = Some(local_url);
    task
}

async fn cache_graphic_image_asset(
    root: &FsPath,
    client: &reqwest::Client,
    mut image: GraphicImageResponse,
) -> GraphicImageResponse {
    let local_url = graphic_asset_url(&image.id);
    if image.image_url.as_deref() == Some(local_url.as_str()) {
        image.b64_json = None;
        return image;
    }

    let path = graphic_asset_path(root, &image.id);
    if path.exists() {
        image.image_url = Some(local_url);
        image.b64_json = None;
        return image;
    }

    let bytes = if let Some(b64) = image.b64_json.as_deref() {
        match general_purpose::STANDARD.decode(b64.trim()) {
            Ok(bytes) if !bytes.is_empty() => Some(bytes),
            Ok(_) => None,
            Err(err) => {
                warn!(image_id = image.id, error = %err, "graphic image base64 decode failed");
                None
            }
        }
    } else if let Some(remote_url) = image
        .image_url
        .clone()
        .filter(|url| url.starts_with("http"))
    {
        match client.get(&remote_url).send().await {
            Ok(response) if response.status().is_success() => match response.bytes().await {
                Ok(bytes) if !bytes.is_empty() => Some(bytes.to_vec()),
                Ok(_) => None,
                Err(err) => {
                    warn!(image_id = image.id, error = %err, "graphic image cache read failed");
                    None
                }
            },
            Ok(response) => {
                warn!(
                    image_id = image.id,
                    status = %response.status(),
                    "graphic image cache skipped"
                );
                None
            }
            Err(err) => {
                warn!(image_id = image.id, error = %err, "graphic image cache failed");
                None
            }
        }
    } else {
        None
    };

    let Some(bytes) = bytes else {
        return image;
    };

    if let Err(err) = tokio::fs::create_dir_all(root).await {
        warn!(image_id = image.id, error = %err, "graphic image cache directory creation failed");
        return image;
    }
    if let Err(err) = tokio::fs::write(&path, bytes).await {
        warn!(image_id = image.id, error = %err, "graphic image cache write failed");
        return image;
    }

    image.image_url = Some(local_url);
    image.b64_json = None;
    image
}

async fn refresh_video_task_history(
    db: &SqlitePool,
    user_id: &str,
    history: Vec<VideoTaskResponse>,
    client: &reqwest::Client,
    config: &BeeBotOSConfig,
    api_key: &str,
) -> Result<Vec<VideoTaskResponse>, GatewayError> {
    let model = config.video_generation.model.clone();
    let mut refreshed_history = Vec::with_capacity(history.len());

    for previous in history {
        let response = match client
            .get(video_task_status_url(
                &config.video_generation.base_url,
                &previous.id,
            ))
            .bearer_auth(api_key)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                warn!(
                    task_id = previous.id,
                    status = %response.status(),
                    "video task history refresh skipped"
                );
                refreshed_history.push(previous);
                continue;
            }
            Err(err) => {
                warn!(
                    task_id = previous.id,
                    error = %err,
                    "video task history refresh failed"
                );
                refreshed_history.push(previous);
                continue;
            }
        };

        let upstream = match response.json::<VolcengineVideoTaskStatusResponse>().await {
            Ok(upstream) => upstream,
            Err(err) => {
                warn!(
                    task_id = previous.id,
                    error = %err,
                    "video task history refresh response parse failed"
                );
                refreshed_history.push(previous);
                continue;
            }
        };

        let task =
            merge_video_task_history_refresh(video_status_to_response(upstream, &model), &previous);
        let task = cache_video_task_preview(&default_video_asset_root(), client, task).await;
        update_video_task_history(db, user_id, &task).await?;
        refreshed_history.push(task);
    }

    Ok(refreshed_history)
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

fn should_refresh_video_task_history(history: &[VideoTaskResponse]) -> bool {
    !history.is_empty()
}

async fn fetch_video_task_from_upstream(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    id: &str,
    model: &str,
) -> Result<VideoTaskResponse, GatewayError> {
    let response = client
        .get(video_task_status_url(base_url, id))
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务查询失败: {}", err)))?;

    let status = response.status();
    if !status.is_success() {
        let upstream_error = response
            .json::<VolcengineApiErrorEnvelope>()
            .await
            .ok()
            .and_then(|envelope| envelope.error);
        let mut fallback =
            video_upstream_unavailable_response(model, status, upstream_error.as_ref());
        fallback.id = id.to_string();
        return Ok(fallback);
    }

    let upstream = response
        .json::<VolcengineVideoTaskStatusResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务响应解析失败: {}", err)))?;
    Ok(video_status_to_response(upstream, model))
}

fn video_generation_config_required_response(model: &str) -> VideoTaskResponse {
    let now = Utc::now().to_rfc3339();
    VideoTaskResponse {
        id: "seedance2-config-required".to_string(),
        provider: "volcengine-ark".to_string(),
        model: model.to_string(),
        status: "blocked".to_string(),
        message: "火山方舟视频生成未配置 VIDEO_GENERATION_API_KEY；开通 Seedance \
                  并配置后即可提交真实任务。"
            .to_string(),
        preview_url: None,
        local_video_deleted: false,
        resolution: None,
        ratio: None,
        duration_seconds: None,
        queue_position: None,
        submitted_at: Some(now.clone()),
        updated_at: Some(now),
        reference_image_count: Some(0),
    }
}

fn video_upstream_unavailable_response(
    model: &str,
    status: reqwest::StatusCode,
    error: Option<&VolcengineApiError>,
) -> VideoTaskResponse {
    let blocked =
        matches!(status.as_u16(), 401 | 402 | 403) || status == reqwest::StatusCode::NOT_FOUND;
    let message = video_upstream_error_message(model, status, blocked, error);
    let now = Utc::now().to_rfc3339();
    VideoTaskResponse {
        id: "seedance2-upstream-unavailable".to_string(),
        provider: "volcengine-ark".to_string(),
        model: model.to_string(),
        status: if blocked { "blocked" } else { "failed" }.to_string(),
        message,
        preview_url: None,
        local_video_deleted: false,
        resolution: None,
        ratio: None,
        duration_seconds: None,
        queue_position: None,
        submitted_at: Some(now.clone()),
        updated_at: Some(now),
        reference_image_count: Some(0),
    }
}

fn video_upstream_error_message(
    model: &str,
    status: reqwest::StatusCode,
    blocked: bool,
    error: Option<&VolcengineApiError>,
) -> String {
    let code = error.and_then(|error| error.code.as_deref()).unwrap_or("");
    if code == "ModelNotOpen" {
        return format!(
            "火山方舟模型未开通: {}。请在 Ark 控制台开通该模型服务后重试。",
            model
        );
    }

    if let Some(message) = error
        .and_then(|error| error.message.as_deref())
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return if code.is_empty() {
            format!("火山方舟返回错误: {}", message)
        } else {
            format!("火山方舟返回错误 {}: {}", code, message)
        };
    }

    if blocked {
        format!("火山方舟鉴权、余额或模型订阅未就绪: HTTP {}", status)
    } else {
        format!("火山方舟视频任务提交失败: HTTP {}", status)
    }
}

fn video_create_to_response(
    created: VolcengineVideoTaskCreateResponse,
    model: &str,
    req: &CreateVideoTaskRequest,
) -> VideoTaskResponse {
    let selected_model = video_task_model(model, req);
    let now = Utc::now().to_rfc3339();
    VideoTaskResponse {
        id: created.id,
        provider: "volcengine-ark".to_string(),
        model: selected_model,
        status: "queued".to_string(),
        message: "Seedance 视频任务已提交，正在排队生成。".to_string(),
        preview_url: None,
        local_video_deleted: false,
        resolution: Some(req.resolution.trim().to_string()),
        ratio: Some(req.ratio.trim().to_string()),
        duration_seconds: Some(req.duration_seconds),
        queue_position: Some(1),
        submitted_at: Some(now.clone()),
        updated_at: Some(now),
        reference_image_count: Some(req.reference_images.len() as u8),
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
            "completed" => "Seedance 视频已生成。".to_string(),
            "queued" => "Seedance 视频任务排队中。".to_string(),
            "running" => "Seedance 视频生成中。".to_string(),
            "expired" => "Seedance 视频任务已超时。".to_string(),
            "cancelled" => "Seedance 视频任务已取消。".to_string(),
            "failed" => "Seedance 视频任务失败。".to_string(),
            _ => "Seedance 视频任务状态已更新。".to_string(),
        });

    VideoTaskResponse {
        id: upstream.id,
        provider: "volcengine-ark".to_string(),
        model: upstream.model.unwrap_or_else(|| fallback_model.to_string()),
        status: status.to_string(),
        message,
        preview_url,
        local_video_deleted: false,
        resolution: None,
        ratio: None,
        duration_seconds: None,
        queue_position: None,
        submitted_at: None,
        updated_at: Some(Utc::now().to_rfc3339()),
        reference_image_count: None,
    }
}

fn video_cancelled_response(mut task: VideoTaskResponse) -> VideoTaskResponse {
    task.status = "cancelled".to_string();
    task.message = "Seedance 视频任务已取消。".to_string();
    task.preview_url = None;
    task.queue_position = None;
    task.updated_at = Some(Utc::now().to_rfc3339());
    task
}

fn video_cancel_not_supported_response(mut task: VideoTaskResponse) -> VideoTaskResponse {
    task.message = "任务已进入生成中，火山方舟不支持强制取消。".to_string();
    task.updated_at = Some(Utc::now().to_rfc3339());
    task
}

pub fn create_graphic_package(req: &CreateGraphicPackageRequest) -> GraphicPackageResponse {
    let platform_prefix = if req.platform == "朋友圈" {
        "朋友圈私域"
    } else {
        "小红书种草"
    };

    GraphicPackageResponse {
        history_id: None,
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

fn package_duration_seconds(req: &CreateVideoPackageRequest) -> u8 {
    req.duration_seconds
        .unwrap_or(default_video_task_duration())
}

fn package_ratio(req: &CreateVideoPackageRequest) -> &str {
    req.ratio
        .as_deref()
        .map(str::trim)
        .filter(|ratio| !ratio.is_empty())
        .unwrap_or("9:16")
}

fn package_voiceover_limit(duration_seconds: u8) -> usize {
    match duration_seconds {
        0..=5 => 28,
        6..=8 => 42,
        9..=12 => 64,
        _ => 80,
    }
}

fn package_scene_limit(duration_seconds: u8) -> usize {
    match duration_seconds {
        0..=5 => 2,
        6..=8 => 3,
        _ => 4,
    }
}

fn reference_image_prompt_summary(images: &[ReferenceImageRequest]) -> String {
    if images.is_empty() {
        return "无".to_string();
    }
    let names = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            image
                .file_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("参考图{}", index + 1))
        })
        .collect::<Vec<_>>()
        .join("、");
    format!("{} 张（{}）", images.len(), names)
}

fn image_understanding_chat_url(base_url: &str) -> String {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{base_url}/chat/completions")
    }
}

fn ark_vision_response_content(raw: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    value
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .map(str::to_string)
}

fn image_understanding_error(message: impl AsRef<str>) -> GatewayError {
    let message = message.as_ref();
    warn!(error_message = %message, "image understanding failed");
    let user_message = if image_understanding_auth_error(message) {
        "参考图片理解鉴权失败：请确认图片理解的 API Key、base URL、模型名称和模型权限匹配；Agent Plan key 需要使用 \
         https://ark.cn-beijing.volces.com/api/plan/v3 和支持 image 输入的模型。"
    } else {
        "参考图片理解失败：请检查图片理解模型配置、API Key 或模型订阅后重试。"
    };
    GatewayError::service_unavailable("ImageUnderstanding", user_message)
}

fn image_understanding_auth_error(message: &str) -> bool {
    message.contains("401")
        || message.contains("Unauthorized")
        || message.contains("AuthenticationError")
}

async fn describe_reference_images_for_video_package(
    state: &AppState,
    req: &CreateVideoPackageRequest,
) -> Result<Option<String>, GatewayError> {
    let Some(image) = req.reference_images.first() else {
        return Ok(None);
    };
    let config = &state.config.image_understanding;
    let api_key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| {
            GatewayError::service_unavailable(
                "ImageUnderstanding",
                "已上传参考图片，但图片理解模型未配置 API Key。请配置 IMAGE_UNDERSTANDING_API_KEY \
                 或 ARK_API_KEY；如果使用 Agent Plan，请确认图片理解 base URL 是 \
                 https://ark.cn-beijing.volces.com/api/plan/v3。",
            )
        })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_seconds))
        .build()
        .map_err(|err| image_understanding_error(format!("创建图片理解客户端失败: {err}")))?;
    let prompt = format!(
        "请识别这张营销参考图，只输出中文图片描述，重点覆盖商品外观、包装文字、颜色、材质、场景、\
         构图、光线、风格和适合短视频使用的视觉卖点。商品名：{}。核心卖点：{}。",
        req.product.trim(),
        req.selling_points.trim()
    );
    let body = json!({
        "model": config.model.trim(),
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": image.data_url.trim(),
                        "detail": "auto"
                    }
                }
            ]
        }],
        "max_tokens": 500,
        "temperature": 0.2
    });

    let response = client
        .post(image_understanding_chat_url(&config.base_url))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|err| image_understanding_error(format!("图片理解请求失败: {err}")))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|err| image_understanding_error(format!("读取图片理解响应失败: {err}")))?;
    if !status.is_success() {
        return Err(image_understanding_error(format!(
            "图片理解上游失败: status={status}, body={}",
            raw.chars().take(300).collect::<String>()
        )));
    }
    let content = ark_vision_response_content(&raw)
        .ok_or_else(|| image_understanding_error("图片理解响应缺少 message.content"))?;
    Ok(Some(content.chars().take(1200).collect()))
}

#[cfg(test)]
fn build_video_package_prompt(req: &CreateVideoPackageRequest) -> String {
    build_video_package_prompt_with_reference_context(req, None)
}

fn build_video_package_prompt_with_reference_context(
    req: &CreateVideoPackageRequest,
    reference_image_context: Option<&str>,
) -> String {
    let duration_seconds = package_duration_seconds(req);
    let ratio = package_ratio(req);
    let voiceover_limit = package_voiceover_limit(duration_seconds);
    let scene_limit = package_scene_limit(duration_seconds);
    let reference_images = reference_image_prompt_summary(&req.reference_images);
    let reference_image_context = reference_image_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("无");
    let audio_requirement = if req.generate_audio.unwrap_or(true) {
        format!(
            "`oral_script` 必须是可直接用于 {duration_seconds} 秒成片的短口播，最多 \
             {voiceover_limit} 个中文字符；不要写完整长稿或多段口播。"
        )
    } else {
        "`oral_script` 写成无口播音频的字幕节奏说明，不要生成口播台词。".to_string()
    };

    format!(
        r##"请作为 BeeBotOS AI 店长，为短视频模型生成一套可直接使用的视频营销脚本包。

任务信息：
- 商品：{product}
- 核心卖点：{selling_points}
- 目标人群：{audience}
- 发布平台：{platform}
- 营销目标：{goal}
- 内容风格：{style}
- 计划成片：{duration_seconds} 秒，{ratio} 画幅
- 参考图片：{reference_images}
- 图片理解：{reference_image_context}

要求：
1. 结果必须贴合商品、平台、人群、目标和风格，不要输出通用模板。
2. {audio_requirement}
3. `storyboard`、`subtitles`、`shot_prompts` 每项 2 到 {scene_limit} 条，镜头数量必须适合 {duration_seconds} 秒。
4. `video_prompt` 会直接传给视频生成模型，必须是一段紧凑成片提示词，包含商品主体、镜头顺序、画幅、节奏、字幕/口播约束。
5. 如果有参考图片，必须以“图片理解”中的真实视觉信息为准，不要编造与图片冲突的外观。
6. 只输出 JSON，不要 Markdown，不要解释。

JSON 结构：
{{
  "title": "短视频标题",
  "hook": "3 秒钩子",
  "oral_script": "口播脚本",
  "storyboard": ["分镜一", "分镜二"],
  "subtitles": ["字幕一", "字幕二"],
  "shot_prompts": ["镜头提示一", "镜头提示二"],
  "tags": ["#话题一", "#话题二"],
  "video_prompt": "给视频模型的完整中文提示词",
  "checks": [
    {{"label": "商品卖点完整", "status": "已覆盖"}},
    {{"label": "平台风格匹配", "status": "已覆盖"}},
    {{"label": "人工审核", "status": "待确认"}}
  ]
}}"##,
        product = req.product,
        selling_points = req.selling_points,
        audience = req.audience,
        platform = req.platform,
        goal = req.goal,
        style = req.style,
        duration_seconds = duration_seconds,
        ratio = ratio,
        reference_images = reference_images,
        reference_image_context = reference_image_context,
        audio_requirement = audio_requirement,
        scene_limit = scene_limit
    )
}

fn validate_video_package(package: &VideoPackageResponse) -> Result<(), GatewayError> {
    if package.title.trim().is_empty()
        || package.hook.trim().is_empty()
        || package.oral_script.trim().is_empty()
        || package.video_prompt.trim().is_empty()
    {
        return Err(GatewayError::internal("视频脚本包内容不完整"));
    }
    if package.storyboard.is_empty()
        || package.subtitles.is_empty()
        || package.shot_prompts.is_empty()
        || package.tags.is_empty()
    {
        return Err(GatewayError::internal("视频脚本包结构不完整"));
    }
    Ok(())
}

fn video_package_from_agent_response(
    raw: &str,
    _req: &CreateVideoPackageRequest,
    agent_id: &str,
) -> Result<VideoPackageResponse, GatewayError> {
    let json = extract_json_object(raw)?;
    let mut package: VideoPackageResponse = serde_json::from_str(json)
        .map_err(|err| GatewayError::internal(format!("视频脚本包 JSON 解析失败: {}", err)))?;

    if package.checks.is_empty() {
        package.checks = vec![
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
        ];
    }
    package.agent_id = Some(agent_id.to_string());
    validate_video_package(&package)?;
    Ok(package)
}

fn video_package_from_llm_response(
    raw: &str,
    req: &CreateVideoPackageRequest,
) -> Result<VideoPackageResponse, GatewayError> {
    let mut package = video_package_from_agent_response(raw, req, "llm-service")?;
    package.agent_id = None;
    Ok(package)
}

fn video_package_llm_error(message: impl AsRef<str>) -> GatewayError {
    let message = message.as_ref().trim();
    let normalized = message.to_ascii_lowercase();
    let user_message = if normalized.contains("invalid api key")
        || normalized.contains("authentication error")
        || normalized.contains("unauthorized")
        || normalized.contains("401")
    {
        "AI 脚本包生成失败：大模型 API Key 无效，请检查默认模型配置后重试。"
    } else if normalized.contains("timed out") || normalized.contains("timeout") {
        "AI 脚本包生成失败：大模型响应超时，请稍后重试或切换更快的默认模型。"
    } else if normalized.contains("all providers failed")
        || normalized.contains("llm request failed")
        || normalized.contains("unavailable")
    {
        "AI 脚本包生成失败：大模型服务暂时不可用，请检查默认模型配置后重试。"
    } else {
        "AI 脚本包生成失败：大模型暂时不可用，请稍后重试。"
    };
    warn!(error_message = %message, "video package llm failed");
    GatewayError::agent(user_message)
}

async fn generate_video_package_with_llm(
    state: &AppState,
    req: &CreateVideoPackageRequest,
) -> Result<VideoPackageResponse, GatewayError> {
    let reference_image_context = describe_reference_images_for_video_package(state, req).await?;
    let prompt =
        build_video_package_prompt_with_reference_context(req, reference_image_context.as_deref());
    let messages = vec![
        LLMMessage::system(
            "你是资深短视频营销编导，只返回符合用户 JSON schema 的中文短视频脚本包。",
        ),
        LLMMessage::user(prompt),
    ];
    let response = state
        .llm_service
        .chat(messages, Some(1800), None, Some("none".to_string()), None)
        .await
        .map_err(|err| video_package_llm_error(err.to_string()))?;

    video_package_from_llm_response(&response, req)
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
    #[serde(default)]
    pub package_id: Option<String>,
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
    #[serde(default)]
    pub package_id: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct SeedreamImageRequest {
    model: String,
    prompt: String,
    size: String,
    output_format: String,
    watermark: bool,
    sequential_image_generation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct SeedreamImageResponse {
    data: Vec<SeedreamImageData>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct SeedreamImageData {
    b64_json: Option<String>,
    url: Option<String>,
}

fn build_image_generation_payload(
    model: &str,
    req: &CreateGraphicImageRequest,
) -> SeedreamImageRequest {
    build_image_generation_payload_with_image(model, req, None, None)
}

fn build_image_generation_payload_with_image(
    model: &str,
    req: &CreateGraphicImageRequest,
    image_mime_type: Option<&str>,
    image_b64: Option<&str>,
) -> SeedreamImageRequest {
    let image = image_mime_type
        .zip(image_b64)
        .map(|(mime_type, b64)| vec![graphic_image_data_uri(mime_type, b64)]);

    SeedreamImageRequest {
        model: model.to_string(),
        prompt: seedream_prompt(req),
        size: seedream_quality(&req.quality).to_string(),
        output_format: "png".to_string(),
        watermark: false,
        sequential_image_generation: "disabled".to_string(),
        image,
    }
}

fn seedream_prompt(req: &CreateGraphicImageRequest) -> String {
    format!(
        "{}\n画幅比例：{}。",
        req.prompt.trim(),
        seedream_aspect_ratio(&req.size)
    )
}

fn seedream_aspect_ratio(size: &str) -> &'static str {
    match size.trim() {
        "1024x1536" => "3:4",
        "1536x1024" => "3:2",
        _ => "1:1",
    }
}

fn seedream_quality(quality: &str) -> &'static str {
    match quality.trim() {
        "high" => "4K",
        _ => "2K",
    }
}

fn graphic_image_data_uri(mime_type: &str, b64: &str) -> String {
    format!("data:{};base64,{}", mime_type.trim(), b64.trim())
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
        package_id: req.package_id.clone(),
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
    _product: &str,
    _platform: &str,
    response: SeedreamImageResponse,
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
        id: new_graphic_image_id(),
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
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateVideoTaskRequest>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    validate_video_task_request(&req)?;

    let config = BeeBotOSConfig::load()
        .map_err(|err| GatewayError::internal(format!("加载视频生成配置失败: {}", err)))?;
    let model = config.video_generation.model.clone();
    let selected_model = video_task_model(&model, &req);
    let api_key = match config
        .video_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
    {
        Some(api_key) => api_key,
        None => {
            return Ok(Json(video_generation_config_required_response(
                &selected_model,
            )))
        }
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
        let upstream_error = response
            .json::<VolcengineApiErrorEnvelope>()
            .await
            .ok()
            .and_then(|envelope| envelope.error);
        return Ok(Json(video_upstream_unavailable_response(
            &selected_model,
            status,
            upstream_error.as_ref(),
        )));
    }

    let created = response
        .json::<VolcengineVideoTaskCreateResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("视频生成响应解析失败: {}", err)))?;
    let task = video_create_to_response(created, &model, &req);
    insert_video_task_history(&state.db, &user.user_id, &req, &task).await?;

    Ok(Json(task))
}

pub async fn get_video_asset(Path(file): Path<String>) -> Result<impl IntoResponse, GatewayError> {
    let Some(task_id) = file.strip_suffix(".mp4") else {
        return Err(GatewayError::not_found("video asset", file));
    };
    if file != video_asset_file_name(task_id) {
        return Err(GatewayError::not_found("video asset", file));
    }

    let path = default_video_asset_root().join(&file);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| GatewayError::not_found("video asset", file))?;

    Ok(([(header::CONTENT_TYPE, "video/mp4")], bytes))
}

pub async fn get_graphic_asset(
    Path(file): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    let Some(image_id) = file.strip_suffix(".png") else {
        return Err(GatewayError::not_found("graphic asset", file));
    };
    if file != graphic_asset_file_name(image_id) {
        return Err(GatewayError::not_found("graphic asset", file));
    }

    let path = default_graphic_asset_root().join(&file);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| GatewayError::not_found("graphic asset", file))?;

    Ok(([(header::CONTENT_TYPE, "image/png")], bytes))
}

pub async fn create_video_package_handler(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateVideoPackageRequest>,
) -> Result<Json<VideoPackageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    validate_video_package_request(&req)?;
    let package = generate_video_package_with_llm(&state, &req).await?;
    Ok(Json(package))
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
    let package = insert_graphic_package_history(&state.db, &user.user_id, &req, package).await?;
    Ok(Json(package))
}

pub async fn create_graphic_image(
    State(state): State<Arc<AppState>>,
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
        .json::<SeedreamImageResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("图片生成响应解析失败: {}", err)))?;
    let result = image_response_to_graphic_result(&req.product, &req.platform, image_response)?;
    let result = cache_graphic_image_asset(&default_graphic_asset_root(), &client, result).await;
    let result = upsert_graphic_image_history(&state.db, &user.user_id, &req, None, result).await?;

    Ok(Json(result))
}

pub async fn create_graphic_image_edit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateGraphicImageEditRequest>,
) -> Result<Json<GraphicImageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    validate_graphic_image_edit_request(&req)?;
    let _ = decode_graphic_image_upload(&req)?;

    let config = BeeBotOSConfig::load()
        .map_err(|err| GatewayError::internal(format!("加载图片生成配置失败: {}", err)))?;
    let api_key = config
        .image_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| GatewayError::bad_request("图片生成未配置"))?;

    let payload = build_image_generation_payload_with_image(
        &config.image_generation.model,
        &CreateGraphicImageRequest {
            product: req.product.clone(),
            platform: req.platform.clone(),
            prompt: req.prompt.clone(),
            size: req.size.clone(),
            quality: req.quality.clone(),
            package_id: req.package_id.clone(),
        },
        Some(&req.image_mime_type),
        Some(&req.image_b64),
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.image_generation.timeout_seconds,
        ))
        .build()
        .map_err(|err| GatewayError::internal(format!("图片编辑客户端创建失败: {}", err)))?;

    let response = client
        .post(image_generation_url(&config.image_generation.base_url))
        .bearer_auth(api_key)
        .json(&payload)
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
        .json::<SeedreamImageResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("图片编辑响应解析失败: {}", err)))?;
    let result = image_response_to_graphic_result(&req.product, &req.platform, image_response)?;
    let result = cache_graphic_image_asset(&default_graphic_asset_root(), &client, result).await;
    let image_req = CreateGraphicImageRequest {
        product: req.product.clone(),
        platform: req.platform.clone(),
        prompt: req.prompt.clone(),
        size: req.size.clone(),
        quality: req.quality.clone(),
        package_id: req.package_id.clone(),
    };
    let result = upsert_graphic_image_history(
        &state.db,
        &user.user_id,
        &image_req,
        Some(req.image_filename.trim()),
        result,
    )
    .await?;

    Ok(Json(result))
}

pub async fn list_graphic_marketing_history_handler(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<Vec<GraphicMarketingHistoryItem>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let history =
        list_graphic_marketing_history(&state.db, &user.user_id, GRAPHIC_HISTORY_LIMIT).await?;
    Ok(Json(history))
}

pub async fn list_video_tasks(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<Vec<VideoTaskResponse>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let history =
        list_video_task_history(&state.db, &user.user_id, VIDEO_TASK_HISTORY_LIMIT).await?;
    if !should_refresh_video_task_history(&history) {
        return Ok(Json(history));
    }

    let config = match BeeBotOSConfig::load() {
        Ok(config) => config,
        Err(err) => {
            warn!(error = %err, "video task history returned without refresh");
            return Ok(Json(history));
        }
    };
    let api_key = match config
        .video_generation
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
    {
        Some(api_key) => api_key,
        None => return Ok(Json(history)),
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.video_generation.timeout_seconds,
        ))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            warn!(error = %err, "video task history returned without refresh client");
            return Ok(Json(history));
        }
    };
    let history = refresh_video_task_history(
        &state.db,
        &user.user_id,
        history,
        &client,
        &config,
        &api_key,
    )
    .await?;

    Ok(Json(history))
}

pub async fn get_video_task(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let previous = get_video_task_history(&state.db, &user.user_id, &id).await?;
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
            if let Some(task) = previous {
                return Ok(Json(task));
            }
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
        let upstream_error = response
            .json::<VolcengineApiErrorEnvelope>()
            .await
            .ok()
            .and_then(|envelope| envelope.error);
        let mut fallback =
            video_upstream_unavailable_response(&model, status, upstream_error.as_ref());
        fallback.id = id;
        update_video_task_history(&state.db, &user.user_id, &fallback).await?;
        return Ok(Json(fallback));
    }

    let upstream = response
        .json::<VolcengineVideoTaskStatusResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务响应解析失败: {}", err)))?;
    let task = if let Some(previous) = previous {
        merge_video_task_history_refresh(video_status_to_response(upstream, &model), &previous)
    } else {
        video_status_to_response(upstream, &model)
    };
    let task = cache_video_task_preview(&default_video_asset_root(), &client, task).await;
    update_video_task_history(&state.db, &user.user_id, &task).await?;

    Ok(Json(task))
}

pub async fn remove_video_task(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let task = get_video_task_history(&state.db, &user.user_id, &id)
        .await?
        .ok_or_else(|| GatewayError::not_found("video task", &id))?;
    remove_video_task_history(&state.db, &user.user_id, &id).await?;
    Ok(Json(task))
}

pub async fn cancel_video_task(
    State(state): State<Arc<AppState>>,
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

    let current = fetch_video_task_from_upstream(
        &client,
        &config.video_generation.base_url,
        &api_key,
        &id,
        &model,
    )
    .await?;
    if current.status != "queued" {
        let task = if current.status == "running" {
            video_cancel_not_supported_response(current)
        } else {
            current
        };
        update_video_task_history(&state.db, &user.user_id, &task).await?;
        return Ok(Json(task));
    }

    let response = client
        .delete(video_task_status_url(
            &config.video_generation.base_url,
            &id,
        ))
        .bearer_auth(&api_key)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务取消失败: {}", err)))?;

    if response.status().is_success() {
        let task = video_cancelled_response(current);
        update_video_task_history(&state.db, &user.user_id, &task).await?;
        return Ok(Json(task));
    }

    let refreshed = fetch_video_task_from_upstream(
        &client,
        &config.video_generation.base_url,
        &api_key,
        &id,
        &model,
    )
    .await?;
    let task = if refreshed.status == "running" {
        video_cancel_not_supported_response(refreshed)
    } else {
        refreshed
    };
    update_video_task_history(&state.db, &user.user_id, &task).await?;
    Ok(Json(task))
}

pub async fn delete_video_task_local_video(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let mut task = get_video_task_history(&state.db, &user.user_id, &id)
        .await?
        .ok_or_else(|| GatewayError::not_found("video task", &id))?;
    if task.status != "completed" {
        return Err(GatewayError::bad_request("只能删除已生成视频的本地缓存"));
    }

    let path = video_asset_path(&default_video_asset_root(), &id);
    if let Err(err) = tokio::fs::remove_file(&path).await {
        if err.kind() != ErrorKind::NotFound {
            return Err(GatewayError::internal(format!("删除本地视频失败: {}", err)));
        }
    }

    set_video_task_local_video_deleted(&state.db, &user.user_id, &id, true, None).await?;
    task.preview_url = None;
    task.local_video_deleted = true;
    task.message = "本地视频已删除。".to_string();
    task.updated_at = Some(Utc::now().to_rfc3339());
    Ok(Json(task))
}

pub async fn restore_video_task_local_video(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<VideoTaskResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let previous = get_video_task_history(&state.db, &user.user_id, &id)
        .await?
        .ok_or_else(|| GatewayError::not_found("video task", &id))?;
    if previous.status != "completed" {
        return Err(GatewayError::bad_request("只能恢复已生成视频的本地缓存"));
    }

    let local_url = video_asset_url(&id);
    if video_asset_path(&default_video_asset_root(), &id).exists() {
        set_video_task_local_video_deleted(
            &state.db,
            &user.user_id,
            &id,
            false,
            Some(local_url.as_str()),
        )
        .await?;
        let mut task = previous;
        task.local_video_deleted = false;
        task.preview_url = Some(local_url);
        task.updated_at = Some(Utc::now().to_rfc3339());
        return Ok(Json(task));
    }

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

    let mut previous_for_restore = previous;
    previous_for_restore.local_video_deleted = false;
    let task = fetch_video_task_from_upstream(
        &client,
        &config.video_generation.base_url,
        &api_key,
        &id,
        &model,
    )
    .await?;
    let mut task = merge_video_task_history_refresh(task, &previous_for_restore);
    task.local_video_deleted = false;
    let task = cache_video_task_preview(&default_video_asset_root(), &client, task).await;
    set_video_task_local_video_deleted(
        &state.db,
        &user.user_id,
        &id,
        false,
        task.preview_url.as_deref(),
    )
    .await?;
    update_video_task_history(&state.db, &user.user_id, &task).await?;
    Ok(Json(task))
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
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    #[test]
    fn seedance_payload_uses_text_content_and_generation_options() {
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: None,
            prompt: "为云柑礼盒生成短视频".to_string(),
            model: Some("doubao-seedance-lite-test".to_string()),
            duration_seconds: 8,
            resolution: "1080p".to_string(),
            ratio: "16:9".to_string(),
            generate_audio: false,
            watermark: true,
            reference_images: vec![],
        };

        let payload = build_seedance_video_payload("doubao-seedance-2.0", &req);

        assert_eq!(payload.model, "doubao-seedance-lite-test");
        assert_eq!(payload.content[0].kind, "text");
        assert_eq!(
            payload.content[0].text.as_deref(),
            Some("为云柑礼盒生成短视频")
        );
        assert_eq!(payload.duration, 8);
        assert_eq!(payload.resolution, "1080p");
        assert_eq!(payload.ratio, "16:9");
        assert!(!payload.generate_audio);
        assert!(payload.watermark);
    }

    #[test]
    fn seedance_payload_marks_uploaded_image_as_seedance_reference() {
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: None,
            prompt: "按商品图生成开箱短视频".to_string(),
            model: Some("doubao-seedance-2.0".to_string()),
            duration_seconds: 8,
            resolution: "720p".to_string(),
            ratio: "9:16".to_string(),
            generate_audio: true,
            watermark: false,
            reference_images: vec![sample_reference_image()],
        };

        let payload = build_seedance_video_payload("doubao-seedance-2.0", &req);

        assert_eq!(payload.content.len(), 2);
        assert_eq!(payload.content[0].kind, "image_url");
        let payload_json = serde_json::to_value(&payload).unwrap();
        assert_eq!(payload_json["content"][0]["role"], "reference_image");
        assert_eq!(
            payload.content[0]
                .image_url
                .as_ref()
                .map(|image| image.url.as_str()),
            Some("data:image/png;base64,aGVsbG8=")
        );
        assert_eq!(payload.content[1].kind, "text");
        let text = payload.content[1].text.as_deref().unwrap_or_default();
        assert!(text.contains("按商品图生成开箱短视频"));
        assert!(text.contains("必须以参考图片中的商品主体、包装、颜色和外观为准"));
    }

    #[test]
    fn seedance_payload_uses_first_frame_role_for_legacy_image_video_model() {
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: None,
            prompt: "按商品图生成开箱短视频".to_string(),
            model: Some("doubao-seedance-1.5-pro".to_string()),
            duration_seconds: 8,
            resolution: "720p".to_string(),
            ratio: "9:16".to_string(),
            generate_audio: true,
            watermark: false,
            reference_images: vec![sample_reference_image()],
        };

        let payload = build_seedance_video_payload("doubao-seedance-2.0", &req);
        let payload_json = serde_json::to_value(&payload).unwrap();

        assert_eq!(payload_json["content"][0]["role"], "first_frame");
    }

    #[test]
    fn video_task_request_rejects_invalid_reference_image_mime() {
        let mut req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: None,
            prompt: "按商品图生成开箱短视频".to_string(),
            model: Some("doubao-seedance-2.0".to_string()),
            duration_seconds: 8,
            resolution: "720p".to_string(),
            ratio: "9:16".to_string(),
            generate_audio: true,
            watermark: false,
            reference_images: vec![sample_reference_image()],
        };
        req.reference_images[0].mime_type = "image/gif".to_string();

        let err = validate_video_task_request(&req).unwrap_err();

        assert_eq!(err.user_message(), "参考图片仅支持 PNG、JPEG 或 WebP");
    }

    #[test]
    fn video_task_url_uses_agent_plan_generation_endpoint() {
        assert_eq!(
            video_task_url("https://ark.cn-beijing.volces.com/api/plan/v3"),
            "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks"
        );
    }

    #[test]
    fn seedance_status_response_maps_succeeded_to_completed_preview() {
        let upstream = VolcengineVideoTaskStatusResponse {
            id: "task-1".to_string(),
            model: Some("doubao-seedance-2.0".to_string()),
            status: "succeeded".to_string(),
            content: Some(VolcengineVideoTaskContent {
                video_url: Some("https://cdn.example/video.mp4".to_string()),
            }),
            error: None,
        };

        let response = video_status_to_response(upstream, "fallback-model");

        assert_eq!(response.id, "task-1");
        assert_eq!(response.provider, "volcengine-ark");
        assert_eq!(response.model, "doubao-seedance-2.0");
        assert_eq!(response.status, "completed");
        assert_eq!(
            response.preview_url.as_deref(),
            Some("https://cdn.example/video.mp4")
        );
    }

    #[test]
    fn seedance_config_required_response_is_blocked() {
        let response = video_generation_config_required_response("doubao-seedance-2.0");

        assert_eq!(response.provider, "volcengine-ark");
        assert_eq!(response.model, "doubao-seedance-2.0");
        assert_eq!(response.status, "blocked");
        assert!(response.message.contains("VIDEO_GENERATION_API_KEY"));
        assert!(response.preview_url.is_none());
    }

    #[test]
    fn seedance_model_not_open_response_is_actionable() {
        let error = VolcengineApiError {
            code: Some("ModelNotOpen".to_string()),
            message: Some("account has not activated the model".to_string()),
        };

        let response = video_upstream_unavailable_response(
            "doubao-seedance-2.0",
            reqwest::StatusCode::NOT_FOUND,
            Some(&error),
        );

        assert_eq!(response.status, "blocked");
        assert!(response.message.contains("模型未开通"));
        assert!(response.message.contains("doubao-seedance-2.0"));
    }

    #[test]
    fn video_cancelled_response_preserves_task_metadata() {
        let queued = sample_video_task_response("task-1", "queued");

        let cancelled = video_cancelled_response(queued);

        assert_eq!(cancelled.status, "cancelled");
        assert!(cancelled.message.contains("已取消"));
        assert_eq!(cancelled.resolution.as_deref(), Some("720p"));
        assert_eq!(cancelled.ratio.as_deref(), Some("9:16"));
        assert_eq!(cancelled.duration_seconds, Some(12));
    }

    #[test]
    fn running_video_cancel_response_stays_running() {
        let running = sample_video_task_response("task-1", "running");

        let response = video_cancel_not_supported_response(running);

        assert_eq!(response.status, "running");
        assert!(response.message.contains("不支持强制取消"));
    }

    #[tokio::test]
    async fn video_task_history_persists_and_lists_user_tasks() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_video_task_history_schema(&db).await.unwrap();
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: None,
            prompt: "生成云柑礼盒视频".to_string(),
            model: Some("doubao-seedance-2.0".to_string()),
            duration_seconds: 12,
            resolution: "720p".to_string(),
            ratio: "9:16".to_string(),
            generate_audio: true,
            watermark: false,
            reference_images: vec![],
        };
        let mut first = sample_video_task_response("task-old", "queued");
        first.submitted_at = Some("2026-06-03T08:00:00Z".to_string());
        first.updated_at = Some("2026-06-03T08:00:00Z".to_string());
        let mut second = sample_video_task_response("task-new", "completed");
        second.preview_url = Some("https://cdn.example/new.mp4".to_string());
        second.submitted_at = Some("2026-06-03T09:00:00Z".to_string());
        second.updated_at = Some("2026-06-03T09:05:00Z".to_string());

        insert_video_task_history(&db, "user-1", &req, &first)
            .await
            .unwrap();
        insert_video_task_history(&db, "user-1", &req, &second)
            .await
            .unwrap();
        insert_video_task_history(
            &db,
            "user-2",
            &req,
            &sample_video_task_response("task-other", "queued"),
        )
        .await
        .unwrap();

        let tasks = list_video_task_history(&db, "user-1", 20).await.unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "task-new");
        assert_eq!(
            tasks[0].preview_url.as_deref(),
            Some("https://cdn.example/new.mp4")
        );
        assert_eq!(tasks[1].id, "task-old");
    }

    #[tokio::test]
    async fn video_task_history_updates_polled_status() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_video_task_history_schema(&db).await.unwrap();
        let req = CreateVideoTaskRequest {
            product: "云柑礼盒".to_string(),
            platform: "抖音".to_string(),
            version: None,
            prompt: "生成云柑礼盒视频".to_string(),
            model: Some("doubao-seedance-2.0".to_string()),
            duration_seconds: 12,
            resolution: "720p".to_string(),
            ratio: "9:16".to_string(),
            generate_audio: true,
            watermark: false,
            reference_images: vec![],
        };
        insert_video_task_history(
            &db,
            "user-1",
            &req,
            &sample_video_task_response("task-1", "running"),
        )
        .await
        .unwrap();

        let mut completed = sample_video_task_response("task-1", "completed");
        completed.preview_url = Some("https://cdn.example/video.mp4".to_string());
        completed.updated_at = Some("2026-06-03T09:10:00Z".to_string());
        update_video_task_history(&db, "user-1", &completed)
            .await
            .unwrap();

        let tasks = list_video_task_history(&db, "user-1", 20).await.unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, "completed");
        assert_eq!(
            tasks[0].preview_url.as_deref(),
            Some("https://cdn.example/video.mp4")
        );
        assert_eq!(tasks[0].updated_at.as_deref(), Some("2026-06-03T09:10:00Z"));
    }

    #[tokio::test]
    async fn video_task_history_records_polled_task_without_create_request() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_video_task_history_schema(&db).await.unwrap();

        let mut completed = sample_video_task_response("task-from-upstream", "completed");
        completed.preview_url = Some("https://cdn.example/upstream.mp4".to_string());
        completed.updated_at = Some("2026-06-03T09:30:00Z".to_string());
        update_video_task_history(&db, "user-1", &completed)
            .await
            .unwrap();

        let tasks = list_video_task_history(&db, "user-1", 20).await.unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-from-upstream");
        assert_eq!(tasks[0].status, "completed");
        assert_eq!(
            tasks[0].preview_url.as_deref(),
            Some("https://cdn.example/upstream.mp4")
        );
    }

    #[tokio::test]
    async fn removing_video_task_history_forgets_only_current_user() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_video_task_history_schema(&db).await.unwrap();

        let task = sample_video_task_response("task-1", "completed");
        update_video_task_history(&db, "user-1", &task)
            .await
            .unwrap();
        update_video_task_history(&db, "user-2", &task)
            .await
            .unwrap();

        remove_video_task_history(&db, "user-1", "task-1")
            .await
            .unwrap();

        assert!(list_video_task_history(&db, "user-1", 20)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            list_video_task_history(&db, "user-2", 20)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn restoring_video_task_local_video_state_clears_deleted_message() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_video_task_history_schema(&db).await.unwrap();

        let task = sample_video_task_response("task-1", "completed");
        update_video_task_history(&db, "user-1", &task)
            .await
            .unwrap();
        set_video_task_local_video_deleted(&db, "user-1", "task-1", true, None)
            .await
            .unwrap();
        set_video_task_local_video_deleted(
            &db,
            "user-1",
            "task-1",
            false,
            Some("/api/v1/ai-store-manager/video-assets/task-1.mp4"),
        )
        .await
        .unwrap();

        let restored = get_video_task_history(&db, "user-1", "task-1")
            .await
            .unwrap()
            .unwrap();
        assert!(!restored.local_video_deleted);
        assert_eq!(restored.message, "Seedance 视频已生成。");
        assert_eq!(
            restored.preview_url.as_deref(),
            Some("/api/v1/ai-store-manager/video-assets/task-1.mp4")
        );
    }

    #[test]
    fn empty_video_task_history_does_not_import_global_upstream_tasks() {
        let history = Vec::new();

        assert!(!should_refresh_video_task_history(&history));
    }

    #[test]
    fn refreshed_video_task_history_keeps_parameters_and_updates_preview_url() {
        let mut previous = sample_video_task_response("task-1", "completed");
        previous.preview_url = Some("https://cdn.example/expired.mp4".to_string());
        previous.resolution = Some("720p".to_string());
        previous.ratio = Some("9:16".to_string());
        previous.duration_seconds = Some(12);
        previous.submitted_at = Some("2026-06-03T08:00:00Z".to_string());

        let mut refreshed = sample_video_task_response("task-1", "completed");
        refreshed.preview_url = Some("https://cdn.example/fresh.mp4".to_string());
        refreshed.resolution = None;
        refreshed.ratio = None;
        refreshed.duration_seconds = None;
        refreshed.submitted_at = None;
        refreshed.updated_at = Some("2026-06-04T09:10:00Z".to_string());

        let merged = merge_video_task_history_refresh(refreshed, &previous);

        assert_eq!(
            merged.preview_url.as_deref(),
            Some("https://cdn.example/fresh.mp4")
        );
        assert_eq!(merged.resolution.as_deref(), Some("720p"));
        assert_eq!(merged.ratio.as_deref(), Some("9:16"));
        assert_eq!(merged.duration_seconds, Some(12));
        assert_eq!(merged.submitted_at.as_deref(), Some("2026-06-03T08:00:00Z"));
    }

    #[test]
    fn refreshed_video_task_history_preserves_local_video_deleted_state() {
        let mut previous = sample_video_task_response("task-1", "completed");
        previous.preview_url = None;
        previous.local_video_deleted = true;

        let mut refreshed = sample_video_task_response("task-1", "completed");
        refreshed.preview_url = Some("https://cdn.example/fresh.mp4".to_string());

        let merged = merge_video_task_history_refresh(refreshed, &previous);

        assert!(merged.local_video_deleted);
        assert!(merged.preview_url.is_none());
    }

    #[test]
    fn video_asset_url_uses_sanitized_mp4_file_name() {
        assert_eq!(video_asset_file_name("task/../one"), "task____one.mp4");
        assert_eq!(
            video_asset_url("task/../one"),
            "/api/v1/ai-store-manager/video-assets/task____one.mp4"
        );
    }

    #[test]
    fn uncached_remote_video_preview_is_not_returned_as_playable() {
        let mut task = sample_video_task_response("task-1", "completed");
        task.preview_url = Some("https://cdn.example/expired.mp4".to_string());

        let task = clear_uncached_remote_video_preview(task);

        assert!(task.preview_url.is_none());
    }

    #[test]
    fn video_package_prompt_uses_generation_duration_budget() {
        let req = CreateVideoPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            goal: "新品种草".to_string(),
            platform: "抖音".to_string(),
            style: "真实测评".to_string(),
            duration_seconds: Some(5),
            ratio: Some("9:16".to_string()),
            generate_audio: Some(true),
            reference_images: vec![],
        };

        let prompt = build_video_package_prompt(&req);

        assert!(prompt.contains("计划成片：5 秒，9:16 画幅"));
        assert!(prompt.contains("最多 28 个中文字符"));
        assert!(prompt.contains("每项 2 到 2 条"));
    }

    #[test]
    fn video_package_prompt_uses_reference_image_context() {
        let req = CreateVideoPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            goal: "新品种草".to_string(),
            platform: "抖音".to_string(),
            style: "真实测评".to_string(),
            duration_seconds: Some(8),
            ratio: Some("9:16".to_string()),
            generate_audio: Some(true),
            reference_images: vec![sample_reference_image()],
        };

        let prompt = build_video_package_prompt(&req);

        assert!(prompt.contains("参考图片：1 张"));
        assert!(prompt.contains("product.png"));
        assert!(prompt.contains("以“图片理解”中的真实视觉信息为准"));
    }

    #[test]
    fn video_package_prompt_includes_reference_image_description() {
        let req = CreateVideoPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            goal: "新品种草".to_string(),
            platform: "抖音".to_string(),
            style: "真实测评".to_string(),
            duration_seconds: Some(8),
            ratio: Some("9:16".to_string()),
            generate_audio: Some(true),
            reference_images: vec![sample_reference_image()],
        };

        let prompt = build_video_package_prompt_with_reference_context(
            &req,
            Some("橙色礼盒包装，桌面自然光，适合送礼场景。"),
        );

        assert!(prompt.contains("图片理解：橙色礼盒包装，桌面自然光，适合送礼场景。"));
        assert!(prompt.contains("以“图片理解”中的真实视觉信息为准"));
    }

    #[test]
    fn image_understanding_chat_url_appends_chat_completions() {
        assert_eq!(
            image_understanding_chat_url("https://ark.cn-beijing.volces.com/api/v3"),
            "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
        );
        assert_eq!(
            image_understanding_chat_url(
                "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
            ),
            "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
        );
    }

    #[test]
    fn ark_vision_response_content_extracts_message_text() {
        let raw = r#"{
            "choices": [
                {"message": {"content": "橙色礼盒包装，自然光桌面。"}}
            ]
        }"#;

        assert_eq!(
            ark_vision_response_content(raw).as_deref(),
            Some("橙色礼盒包装，自然光桌面。")
        );
    }

    #[test]
    fn image_understanding_auth_error_is_actionable() {
        let err = image_understanding_error(
            "图片理解上游失败: status=401 Unauthorized, \
             body={\"error\":{\"code\":\"AuthenticationError\"}}",
        );

        assert_eq!(
            err.user_message(),
            "Service unavailable: 参考图片理解鉴权失败：请确认图片理解的 API Key、base URL、模型名称和模型权限匹配；Agent Plan key 需要使用 \
             https://ark.cn-beijing.volces.com/api/plan/v3 和支持 image 输入的模型。"
        );
    }

    #[test]
    fn video_package_from_agent_response_accepts_structured_json() {
        let req = CreateVideoPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            goal: "新品种草".to_string(),
            platform: "抖音".to_string(),
            style: "真实测评".to_string(),
            duration_seconds: Some(12),
            ratio: Some("9:16".to_string()),
            generate_audio: Some(true),
            reference_images: vec![],
        };
        let raw = r##"```json
{
  "title": "云柑礼盒真实开箱",
  "hook": "办公室下午茶被它承包了",
  "oral_script": "这盒云柑礼盒适合送礼和办公室分享。",
  "storyboard": ["开箱", "果肉特写", "同事试吃"],
  "subtitles": ["当季鲜果", "顺丰冷链"],
  "shot_prompts": ["真实办公室桌面，礼盒开箱", "自然光下果肉特写"],
  "tags": ["#云柑礼盒", "#抖音好物"],
  "video_prompt": "真实办公室场景，云柑礼盒开箱，果肉特写，同事试吃反馈。",
  "checks": [
    {"label": "商品卖点完整", "status": "已覆盖"},
    {"label": "人工审核", "status": "待确认"}
  ]
}
```"##;

        let package = video_package_from_agent_response(raw, &req, "agent-1").unwrap();

        assert_eq!(package.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(package.title, "云柑礼盒真实开箱");
        assert_eq!(package.storyboard.len(), 3);
        assert_eq!(package.subtitles.len(), 2);
        assert!(package.video_prompt.contains("云柑礼盒"));
        assert!(package.shot_prompts[0].contains("办公室"));
    }

    #[test]
    fn video_package_from_llm_response_has_no_agent_id() {
        let req = CreateVideoPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            goal: "新品种草".to_string(),
            platform: "抖音".to_string(),
            style: "真实测评".to_string(),
            duration_seconds: Some(12),
            ratio: Some("9:16".to_string()),
            generate_audio: Some(true),
            reference_images: vec![],
        };
        let raw = r##"{
  "title": "标题",
  "hook": "钩子",
  "oral_script": "口播",
  "storyboard": ["分镜"],
  "subtitles": ["字幕"],
  "shot_prompts": ["镜头"],
  "tags": ["#标签"],
  "video_prompt": "视频提示词",
  "checks": []
}"##;

        let package = video_package_from_llm_response(raw, &req).unwrap();

        assert_eq!(package.video_prompt, "视频提示词");
        assert!(package.agent_id.is_none());
    }

    #[test]
    fn video_package_from_agent_response_rejects_missing_video_prompt() {
        let req = CreateVideoPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            goal: "新品种草".to_string(),
            platform: "抖音".to_string(),
            style: "真实测评".to_string(),
            duration_seconds: Some(12),
            ratio: Some("9:16".to_string()),
            generate_audio: Some(true),
            reference_images: vec![],
        };
        let raw = r##"{
  "title": "标题",
  "hook": "钩子",
  "oral_script": "口播",
  "storyboard": ["分镜"],
  "subtitles": ["字幕"],
  "shot_prompts": ["镜头"],
  "tags": ["#标签"],
  "checks": []
}"##;

        assert!(video_package_from_agent_response(raw, &req, "agent-1").is_err());
    }

    #[test]
    fn video_package_llm_auth_failure_is_actionable() {
        let err = video_package_llm_error(
            "Execution error: LLM request failed: Provider error: All providers failed or are \
             unavailable: Authentication error: Invalid API key",
        );

        assert_eq!(
            err.user_message(),
            "AI 脚本包生成失败：大模型 API Key 无效，请检查默认模型配置后重试。"
        );
    }

    fn sample_video_task_response(id: &str, status: &str) -> VideoTaskResponse {
        VideoTaskResponse {
            id: id.to_string(),
            provider: "volcengine-ark".to_string(),
            model: "doubao-seedance-2.0".to_string(),
            status: status.to_string(),
            message: status.to_string(),
            preview_url: None,
            local_video_deleted: false,
            resolution: Some("720p".to_string()),
            ratio: Some("9:16".to_string()),
            duration_seconds: Some(12),
            queue_position: Some(1),
            submitted_at: Some("2026-06-03T08:00:00Z".to_string()),
            updated_at: Some("2026-06-03T08:00:00Z".to_string()),
            reference_image_count: Some(0),
        }
    }

    fn sample_reference_image() -> ReferenceImageRequest {
        ReferenceImageRequest {
            mime_type: "image/png".to_string(),
            data_url: "data:image/png;base64,aGVsbG8=".to_string(),
            file_name: Some("product.png".to_string()),
        }
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

    #[tokio::test]
    async fn graphic_marketing_history_persists_package_and_image() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        init_graphic_marketing_history_schema(&db).await.unwrap();
        let package_req = CreateGraphicPackageRequest {
            product: "云柑礼盒".to_string(),
            selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
            audience: "25-40 岁办公室人群".to_string(),
            price_range: "99-199 元".to_string(),
            platform: "小红书".to_string(),
            goal: "新品种草".to_string(),
            style: "真实测评".to_string(),
        };
        let package = create_graphic_package(&package_req);

        let package = insert_graphic_package_history(&db, "user-1", &package_req, package)
            .await
            .unwrap();
        let record_id = package.history_id.clone().unwrap();
        let image_req = CreateGraphicImageRequest {
            product: "云柑礼盒".to_string(),
            platform: "小红书".to_string(),
            prompt: package.image_prompt.clone(),
            size: "1024x1536".to_string(),
            quality: "medium".to_string(),
            package_id: Some(record_id.clone()),
        };
        let image = GraphicImageResponse {
            id: "graphic-image-1".to_string(),
            provider: "openai-compatible-image".to_string(),
            status: "completed".to_string(),
            message: "图片已生成。".to_string(),
            image_url: Some(
                "/api/v1/ai-store-manager/graphic-assets/graphic-image-1.png".to_string(),
            ),
            b64_json: None,
        };

        upsert_graphic_image_history(&db, "user-1", &image_req, None, image)
            .await
            .unwrap();

        let history = list_graphic_marketing_history(&db, "user-1", 20)
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, record_id);
        assert_eq!(history[0].product, "云柑礼盒");
        assert_eq!(
            history[0].package.as_ref().unwrap().image_prompt,
            package.image_prompt
        );
        assert_eq!(
            history[0].image.as_ref().unwrap().image_url.as_deref(),
            Some("/api/v1/ai-store-manager/graphic-assets/graphic-image-1.png")
        );

        let other_user = list_graphic_marketing_history(&db, "user-2", 20)
            .await
            .unwrap();
        assert!(other_user.is_empty());
    }

    #[test]
    fn image_payload_targets_volcengine_seedream_options() {
        let req = CreateGraphicImageRequest {
            product: "云柑礼盒".to_string(),
            platform: "小红书".to_string(),
            prompt: "生成海报".to_string(),
            size: "1024x1536".to_string(),
            quality: "medium".to_string(),
            package_id: None,
        };

        let payload = build_image_generation_payload("doubao-seedream-5.0-lite", &req);

        assert_eq!(payload.model, "doubao-seedream-5.0-lite");
        assert!(payload.prompt.contains("生成海报"));
        assert!(payload.prompt.contains("3:4"));
        assert_eq!(payload.size, "2K");
        assert_eq!(payload.output_format, "png");
        assert!(!payload.watermark);
        assert_eq!(payload.sequential_image_generation, "disabled");
        assert!(payload.image.is_none());
    }

    #[test]
    fn image_payload_can_embed_reference_image_as_data_uri() {
        let req = CreateGraphicImageRequest {
            product: "云柑礼盒".to_string(),
            platform: "小红书".to_string(),
            prompt: "生成营销图".to_string(),
            size: "1024x1024".to_string(),
            quality: "high".to_string(),
            package_id: None,
        };

        let payload = build_image_generation_payload_with_image(
            "doubao-seedream-5.0-lite",
            &req,
            Some("image/png"),
            Some("abc123"),
        );

        assert_eq!(payload.size, "4K");
        assert_eq!(
            payload.image.as_deref(),
            Some(&["data:image/png;base64,abc123".to_string()][..])
        );
    }

    #[test]
    fn image_generation_url_uses_generations_endpoint() {
        assert_eq!(
            image_generation_url("https://ark.cn-beijing.volces.com/api/plan/v3/"),
            "https://ark.cn-beijing.volces.com/api/plan/v3/images/generations"
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
            package_id: None,
        };

        assert!(validate_graphic_image_edit_request(&req).is_err());

        req.image_b64 = "aGVsbG8=".to_string();
        assert!(validate_graphic_image_edit_request(&req).is_ok());
    }

    #[test]
    fn image_result_normalizes_base64_response() {
        let response = SeedreamImageResponse {
            data: vec![SeedreamImageData {
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
        let response = SeedreamImageResponse { data: vec![] };

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
            package_id: None,
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
