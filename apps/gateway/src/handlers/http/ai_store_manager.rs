use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use base64::engine::general_purpose;
use base64::Engine as _;
use beebotos_agents::communication::PlatformType;
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

const VIDEO_TASK_LIST_LIMIT: usize = 6;
const VIDEO_TASK_HISTORY_LIMIT: i64 = 20;
const VIDEO_ASSET_DIR: &str = "data/ai-video-marketing/videos";
const VIDEO_ASSET_ROUTE: &str = "/api/v1/ai-store-manager/video-assets";

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
struct VolcengineVideoTaskListResponse {
    items: Vec<VolcengineVideoTaskListItem>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct VolcengineVideoTaskListItem {
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
    VolcengineVideoTaskRequest {
        model: video_task_model(model, req),
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
            resolution TEXT,
            ratio TEXT,
            duration_seconds INTEGER,
            generate_audio INTEGER NOT NULL DEFAULT 1,
            watermark INTEGER NOT NULL DEFAULT 0,
            queue_position INTEGER,
            submitted_at TEXT,
            updated_at TEXT,
            created_at TEXT NOT NULL,
            last_synced_at TEXT NOT NULL
        )
        "#,
    )
    .execute(db)
    .await
    .map_err(|err| GatewayError::internal(format!("初始化视频任务历史表失败: {}", err)))?;

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

    sqlx::query(
        r#"
        INSERT INTO ai_video_marketing_tasks (
            id, user_id, product, platform, prompt, model, provider, status, message,
            preview_url, resolution, ratio, duration_seconds, generate_audio, watermark,
            queue_position, submitted_at, updated_at, created_at, last_synced_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
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
            resolution = excluded.resolution,
            ratio = excluded.ratio,
            duration_seconds = excluded.duration_seconds,
            generate_audio = excluded.generate_audio,
            watermark = excluded.watermark,
            queue_position = excluded.queue_position,
            submitted_at = excluded.submitted_at,
            updated_at = excluded.updated_at,
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
    .bind(resolution)
    .bind(ratio)
    .bind(duration_seconds as i64)
    .bind(if req.generate_audio { 1_i64 } else { 0_i64 })
    .bind(if req.watermark { 1_i64 } else { 0_i64 })
    .bind(queue_position)
    .bind(&submitted_at)
    .bind(&updated_at)
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

    sqlx::query(
        r#"
        INSERT INTO ai_video_marketing_tasks (
            id, user_id, product, platform, prompt, model, provider, status, message,
            preview_url, resolution, ratio, duration_seconds, generate_audio, watermark,
            queue_position, submitted_at, updated_at, created_at, last_synced_at
        )
        VALUES (?1, ?2, '', '', '', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 0, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(id) DO UPDATE SET
            user_id = excluded.user_id,
            model = excluded.model,
            provider = excluded.provider,
            status = excluded.status,
            message = excluded.message,
            preview_url = excluded.preview_url,
            resolution = COALESCE(excluded.resolution, ai_video_marketing_tasks.resolution),
            ratio = COALESCE(excluded.ratio, ai_video_marketing_tasks.ratio),
            duration_seconds = COALESCE(excluded.duration_seconds, ai_video_marketing_tasks.duration_seconds),
            queue_position = excluded.queue_position,
            submitted_at = COALESCE(ai_video_marketing_tasks.submitted_at, excluded.submitted_at),
            updated_at = excluded.updated_at,
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
    .bind(task.resolution.as_deref())
    .bind(task.ratio.as_deref())
    .bind(duration_seconds)
    .bind(queue_position)
    .bind(&submitted_at)
    .bind(&updated_at)
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
               duration_seconds, queue_position, submitted_at, updated_at
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
        })
        .collect())
}

fn merge_video_task_history_refresh(
    mut refreshed: VideoTaskResponse,
    previous: &VideoTaskResponse,
) -> VideoTaskResponse {
    if refreshed.preview_url.is_none() {
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

fn video_task_list_url(base_url: &str, page_size: usize) -> String {
    format!(
        "{}?page_num=1&page_size={}",
        video_task_url(base_url),
        page_size
    )
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
        resolution: None,
        ratio: None,
        duration_seconds: None,
        queue_position: None,
        submitted_at: Some(now.clone()),
        updated_at: Some(now),
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
        resolution: None,
        ratio: None,
        duration_seconds: None,
        queue_position: None,
        submitted_at: Some(now.clone()),
        updated_at: Some(now),
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
        resolution: Some(req.resolution.trim().to_string()),
        ratio: Some(req.ratio.trim().to_string()),
        duration_seconds: Some(req.duration_seconds),
        queue_position: Some(1),
        submitted_at: Some(now.clone()),
        updated_at: Some(now),
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
        resolution: None,
        ratio: None,
        duration_seconds: None,
        queue_position: None,
        submitted_at: None,
        updated_at: Some(Utc::now().to_rfc3339()),
    }
}

fn video_list_item_to_response(
    item: VolcengineVideoTaskListItem,
    fallback_model: &str,
) -> VideoTaskResponse {
    video_status_to_response(
        VolcengineVideoTaskStatusResponse {
            id: item.id,
            model: item.model,
            status: item.status,
            content: item.content,
            error: item.error,
        },
        fallback_model,
    )
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

fn build_video_package_prompt(req: &CreateVideoPackageRequest) -> String {
    let duration_seconds = package_duration_seconds(req);
    let ratio = package_ratio(req);
    let voiceover_limit = package_voiceover_limit(duration_seconds);
    let scene_limit = package_scene_limit(duration_seconds);
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

要求：
1. 结果必须贴合商品、平台、人群、目标和风格，不要输出通用模板。
2. {audio_requirement}
3. `storyboard`、`subtitles`、`shot_prompts` 每项 2 到 {scene_limit} 条，镜头数量必须适合 {duration_seconds} 秒。
4. `video_prompt` 会直接传给视频生成模型，必须是一段紧凑成片提示词，包含商品主体、镜头顺序、画幅、节奏、字幕/口播约束。
5. 只输出 JSON，不要 Markdown，不要解释。

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

fn agent_output_text(output: &serde_json::Value) -> Option<String> {
    output
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            output
                .get("response")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .or_else(|| {
            output
                .get("content")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
}

async fn resolve_video_package_agent_id(
    state: &AppState,
    user: &AuthUser,
) -> Result<String, GatewayError> {
    if let Some(resolver) = &state.agent_resolver {
        return resolver
            .resolve(
                PlatformType::Custom,
                "ai-store-manager:video-marketing",
                &user.user_id,
            )
            .await;
    }

    let agents = state
        .agent_runtime
        .list_agents()
        .await
        .map_err(|err| GatewayError::internal(format!("读取内部 agent 失败: {}", err)))?;
    agents
        .into_iter()
        .find(|agent| {
            !matches!(
                agent.state,
                gateway::AgentState::Stopped | gateway::AgentState::Error
            )
        })
        .map(|agent| agent.agent_id)
        .ok_or_else(|| GatewayError::internal("没有可用的内部 agent"))
}

async fn generate_video_package_with_agent(
    state: &AppState,
    user: &AuthUser,
    req: &CreateVideoPackageRequest,
) -> Result<VideoPackageResponse, GatewayError> {
    let agent_id = resolve_video_package_agent_id(state, user).await?;
    let task = gateway::TaskConfig {
        task_type: "llm_chat".to_string(),
        input: json!({
            "content": build_video_package_prompt(req),
            "platform": "custom",
            "channel_id": "ai-store-manager:video-marketing",
            "user_id": user.user_id,
            "session_id": format!("ai-video-marketing-{}", uuid::Uuid::new_v4()),
        }),
        timeout_secs: 1200,
        priority: 6,
        stream_tx: None,
    };

    let result = state
        .agent_runtime
        .execute_task(&agent_id, task)
        .await
        .map_err(|err| GatewayError::internal(format!("内部 agent 执行失败: {}", err)))?;
    if !result.success {
        return Err(GatewayError::internal(
            result
                .error
                .unwrap_or_else(|| "内部 agent 生成视频脚本包失败".to_string()),
        ));
    }
    let raw = agent_output_text(&result.output)
        .ok_or_else(|| GatewayError::internal("内部 agent 返回为空"))?;
    video_package_from_agent_response(&raw, req, &agent_id)
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

pub async fn create_video_package_handler(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateVideoPackageRequest>,
) -> Result<Json<VideoPackageResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    validate_video_package_request(&req)?;
    let package = generate_video_package_with_agent(&state, &user, &req).await?;
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
        .json::<SeedreamImageResponse>()
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

    Ok(Json(result))
}

pub async fn list_video_tasks(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<Vec<VideoTaskResponse>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;
    let history =
        list_video_task_history(&state.db, &user.user_id, VIDEO_TASK_HISTORY_LIMIT).await?;
    if !history.is_empty() {
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
        return Ok(Json(history));
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
        None => return Ok(Json(Vec::new())),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.video_generation.timeout_seconds,
        ))
        .build()
        .map_err(|err| GatewayError::internal(format!("视频生成客户端创建失败: {}", err)))?;

    let response = client
        .get(video_task_list_url(
            &config.video_generation.base_url,
            VIDEO_TASK_LIST_LIMIT,
        ))
        .bearer_auth(&api_key)
        .send()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务列表查询失败: {}", err)))?;

    let status = response.status();
    if !status.is_success() {
        let upstream_error = response
            .json::<VolcengineApiErrorEnvelope>()
            .await
            .ok()
            .and_then(|envelope| envelope.error);
        return Ok(Json(vec![video_upstream_unavailable_response(
            &model,
            status,
            upstream_error.as_ref(),
        )]));
    }

    let list = response
        .json::<VolcengineVideoTaskListResponse>()
        .await
        .map_err(|err| GatewayError::internal(format!("视频任务列表响应解析失败: {}", err)))?;

    let mut tasks = Vec::new();
    for item in list.items.into_iter().take(VIDEO_TASK_LIST_LIMIT) {
        let id = item.id.clone();
        let detail = client
            .get(video_task_status_url(
                &config.video_generation.base_url,
                &id,
            ))
            .bearer_auth(&api_key)
            .send()
            .await
            .ok()
            .filter(|response| response.status().is_success());

        if let Some(detail) = detail {
            if let Ok(upstream) = detail.json::<VolcengineVideoTaskStatusResponse>().await {
                let task = video_status_to_response(upstream, &model);
                let task =
                    cache_video_task_preview(&default_video_asset_root(), &client, task).await;
                update_video_task_history(&state.db, &user.user_id, &task).await?;
                tasks.push(task);
                continue;
            }
        }

        let task = video_list_item_to_response(item, &model);
        let task = cache_video_task_preview(&default_video_asset_root(), &client, task).await;
        update_video_task_history(&state.db, &user.user_id, &task).await?;
        tasks.push(task);
    }

    Ok(Json(tasks))
}

pub async fn get_video_task(
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
    let task = video_status_to_response(upstream, &model);
    let task = cache_video_task_preview(&default_video_asset_root(), &client, task).await;
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
        };

        let payload = build_seedance_video_payload("doubao-seedance-2.0", &req);

        assert_eq!(payload.model, "doubao-seedance-lite-test");
        assert_eq!(payload.content[0].kind, "text");
        assert_eq!(payload.content[0].text, "为云柑礼盒生成短视频");
        assert_eq!(payload.duration, 8);
        assert_eq!(payload.resolution, "1080p");
        assert_eq!(payload.ratio, "16:9");
        assert!(!payload.generate_audio);
        assert!(payload.watermark);
    }

    #[test]
    fn video_task_url_uses_agent_plan_generation_endpoint() {
        assert_eq!(
            video_task_url("https://ark.cn-beijing.volces.com/api/plan/v3"),
            "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks"
        );
    }

    #[test]
    fn video_task_list_url_uses_agent_plan_generation_endpoint() {
        assert_eq!(
            video_task_list_url("https://ark.cn-beijing.volces.com/api/plan/v3", 6),
            "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks?page_num=1&page_size=6"
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
        };

        let prompt = build_video_package_prompt(&req);

        assert!(prompt.contains("计划成片：5 秒，9:16 画幅"));
        assert!(prompt.contains("最多 28 个中文字符"));
        assert!(prompt.contains("每项 2 到 2 条"));
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

    fn sample_video_task_response(id: &str, status: &str) -> VideoTaskResponse {
        VideoTaskResponse {
            id: id.to_string(),
            provider: "volcengine-ark".to_string(),
            model: "doubao-seedance-2.0".to_string(),
            status: status.to_string(),
            message: status.to_string(),
            preview_url: None,
            resolution: Some("720p".to_string()),
            ratio: Some("9:16".to_string()),
            duration_seconds: Some(12),
            queue_position: Some(1),
            submitted_at: Some("2026-06-03T08:00:00Z".to_string()),
            updated_at: Some("2026-06-03T08:00:00Z".to_string()),
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

    #[test]
    fn image_payload_targets_volcengine_seedream_options() {
        let req = CreateGraphicImageRequest {
            product: "云柑礼盒".to_string(),
            platform: "小红书".to_string(),
            prompt: "生成海报".to_string(),
            size: "1024x1536".to_string(),
            quality: "medium".to_string(),
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
