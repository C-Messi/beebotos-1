#[cfg(target_arch = "wasm32")]
use base64::{engine::general_purpose, Engine as _};
#[cfg(target_arch = "wasm32")]
use gloo_storage::{LocalStorage, Storage};
#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

use crate::api::{
    create_ai_store_manager_service, CreateVideoPackageRequest, CreateVideoTaskRequest,
    ReferenceImageRequest, VideoPackageResponse, VideoTaskResponse,
};
use crate::state::use_app_state;
use crate::utils::event_target_value;

const VIDEO_TASK_POLL_INTERVAL_MS: u32 = 5_000;
const VIDEO_TASK_MAX_POLLS: usize = 96;
const VIDEO_TASK_QUEUE_LIMIT: usize = 20;
const DEFAULT_VIDEO_PLATFORM: &str = "抖音";
#[cfg(target_arch = "wasm32")]
const VIDEO_TASK_QUEUE_STORAGE_KEY: &str = "beebotos_ai_video_marketing_tasks";
#[cfg(target_arch = "wasm32")]
const VIDEO_MARKETING_DRAFT_STORAGE_KEY: &str = "beebotos_ai_video_marketing_draft";
const VIDEO_MODEL_OPTIONS: [&str; 3] = [
    "doubao-seedance-2.0",
    "doubao-seedance-2.0-fast",
    "doubao-seedance-1.5-pro",
];
const VIDEO_DURATION_OPTIONS: [&str; 4] = ["5", "8", "12", "15"];
#[cfg(target_arch = "wasm32")]
const VIDEO_REFERENCE_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct VideoMarketingResult {
    pub label: &'static str,
    pub content: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoMarketingTask {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub goal: String,
    pub platform: String,
    pub style: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoGenerationOptions {
    pub model: String,
    pub duration_seconds: u8,
    pub resolution: String,
    pub ratio: String,
    pub generate_audio: bool,
    pub watermark: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct VideoMarketingDraft {
    task: VideoMarketingTask,
    options: VideoGenerationOptions,
    package: Option<VideoPackageResponse>,
    #[serde(default)]
    reference_image: Option<ReferenceImageRequest>,
}

pub fn default_video_marketing_task() -> VideoMarketingTask {
    VideoMarketingTask {
        product: "云柑礼盒".to_string(),
        selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
        audience: "25-40 岁办公室人群".to_string(),
        goal: "新品种草".to_string(),
        platform: DEFAULT_VIDEO_PLATFORM.to_string(),
        style: "真实测评".to_string(),
    }
}

fn default_video_marketing_draft() -> VideoMarketingDraft {
    VideoMarketingDraft {
        task: default_video_marketing_task(),
        options: default_video_generation_options(),
        package: None,
        reference_image: None,
    }
}

pub fn default_video_generation_options() -> VideoGenerationOptions {
    VideoGenerationOptions {
        model: "doubao-seedance-2.0".to_string(),
        duration_seconds: 12,
        resolution: "720p".to_string(),
        ratio: "9:16".to_string(),
        generate_audio: true,
        watermark: false,
    }
}

fn package_results(package: &VideoPackageResponse) -> Vec<VideoMarketingResult> {
    vec![
        VideoMarketingResult {
            label: "爆款标题",
            content: package.title.clone(),
        },
        VideoMarketingResult {
            label: "3 秒钩子",
            content: package.hook.clone(),
        },
        VideoMarketingResult {
            label: "口播脚本",
            content: package.oral_script.clone(),
        },
        VideoMarketingResult {
            label: "分镜脚本",
            content: package.storyboard.join(" -> "),
        },
        VideoMarketingResult {
            label: "字幕文案",
            content: package.subtitles.join(" / "),
        },
        VideoMarketingResult {
            label: "镜头提示",
            content: package.shot_prompts.join("；"),
        },
        VideoMarketingResult {
            label: "视频模型提示词",
            content: package.video_prompt.clone(),
        },
        VideoMarketingResult {
            label: "话题标签",
            content: package.tags.join(" "),
        },
    ]
}

fn voiceover_char_limit(duration_seconds: u8) -> usize {
    match duration_seconds {
        0..=5 => 28,
        6..=8 => 42,
        9..=12 => 64,
        _ => 80,
    }
}

fn scene_limit(duration_seconds: u8) -> usize {
    match duration_seconds {
        0..=5 => 2,
        6..=8 => 3,
        _ => 4,
    }
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut compact = trimmed.chars().take(max_chars).collect::<String>();
    compact.push('。');
    compact
}

fn compact_join(items: &[String], limit: usize, separator: &str) -> String {
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .join(separator)
}

fn reference_images_for_request(
    reference_image: &Option<ReferenceImageRequest>,
) -> Vec<ReferenceImageRequest> {
    reference_image.iter().cloned().collect()
}

fn reference_image_display_name(image: &ReferenceImageRequest) -> String {
    image
        .file_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("参考图片")
        .to_string()
}

fn reference_image_count_label(count: Option<u8>) -> &'static str {
    match count.unwrap_or(0) {
        0 => "无参考图",
        _ => "1 张参考图",
    }
}

#[cfg(target_arch = "wasm32")]
fn normalize_video_reference_image_mime(filename: &str, mime_type: &str) -> Option<String> {
    let mime = mime_type.trim();
    if matches!(mime, "image/png" | "image/jpeg" | "image/webp") {
        return Some(mime.to_string());
    }

    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png".to_string())
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg".to_string())
    } else if lower.ends_with(".webp") {
        Some("image/webp".to_string())
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
async fn video_reference_image_from_file(
    file: web_sys::File,
) -> Result<ReferenceImageRequest, String> {
    let file_name = file.name();
    let mime_type = normalize_video_reference_image_mime(&file_name, &file.type_())
        .ok_or_else(|| "参考图片仅支持 PNG、JPG、WebP。".to_string())?;
    let array_buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "参考图片读取失败。".to_string())?;
    let bytes = Uint8Array::new(&array_buffer).to_vec();
    if bytes.is_empty() {
        return Err("参考图片不能为空。".to_string());
    }
    if bytes.len() > VIDEO_REFERENCE_IMAGE_MAX_BYTES {
        return Err("参考图片不能超过 8MB。".to_string());
    }

    Ok(ReferenceImageRequest {
        mime_type: mime_type.clone(),
        data_url: format!(
            "data:{};base64,{}",
            mime_type,
            general_purpose::STANDARD.encode(bytes)
        ),
        file_name: Some(file_name),
    })
}

fn build_seedance_prompt_from_package(
    package: &VideoPackageResponse,
    options: &VideoGenerationOptions,
) -> String {
    let duration_seconds = options.duration_seconds;
    let scene_limit = scene_limit(duration_seconds);
    let voiceover_limit = voiceover_char_limit(duration_seconds);
    let storyboard = compact_join(&package.storyboard, scene_limit, " -> ");
    let subtitles = compact_join(&package.subtitles, scene_limit, " / ");
    let shot_prompts = compact_join(&package.shot_prompts, scene_limit, "；");
    let audio_line = if options.generate_audio {
        format!(
            "口播：{}。口播总字数不超过 {} 个中文字符，按画面节奏自然播报，不要扩写长稿。",
            compact_text(&package.oral_script, voiceover_limit),
            voiceover_limit
        )
    } else {
        "音频：不要生成口播；只用字幕短句承接卖点。".to_string()
    };

    format!(
        "生成 {duration} 秒 {ratio} {resolution} \
         短视频。\n商品主题：{title}\n核心画面：{video_prompt}\n3 \
         秒钩子：{hook}\n画面顺序：{storyboard}\n镜头要求：{shot_prompts}\n字幕短句：{subtitles}\\
         n{audio_line}\n约束：画面必须围绕同一商品和卖点，人物、场景、字幕与口播保持一致，\
         不要加入无关商品、价格或品牌。",
        duration = duration_seconds,
        ratio = options.ratio.trim(),
        resolution = options.resolution.trim(),
        title = package.title.trim(),
        video_prompt = package.video_prompt.trim(),
        hook = package.hook.trim(),
        storyboard = storyboard,
        shot_prompts = shot_prompts,
        subtitles = subtitles,
        audio_line = audio_line
    )
}

fn should_poll_video_task(task: &VideoTaskResponse) -> bool {
    matches!(task.status.as_str(), "queued" | "running")
}

fn normalize_video_task_queue(tasks: Vec<VideoTaskResponse>) -> Vec<VideoTaskResponse> {
    let mut queue = Vec::new();
    for task in tasks {
        if !task.id.trim().is_empty()
            && !queue
                .iter()
                .any(|item: &VideoTaskResponse| item.id == task.id)
        {
            queue.push(task);
            if queue.len() >= VIDEO_TASK_QUEUE_LIMIT {
                break;
            }
        }
    }
    queue
}

#[cfg(target_arch = "wasm32")]
fn load_video_task_queue() -> Vec<VideoTaskResponse> {
    LocalStorage::get(VIDEO_TASK_QUEUE_STORAGE_KEY)
        .map(normalize_video_task_queue)
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_video_task_queue() -> Vec<VideoTaskResponse> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
fn save_video_task_queue(queue: &[VideoTaskResponse]) {
    let _ = LocalStorage::set(VIDEO_TASK_QUEUE_STORAGE_KEY, queue);
}

#[cfg(not(target_arch = "wasm32"))]
fn save_video_task_queue(_queue: &[VideoTaskResponse]) {}

#[cfg(target_arch = "wasm32")]
fn load_video_marketing_draft() -> VideoMarketingDraft {
    LocalStorage::get(VIDEO_MARKETING_DRAFT_STORAGE_KEY)
        .map(normalize_video_marketing_draft)
        .unwrap_or_else(|_| default_video_marketing_draft())
}

#[cfg(not(target_arch = "wasm32"))]
fn load_video_marketing_draft() -> VideoMarketingDraft {
    default_video_marketing_draft()
}

#[cfg(target_arch = "wasm32")]
fn save_video_marketing_draft(draft: &VideoMarketingDraft) {
    let _ = LocalStorage::set(VIDEO_MARKETING_DRAFT_STORAGE_KEY, draft);
}

#[cfg(not(target_arch = "wasm32"))]
fn save_video_marketing_draft(_draft: &VideoMarketingDraft) {}

fn persist_video_marketing_draft(
    task: VideoMarketingTask,
    options: VideoGenerationOptions,
    package: Option<VideoPackageResponse>,
    reference_image: Option<ReferenceImageRequest>,
) {
    save_video_marketing_draft(&normalize_video_marketing_draft(VideoMarketingDraft {
        task,
        options,
        package,
        reference_image,
    }));
}

fn normalize_video_marketing_draft(mut draft: VideoMarketingDraft) -> VideoMarketingDraft {
    draft.task.platform = DEFAULT_VIDEO_PLATFORM.to_string();
    draft
}

fn video_task_status_label(status: &str) -> &'static str {
    match status {
        "blocked" => "待配置",
        "completed" => "视频已生成",
        "cancelled" => "已取消",
        "failed" | "expired" => "生成失败",
        "queued" => "排队中",
        "running" => "生成中",
        _ => "状态已更新",
    }
}

fn video_task_poll_error_status() -> &'static str {
    "状态刷新失败"
}

fn can_request_video_cancel(task: &VideoTaskResponse) -> bool {
    matches!(task.status.as_str(), "queued" | "running")
}

fn video_marketing_status_from_state(
    package: &Option<VideoPackageResponse>,
    video_tasks: &[VideoTaskResponse],
) -> String {
    video_tasks
        .first()
        .map(|task| video_task_status_label(&task.status).to_string())
        .unwrap_or_else(|| {
            if package.is_some() {
                "脚本包待审核".to_string()
            } else {
                "待生成脚本包".to_string()
            }
        })
}

fn upsert_video_task(queue: &mut Vec<VideoTaskResponse>, task: VideoTaskResponse) {
    let task = queue
        .iter()
        .find(|item| item.id == task.id)
        .map(|previous| merge_video_task_update(task.clone(), previous))
        .unwrap_or(task);
    queue.retain(|item| item.id != task.id);
    queue.insert(0, task);
    queue.truncate(VIDEO_TASK_QUEUE_LIMIT);
    save_video_task_queue(queue);
}

fn merge_video_task_update(
    mut updated: VideoTaskResponse,
    previous: &VideoTaskResponse,
) -> VideoTaskResponse {
    if updated.resolution.is_none() {
        updated.resolution = previous.resolution.clone();
    }
    if updated.ratio.is_none() {
        updated.ratio = previous.ratio.clone();
    }
    if updated.duration_seconds.is_none() {
        updated.duration_seconds = previous.duration_seconds;
    }
    if updated.queue_position.is_none() {
        updated.queue_position = previous.queue_position;
    }
    if updated.reference_image_count.is_none() {
        updated.reference_image_count = previous.reference_image_count;
    }
    if updated.submitted_at.is_none() {
        updated.submitted_at = previous.submitted_at.clone();
    }
    updated
}

fn completed_video_previews(tasks: &[VideoTaskResponse]) -> Vec<VideoTaskResponse> {
    tasks
        .iter()
        .filter(|task| task.status == "completed" && task.preview_url.is_some())
        .cloned()
        .collect()
}

#[component]
pub fn AiVideoMarketingPage() -> impl IntoView {
    let restored_draft = load_video_marketing_draft();
    let restored_video_tasks = load_video_task_queue();
    let restored_live_task = restored_video_tasks
        .iter()
        .find(|task| should_poll_video_task(task))
        .cloned();
    let initial_task_status =
        video_marketing_status_from_state(&restored_draft.package, &restored_video_tasks);
    let (task, set_task) = signal(restored_draft.task);
    let (generation_options, set_generation_options) = signal(restored_draft.options);
    let (task_status, set_task_status) = signal(initial_task_status);
    let (package, set_package) = signal(restored_draft.package);
    let (reference_image, set_reference_image) = signal(restored_draft.reference_image);
    let (reference_image_error, set_reference_image_error) = signal::<Option<String>>(None);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_reference_image_error;
    let (package_error, set_package_error) = signal::<Option<String>>(None);
    let (package_loading, set_package_loading) = signal(false);
    let (video_tasks, set_video_tasks) = signal::<Vec<VideoTaskResponse>>(restored_video_tasks);
    let (video_error, set_video_error) = signal::<Option<String>>(None);
    let (video_loading, set_video_loading) = signal(restored_live_task.is_some());
    let (restore_video_task_id, set_restore_video_task_id) = signal(String::new());
    let (restore_video_loading, set_restore_video_loading) = signal(false);
    let (canceling_video_task_id, set_canceling_video_task_id) = signal::<Option<String>>(None);
    let video_service = create_ai_store_manager_service(use_app_state().api_client());
    let video_service = StoredValue::new(video_service);

    {
        let service = video_service.get_value();
        spawn_local(async move {
            if let Ok(tasks) = service.list_video_tasks().await {
                if tasks.is_empty() {
                    return;
                }

                let mut latest_status = None;
                set_video_tasks.update(|queue| {
                    for task in tasks.into_iter().rev() {
                        upsert_video_task(queue, task);
                    }
                    latest_status = queue
                        .first()
                        .map(|task| video_task_status_label(&task.status).to_string());
                });
                if let Some(status) = latest_status {
                    set_task_status.set(status);
                }
            }
        });
    }

    if let Some(restored_task) = restored_live_task {
        let service = video_service.get_value();
        spawn_local(async move {
            let mut latest = restored_task;
            for _ in 0..VIDEO_TASK_MAX_POLLS {
                if !should_poll_video_task(&latest) {
                    break;
                }

                gloo_timers::future::TimeoutFuture::new(VIDEO_TASK_POLL_INTERVAL_MS).await;

                match service.get_video_task(&latest.id).await {
                    Ok(updated) => {
                        latest = merge_video_task_update(updated, &latest);
                        set_task_status.set(video_task_status_label(&latest.status).to_string());
                        set_video_tasks.update(|queue| upsert_video_task(queue, latest.clone()));
                    }
                    Err(err) => {
                        set_video_error.set(Some(err.to_string()));
                        set_task_status.set(video_task_poll_error_status().to_string());
                        break;
                    }
                }
            }
            set_video_loading.set(false);
        });
    }

    let generate_package = move || {
        let service = video_service.get_value();
        let current_task = task.get();
        let options = generation_options.get();
        let current_reference_image = reference_image.get();
        let req = CreateVideoPackageRequest {
            product: current_task.product.clone(),
            selling_points: current_task.selling_points.clone(),
            audience: current_task.audience.clone(),
            goal: current_task.goal.clone(),
            platform: current_task.platform.clone(),
            style: current_task.style.clone(),
            duration_seconds: Some(options.duration_seconds),
            ratio: Some(options.ratio.clone()),
            generate_audio: Some(options.generate_audio),
            reference_images: reference_images_for_request(&current_reference_image),
        };

        set_package_loading.set(true);
        set_package_error.set(None);
        set_video_error.set(None);
        set_task_status.set("AI 脚本包生成中".to_string());

        spawn_local(async move {
            match service.create_video_package(&req).await {
                Ok(response) => {
                    persist_video_marketing_draft(
                        current_task,
                        options,
                        Some(response.clone()),
                        current_reference_image,
                    );
                    set_package.set(Some(response));
                    set_task_status.set("脚本包待审核".to_string());
                }
                Err(err) => {
                    set_package_error.set(Some(err.to_string()));
                    set_task_status.set("脚本包生成失败".to_string());
                }
            }
            set_package_loading.set(false);
        });
    };

    let create_video = move || {
        let Some(current_package) = package.get() else {
            set_video_error.set(Some("请先生成 AI 脚本包".to_string()));
            return;
        };

        let service = video_service.get_value();
        let current_task = task.get();
        let options = generation_options.get();
        let current_reference_image = reference_image.get();
        let req = CreateVideoTaskRequest {
            product: current_task.product.clone(),
            platform: current_task.platform.clone(),
            prompt: build_seedance_prompt_from_package(&current_package, &options),
            model: Some(options.model.clone()),
            duration_seconds: options.duration_seconds,
            resolution: options.resolution.clone(),
            ratio: options.ratio.clone(),
            generate_audio: options.generate_audio,
            watermark: options.watermark,
            reference_images: reference_images_for_request(&current_reference_image),
        };

        set_video_loading.set(true);
        set_video_error.set(None);
        set_task_status.set("视频任务提交中".to_string());

        spawn_local(async move {
            match service.create_video_task(&req).await {
                Ok(mut latest) => {
                    set_task_status.set(video_task_status_label(&latest.status).to_string());
                    set_video_tasks.update(|queue| upsert_video_task(queue, latest.clone()));

                    for _ in 0..VIDEO_TASK_MAX_POLLS {
                        if !should_poll_video_task(&latest) {
                            break;
                        }

                        gloo_timers::future::TimeoutFuture::new(VIDEO_TASK_POLL_INTERVAL_MS).await;

                        match service.get_video_task(&latest.id).await {
                            Ok(updated) => {
                                latest = merge_video_task_update(updated, &latest);
                                set_task_status
                                    .set(video_task_status_label(&latest.status).to_string());
                                set_video_tasks
                                    .update(|queue| upsert_video_task(queue, latest.clone()));
                            }
                            Err(err) => {
                                set_video_error.set(Some(err.to_string()));
                                set_task_status.set(video_task_poll_error_status().to_string());
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    set_video_error.set(Some(err.to_string()));
                    set_task_status.set("生成失败".to_string());
                }
            }
            set_video_loading.set(false);
        });
    };

    let restore_video_task = move || {
        let id = restore_video_task_id.get().trim().to_string();
        if id.is_empty() {
            set_video_error.set(Some("请输入任务 ID".to_string()));
            return;
        }

        let service = video_service.get_value();
        set_restore_video_loading.set(true);
        set_video_error.set(None);
        set_task_status.set("状态刷新中".to_string());

        spawn_local(async move {
            match service.get_video_task(&id).await {
                Ok(updated) => {
                    set_task_status.set(video_task_status_label(&updated.status).to_string());
                    set_video_tasks.update(|queue| upsert_video_task(queue, updated));
                    set_restore_video_task_id.set(String::new());
                }
                Err(err) => {
                    set_video_error.set(Some(format!("任务恢复失败：{}", err)));
                    set_task_status.set(video_task_poll_error_status().to_string());
                }
            }
            set_restore_video_loading.set(false);
        });
    };

    let cancel_video = Callback::new(move |id: String| {
        if id.trim().is_empty() {
            return;
        }

        let service = video_service.get_value();
        set_canceling_video_task_id.set(Some(id.clone()));
        set_video_error.set(None);

        spawn_local(async move {
            match service.cancel_video_task(&id).await {
                Ok(updated) => {
                    set_task_status.set(video_task_status_label(&updated.status).to_string());
                    set_video_tasks.update(|queue| upsert_video_task(queue, updated));
                }
                Err(err) => {
                    set_video_error.set(Some(err.to_string()));
                }
            }
            set_canceling_video_task_id.set(None);
        });
    });

    view! {
        <Title text="AI 视频营销 - BeeBotOS" />
        <div class="page ai-video-marketing-page">
            <div class="page-header ai-video-marketing-header">
                <div>
                    <h2>"AI 视频营销"</h2>
                    <p>"用内部 agent 生成短视频脚本包，再提交视频模型生成成片。"</p>
                </div>
                <div class="ai-store-manager-actions">
                    <a class="btn btn-secondary" href="/ai-store-manager">"返回 AI 店长"</a>
                    <button class="btn btn-primary" disabled=move || package_loading.get() on:click=move |_| generate_package()>
                        {move || if package_loading.get() { "生成中..." } else { "生成 AI 脚本包" }}
                    </button>
                    <button class="btn btn-secondary" disabled=move || video_loading.get() on:click=move |_| create_video()>
                        {move || if video_loading.get() { "生成中..." } else { "生成视频" }}
                    </button>
                </div>
            </div>

            <section class="ai-video-marketing-workspace">
                <div class="ai-video-marketing-panel">
                    <div class="section-title compact">
                        <h2>"任务输入"</h2>
                    </div>
                    <div class="ai-video-status-row">
                        <span>"当前状态"</span>
                        <strong>{move || task_status.get()}</strong>
                    </div>
                    <div class="ai-video-form-grid">
                        <TextField label="商品" value=Signal::derive(move || task.get().product) on_input=move |value| {
                            set_task.update(|task| task.product = value);
                            set_package.set(None);
                            set_task_status.set("待生成脚本包".to_string());
                            persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                        } />
                        <TextField label="核心卖点" value=Signal::derive(move || task.get().selling_points) on_input=move |value| {
                            set_task.update(|task| task.selling_points = value);
                            set_package.set(None);
                            set_task_status.set("待生成脚本包".to_string());
                            persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                        } />
                        <TextField label="目标人群" value=Signal::derive(move || task.get().audience) on_input=move |value| {
                            set_task.update(|task| task.audience = value);
                            set_package.set(None);
                            set_task_status.set("待生成脚本包".to_string());
                            persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                        } />
                        <SelectField
                            label="营销目标"
                            value=Signal::derive(move || task.get().goal)
                            options=vec!["新品种草", "促销转化", "老客复购", "直播预热"]
                            on_change=move |value| {
                                set_task.update(|task| task.goal = value);
                                set_package.set(None);
                                set_task_status.set("待生成脚本包".to_string());
                                persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                            }
                        />
                        <SelectField
                            label="内容风格"
                            value=Signal::derive(move || task.get().style)
                            options=vec!["真实测评", "情绪种草", "痛点解决", "价格促销"]
                            on_change=move |value| {
                                set_task.update(|task| task.style = value);
                                set_package.set(None);
                                set_task_status.set("待生成脚本包".to_string());
                                persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                            }
                        />
                        <div class="ai-video-field ai-video-upload-field">
                            <span>"参考图片"</span>
                            <input
                                type="file"
                                accept="image/png,image/jpeg,image/webp"
                                on:change=move |event| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        let input = event
                                            .target()
                                            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok());
                                        let file = input
                                            .and_then(|input| input.files())
                                            .and_then(|files| files.get(0));
                                        if let Some(file) = file {
                                            set_reference_image_error.set(None);
                                            spawn_local(async move {
                                                match video_reference_image_from_file(file).await {
                                                    Ok(image) => {
                                                        set_reference_image.set(Some(image.clone()));
                                                        set_package.set(None);
                                                        set_task_status.set("待生成脚本包".to_string());
                                                        persist_video_marketing_draft(
                                                            task.get_untracked(),
                                                            generation_options.get_untracked(),
                                                            None,
                                                            Some(image),
                                                        );
                                                    }
                                                    Err(err) => set_reference_image_error.set(Some(err)),
                                                }
                                            });
                                        }
                                    }
                                    #[cfg(not(target_arch = "wasm32"))]
                                    {
                                        let _ = event;
                                    }
                                }
                            />
                            {move || reference_image.get().map(|image| {
                                let name = reference_image_display_name(&image);
                                view! {
                                    <div class="ai-video-reference-image">
                                        <img src=image.data_url.clone() alt=name.clone() />
                                        <div>
                                            <strong>{name}</strong>
                                            <button class="btn btn-secondary" type="button" on:click=move |_| {
                                                set_reference_image.set(None);
                                                set_package.set(None);
                                                set_task_status.set("待生成脚本包".to_string());
                                                persist_video_marketing_draft(
                                                    task.get_untracked(),
                                                    generation_options.get_untracked(),
                                                    None,
                                                    None,
                                                );
                                            }>"移除"</button>
                                        </div>
                                    </div>
                                }
                            })}
                            {move || reference_image_error.get().map(|error| view! {
                                <p class="ai-video-upload-error">{error}</p>
                            })}
                        </div>
                    </div>
                </div>

                <div class="ai-video-marketing-panel">
                    <div class="section-title compact">
                        <h2>"AI 脚本包"</h2>
                    </div>
                    {move || {
                        if let Some(error) = package_error.get() {
                            view! { <div class="ai-video-task-card error">{error}</div> }.into_any()
                        } else if let Some(package) = package.get() {
                            view! {
                                <div class="ai-video-result-list">
                                    {package_results(&package)
                                        .into_iter()
                                        .map(|result| view! { <ResultItem result=result /> })
                                        .collect_view()}
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="ai-video-task-card">
                                    "点击生成 AI 脚本包后，内部 agent 会返回可传给视频模型的结构化结果。"
                                </div>
                            }.into_any()
                        }
                    }}
                </div>
            </section>

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"视频参数"</h2>
                </div>
                <div class="ai-video-form-grid">
                    <SelectField
                        label="视频模型"
                        value=Signal::derive(move || generation_options.get().model)
                        options=VIDEO_MODEL_OPTIONS.to_vec()
                        on_change=move |value| {
                            set_generation_options.update(|options| options.model = value);
                            persist_video_marketing_draft(task.get(), generation_options.get(), package.get(), reference_image.get());
                        }
                    />
                    <SelectField
                        label="分辨率"
                        value=Signal::derive(move || generation_options.get().resolution)
                        options=vec!["480p", "720p", "1080p"]
                        on_change=move |value| {
                            set_generation_options.update(|options| options.resolution = value);
                            persist_video_marketing_draft(task.get(), generation_options.get(), package.get(), reference_image.get());
                        }
                    />
                    <SelectField
                        label="画幅比例"
                        value=Signal::derive(move || generation_options.get().ratio)
                        options=vec!["9:16", "16:9", "1:1", "3:4", "4:3", "21:9", "adaptive"]
                        on_change=move |value| {
                            set_generation_options.update(|options| options.ratio = value);
                            set_package.set(None);
                            set_task_status.set("待生成脚本包".to_string());
                            persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                        }
                    />
                    <SelectField
                        label="时长"
                        value=Signal::derive(move || generation_options.get().duration_seconds.to_string())
                        options=VIDEO_DURATION_OPTIONS.to_vec()
                        on_change=move |value| {
                            if let Ok(duration) = value.parse::<u8>() {
                                set_generation_options.update(|options| options.duration_seconds = duration);
                                set_package.set(None);
                                set_task_status.set("待生成脚本包".to_string());
                                persist_video_marketing_draft(task.get(), generation_options.get(), None, reference_image.get());
                            }
                        }
                    />
                </div>
            </section>

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"生成队列"</h2>
                </div>
                <div class="ai-video-task-restore">
                    <label class="ai-video-field">
                        <span>"任务 ID"</span>
                        <input
                            type="text"
                            placeholder="cgt-..."
                            prop:value=Signal::derive(move || restore_video_task_id.get())
                            on:input=move |event| set_restore_video_task_id.set(event_target_value(&event))
                        />
                    </label>
                    <button
                        type="button"
                        class="btn btn-secondary"
                        disabled=move || restore_video_loading.get()
                        on:click=move |_| restore_video_task()
                    >
                        {move || if restore_video_loading.get() { "恢复中..." } else { "恢复任务" }}
                    </button>
                </div>
                {move || {
                    let tasks = video_tasks.get();
                    if tasks.is_empty() {
                        view! {
                            <div class="ai-video-task-card">
                                "生成视频后，任务会出现在这里并自动刷新状态。"
                            </div>
                        }.into_any()
                    } else {
                        let options = generation_options.get();
                        let canceling_id = canceling_video_task_id.get();
                        view! {
                            <div class="ai-video-queue-list">
                                {tasks.into_iter().map(|task| {
                                    let is_canceling =
                                        canceling_id.as_deref() == Some(task.id.as_str());
                                    view! {
                                        <VideoTaskCard
                                            task=task
                                            options=options.clone()
                                            is_canceling=is_canceling
                                            on_cancel=cancel_video
                                        />
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
                {move || video_error.get().map(|error| view! {
                    <div class="ai-video-task-card error">{error}</div>
                })}
            </section>

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"视频预览"</h2>
                </div>
                {move || {
                    let completed = completed_video_previews(&video_tasks.get());
                    if completed.is_empty() {
                        view! {
                            <div class="ai-video-task-card">
                                "视频完成后会在这里直接播放。"
                            </div>
                        }.into_any()
                    } else {
                        let options = generation_options.get();
                        view! {
                            <div class="ai-video-preview-list">
                                {completed.into_iter().map(|task| {
                                    view! { <VideoPreview task=task options=options.clone() /> }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
            </section>
        </div>
    }
}

#[component]
fn TextField(
    label: &'static str,
    value: Signal<String>,
    on_input: impl Fn(String) + Clone + 'static,
) -> impl IntoView {
    view! {
        <label class="ai-video-field">
            <span>{label}</span>
            <input
                type="text"
                prop:value=value
                on:input=move |event| on_input(event_target_value(&event))
            />
        </label>
    }
}

#[component]
fn SelectField(
    label: &'static str,
    value: Signal<String>,
    options: Vec<&'static str>,
    on_change: impl Fn(String) + Clone + 'static,
) -> impl IntoView {
    view! {
        <label class="ai-video-field">
            <span>{label}</span>
            <select prop:value=value on:change=move |event| on_change(event_target_value(&event))>
                {options.into_iter().map(|option| view! {
                    <option value=option>{option}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

#[component]
fn ResultItem(result: VideoMarketingResult) -> impl IntoView {
    view! {
        <article class="ai-video-result-item">
            <span>{result.label}</span>
            <p>{result.content}</p>
        </article>
    }
}

#[component]
fn VideoTaskCard(
    task: VideoTaskResponse,
    options: VideoGenerationOptions,
    is_canceling: bool,
    #[prop(into)] on_cancel: Callback<String>,
) -> impl IntoView {
    let status_label = video_task_status_label(&task.status);
    let can_cancel = can_request_video_cancel(&task);
    view! {
        <article class="ai-video-task-card">
            <div>
                <span>"任务 ID"</span>
                <strong>{task.id.clone()}</strong>
            </div>
            <div>
                <span>"状态"</span>
                <strong>{status_label}</strong>
            </div>
            <div>
                <span>"模型"</span>
                <strong>{task.model.clone()}</strong>
            </div>
            <div>
                <span>"参数"</span>
                <strong>{format_video_options_with_defaults(&task, &options)}</strong>
            </div>
            <div>
                <span>"参考图"</span>
                <strong>{reference_image_count_label(task.reference_image_count)}</strong>
            </div>
            <p>{task.message.clone()}</p>
            {task.updated_at.clone().map(|updated_at| view! {
                <small>{format!("更新：{}", updated_at)}</small>
            })}
            {can_cancel.then(|| {
                let task_id = task.id.clone();
                view! {
                    <div class="ai-video-task-actions">
                        <button
                            type="button"
                            class="btn btn-secondary ai-video-task-cancel"
                            disabled=is_canceling
                            on:click=move |_| on_cancel.run(task_id.clone())
                        >
                            {if is_canceling { "取消中..." } else { "取消生成" }}
                        </button>
                    </div>
                }
            })}
        </article>
    }
}

#[component]
fn VideoPreview(task: VideoTaskResponse, options: VideoGenerationOptions) -> impl IntoView {
    let url = task.preview_url.clone().unwrap_or_default();
    view! {
        <article class="ai-video-preview-card">
            <video class="ai-video-player" controls src=url.clone()></video>
            <div>
                <strong>{task.model.clone()}</strong>
                <span>{format_video_options_with_defaults(&task, &options)}</span>
                <span>{reference_image_count_label(task.reference_image_count)}</span>
                <a class="btn btn-secondary" href=url target="_blank" rel="noopener noreferrer">"查看原视频"</a>
            </div>
        </article>
    }
}

fn format_video_options_with_defaults(
    task: &VideoTaskResponse,
    options: &VideoGenerationOptions,
) -> String {
    let resolution = task.resolution.as_deref().unwrap_or(&options.resolution);
    let ratio = task.ratio.as_deref().unwrap_or(&options.ratio);
    let duration = task.duration_seconds.unwrap_or(options.duration_seconds);
    format!("{} / {} / {}s", resolution, ratio, duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{GraphicMarketingCheck, ReferenceImageRequest, VideoPackageResponse};

    #[test]
    fn seedance_prompt_uses_agent_generated_package() {
        let package = sample_video_package();
        let options = default_video_generation_options();

        let prompt = build_seedance_prompt_from_package(&package, &options);

        assert!(prompt.contains("云柑礼盒"));
        assert!(prompt.contains("真实办公室场景"));
        assert!(prompt.contains("果肉特写"));
        assert!(prompt.contains("开箱"));
    }

    #[test]
    fn seedance_prompt_compacts_package_for_short_video() {
        let mut package = sample_video_package();
        let mut options = default_video_generation_options();
        options.duration_seconds = 5;
        package.oral_script = "这盒云柑礼盒适合送礼和办公室分享，打开以后果香很明显，\
                               冷链送到手还是新鲜饱满，送客户、送同事、下午茶都很体面，\
                               这句不该进入五秒视频提示词。"
            .to_string();
        package.storyboard = vec![
            "开箱".to_string(),
            "果肉特写".to_string(),
            "同事试吃".to_string(),
            "礼盒收尾".to_string(),
        ];

        let prompt = build_seedance_prompt_from_package(&package, &options);

        assert!(prompt.contains("5 秒"));
        assert!(prompt.contains("口播总字数不超过 28"));
        assert!(prompt.contains("开箱 -> 果肉特写"));
        assert!(!prompt.contains("礼盒收尾"));
        assert!(!prompt.contains("这句不该进入五秒视频提示词"));
    }

    #[test]
    fn package_results_show_agent_video_prompt() {
        let package = sample_video_package();

        let results = package_results(&package);

        assert!(results.iter().any(|result| {
            result.label == "视频模型提示词" && result.content.contains("云柑礼盒开箱")
        }));
    }

    #[test]
    fn video_marketing_draft_round_trips_agent_package() {
        let draft = VideoMarketingDraft {
            task: default_video_marketing_task(),
            options: default_video_generation_options(),
            package: Some(sample_video_package()),
            reference_image: None,
        };

        let encoded = serde_json::to_string(&draft).unwrap();
        let restored: VideoMarketingDraft = serde_json::from_str(&encoded).unwrap();

        assert_eq!(
            restored
                .package
                .as_ref()
                .map(|package| package.title.as_str()),
            Some("云柑礼盒真实开箱")
        );
        assert_eq!(
            video_marketing_status_from_state(&restored.package, &[]),
            "脚本包待审核"
        );
        assert_eq!(restored.options.duration_seconds, 12);
    }

    #[test]
    fn video_marketing_draft_round_trips_reference_image() {
        let draft = VideoMarketingDraft {
            task: default_video_marketing_task(),
            options: default_video_generation_options(),
            package: None,
            reference_image: Some(sample_reference_image()),
        };

        let encoded = serde_json::to_string(&draft).unwrap();
        let restored: VideoMarketingDraft = serde_json::from_str(&encoded).unwrap();

        assert_eq!(
            restored
                .reference_image
                .as_ref()
                .and_then(|image| image.file_name.as_deref()),
            Some("product.png")
        );
        assert_eq!(
            reference_images_for_request(&restored.reference_image).len(),
            1
        );
    }

    #[test]
    fn video_marketing_draft_hides_platform_as_internal_default() {
        let mut draft = VideoMarketingDraft {
            task: default_video_marketing_task(),
            options: default_video_generation_options(),
            package: None,
            reference_image: None,
        };
        draft.task.platform = "小红书".to_string();

        let normalized = normalize_video_marketing_draft(draft);

        assert_eq!(normalized.task.platform, DEFAULT_VIDEO_PLATFORM);
    }

    #[test]
    fn default_video_generation_options_are_user_visible_settings() {
        let options = default_video_generation_options();

        assert_eq!(options.model, "doubao-seedance-2.0");
        assert_eq!(options.duration_seconds, 12);
        assert_eq!(options.resolution, "720p");
        assert_eq!(options.ratio, "9:16");
        assert!(options.generate_audio);
        assert!(!options.watermark);
    }

    #[test]
    fn video_model_options_match_agent_plan_models() {
        assert_eq!(
            VIDEO_MODEL_OPTIONS,
            [
                "doubao-seedance-2.0",
                "doubao-seedance-2.0-fast",
                "doubao-seedance-1.5-pro"
            ]
        );
    }

    #[test]
    fn duration_options_include_longer_seedance_clip() {
        assert_eq!(VIDEO_DURATION_OPTIONS, ["5", "8", "12", "15"]);
    }

    #[test]
    fn cancelled_video_task_uses_cancelled_label() {
        assert_eq!(video_task_status_label("cancelled"), "已取消");
    }

    #[test]
    fn video_task_poll_error_does_not_claim_generation_failed() {
        assert_eq!(video_task_poll_error_status(), "状态刷新失败");
    }

    #[test]
    fn can_request_cancel_for_live_video_tasks_only() {
        let mut task = sample_video_task("task-1", "queued", None);

        assert!(can_request_video_cancel(&task));
        task.status = "running".to_string();
        assert!(can_request_video_cancel(&task));
        task.status = "completed".to_string();
        assert!(!can_request_video_cancel(&task));
        task.status = "cancelled".to_string();
        assert!(!can_request_video_cancel(&task));
    }

    #[test]
    fn video_task_queue_updates_existing_task_without_duplicates() {
        let mut queue = vec![sample_video_task("task-1", "queued", None)];
        upsert_video_task(
            &mut queue,
            sample_video_task("task-1", "completed", Some("https://cdn.example/video.mp4")),
        );

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].status, "completed");
        assert_eq!(
            queue[0].preview_url.as_deref(),
            Some("https://cdn.example/video.mp4")
        );
    }

    #[test]
    fn video_task_queue_preserves_existing_options_when_history_lacks_them() {
        let mut queue = vec![sample_video_task("task-1", "queued", None)];
        let mut history_task =
            sample_video_task("task-1", "completed", Some("https://cdn.example/video.mp4"));
        history_task.resolution = None;
        history_task.ratio = None;
        history_task.duration_seconds = None;

        upsert_video_task(&mut queue, history_task);

        assert_eq!(queue[0].resolution.as_deref(), Some("720p"));
        assert_eq!(queue[0].ratio.as_deref(), Some("9:16"));
        assert_eq!(queue[0].duration_seconds, Some(8));
    }

    #[test]
    fn video_task_queue_preserves_reference_image_count_when_history_lacks_it() {
        let mut queue = vec![sample_video_task("task-1", "queued", None)];
        queue[0].reference_image_count = Some(1);
        let mut history_task =
            sample_video_task("task-1", "completed", Some("https://cdn.example/video.mp4"));
        history_task.reference_image_count = None;

        upsert_video_task(&mut queue, history_task);

        assert_eq!(queue[0].reference_image_count, Some(1));
    }

    #[test]
    fn video_options_display_uses_current_options_when_task_lacks_params() {
        let mut task = sample_video_task("task-1", "completed", Some("https://cdn.example/a.mp4"));
        task.resolution = None;
        task.ratio = None;
        task.duration_seconds = None;
        let options = default_video_generation_options();

        assert_eq!(
            format_video_options_with_defaults(&task, &options),
            "720p / 9:16 / 12s"
        );
    }

    #[test]
    fn completed_video_previews_include_all_completed_videos() {
        let tasks = vec![
            sample_video_task("task-1", "completed", Some("https://cdn.example/a.mp4")),
            sample_video_task("task-2", "running", None),
            sample_video_task("task-3", "completed", Some("https://cdn.example/b.mp4")),
        ];

        let previews = completed_video_previews(&tasks);

        assert_eq!(previews.len(), 2);
        assert_eq!(previews[0].id, "task-1");
        assert_eq!(previews[1].id, "task-3");
    }

    #[test]
    fn normalized_video_task_queue_keeps_latest_unique_tasks() {
        let tasks = vec![
            sample_video_task("task-1", "completed", Some("https://cdn.example/video.mp4")),
            sample_video_task("task-2", "running", None),
            sample_video_task("task-1", "queued", None),
            sample_video_task("", "running", None),
        ];

        let queue = normalize_video_task_queue(tasks);

        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].id, "task-1");
        assert_eq!(queue[0].status, "completed");
        assert_eq!(queue[1].id, "task-2");
    }

    #[test]
    fn normalized_video_task_queue_keeps_twenty_recent_tasks() {
        let tasks = (0..25)
            .map(|index| sample_video_task(&format!("task-{}", index), "completed", None))
            .collect::<Vec<_>>();

        let queue = normalize_video_task_queue(tasks);

        assert_eq!(queue.len(), 20);
        assert_eq!(queue[0].id, "task-0");
        assert_eq!(queue[19].id, "task-19");
    }

    #[test]
    fn should_poll_only_live_seedance_statuses() {
        let mut response = sample_video_task("task-1", "queued", None);

        assert!(should_poll_video_task(&response));
        response.status = "blocked".to_string();
        assert!(!should_poll_video_task(&response));
        response.status = "completed".to_string();
        response.preview_url = Some("https://cdn.example/video.mp4".to_string());
        assert!(!should_poll_video_task(&response));
    }

    fn sample_video_package() -> VideoPackageResponse {
        VideoPackageResponse {
            title: "云柑礼盒真实开箱".to_string(),
            hook: "办公室下午茶被它承包了".to_string(),
            oral_script: "这盒云柑礼盒适合送礼和办公室分享。".to_string(),
            storyboard: vec!["开箱".to_string(), "果肉特写".to_string()],
            subtitles: vec!["当季鲜果".to_string(), "顺丰冷链".to_string()],
            shot_prompts: vec!["真实办公室场景".to_string(), "自然光果肉特写".to_string()],
            tags: vec!["#云柑礼盒".to_string()],
            video_prompt: "真实办公室场景，云柑礼盒开箱，果肉特写。".to_string(),
            checks: vec![GraphicMarketingCheck {
                label: "人工审核".to_string(),
                status: "待确认".to_string(),
            }],
            agent_id: Some("agent-1".to_string()),
        }
    }

    fn sample_video_task(id: &str, status: &str, preview_url: Option<&str>) -> VideoTaskResponse {
        VideoTaskResponse {
            id: id.to_string(),
            provider: "volcengine-ark".to_string(),
            model: "doubao-seedance-2.0".to_string(),
            status: status.to_string(),
            message: status.to_string(),
            preview_url: preview_url.map(str::to_string),
            resolution: Some("720p".to_string()),
            ratio: Some("9:16".to_string()),
            duration_seconds: Some(8),
            queue_position: Some(1),
            submitted_at: Some("2026-06-03T00:00:00Z".to_string()),
            updated_at: Some("2026-06-03T00:00:00Z".to_string()),
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
}
