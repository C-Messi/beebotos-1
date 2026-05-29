use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::Title;

use crate::api::{
    create_ai_store_manager_service, CreateGraphicImageRequest, CreateGraphicPackageRequest,
    GraphicImageResponse, GraphicMarketingCheck, GraphicPackageResponse,
};
use crate::state::use_app_state;
use crate::utils::event_target_value;

pub const GRAPHIC_PAGE_CLASS: &str = "page ai-graphic-marketing-page";
pub const GRAPHIC_WORKSPACE_CLASS: &str = "ai-graphic-workspace";
pub const GRAPHIC_PANEL_CLASS: &str = "ai-video-marketing-panel";
pub const GRAPHIC_IMAGE_PANEL_CLASS: &str = "ai-graphic-image-panel";
pub const GRAPHIC_IMAGE_PANEL_SECTION_CLASS: &str =
    "ai-video-marketing-section ai-graphic-image-panel";
pub const GRAPHIC_PREVIEW_CLASS: &str = "ai-graphic-preview";

#[derive(Clone, PartialEq, Eq)]
pub struct GraphicMarketingTask {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub price_range: String,
    pub platform: String,
    pub goal: String,
    pub style: String,
    pub size: String,
    pub quality: String,
}

pub fn default_graphic_marketing_task() -> GraphicMarketingTask {
    GraphicMarketingTask {
        product: "云柑礼盒".to_string(),
        selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
        audience: "25-40 岁办公室人群".to_string(),
        price_range: "99-199 元".to_string(),
        platform: "小红书".to_string(),
        goal: "新品种草".to_string(),
        style: "真实测评".to_string(),
        size: "1024x1536".to_string(),
        quality: "medium".to_string(),
    }
}

pub fn fallback_graphic_package(task: &GraphicMarketingTask) -> GraphicPackageResponse {
    GraphicPackageResponse {
        title_options: vec![
            format!("{}真实体验，{}也会想收藏", task.product, task.audience),
            format!("{}｜{}入手前先看这篇", task.product, task.selling_points),
            format!("{}场景里的{}", task.goal, task.product),
        ],
        body: format!(
            "{}适合{}，主打{}，价格区间{}。内容按{}风格展开，先讲场景，再讲卖点，\
             最后引导评论咨询。",
            task.product, task.audience, task.selling_points, task.price_range, task.style
        ),
        moments_copy: format!(
            "{}到了，{}。想了解规格和到手时间可以留言。",
            task.product, task.selling_points
        ),
        poster_copy: format!(
            "{}\n{}\n{}",
            task.product, task.selling_points, task.price_range
        ),
        comment_guide: format!(
            "评论告诉我你的使用场景，我帮你判断{}是否合适。",
            task.product
        ),
        image_prompt: format!(
            "为{}生成{}海报，平台{}，突出{}，风格{}，商品主体清晰。",
            task.product, task.goal, task.platform, task.selling_points, task.style
        ),
        checks: vec![
            GraphicMarketingCheck {
                label: "商品卖点完整".to_string(),
                status: "已覆盖".to_string(),
            },
            GraphicMarketingCheck {
                label: "人工审核".to_string(),
                status: "待确认".to_string(),
            },
        ],
    }
}

fn graphic_image_src(image: &GraphicImageResponse) -> Option<String> {
    image
        .b64_json
        .as_ref()
        .map(|b64| format!("data:image/png;base64,{}", b64))
        .or_else(|| image.image_url.clone())
}

fn graphic_image_download_href(image: &GraphicImageResponse) -> Option<String> {
    graphic_image_src(image)
}

fn graphic_image_download_filename(image: &GraphicImageResponse) -> String {
    format!("{}.png", image.id.replace('/', "-"))
}

fn can_generate_graphic_image(
    package_ready: bool,
    package_loading: bool,
    image_loading: bool,
) -> bool {
    package_ready && !package_loading && !image_loading
}

fn graphic_task_matches_snapshot(
    current: &GraphicMarketingTask,
    snapshot: &GraphicMarketingTask,
) -> bool {
    current == snapshot
}

fn graphic_image_request_matches_task_and_prompt(
    current: &GraphicMarketingTask,
    current_prompt: &str,
    req: &CreateGraphicImageRequest,
) -> bool {
    current.product == req.product
        && current.platform == req.platform
        && current.size == req.size
        && current.quality == req.quality
        && current_prompt == req.prompt
}

#[cfg(test)]
fn graphic_workspace_child_classes() -> [&'static str; 3] {
    [
        GRAPHIC_PANEL_CLASS,
        GRAPHIC_PANEL_CLASS,
        GRAPHIC_IMAGE_PANEL_SECTION_CLASS,
    ]
}

#[component]
pub fn AiGraphicMarketingPage() -> impl IntoView {
    let (task, set_task) = signal(default_graphic_marketing_task());
    let initial_package = fallback_graphic_package(&task.get_untracked());
    let (package, set_package) = signal(initial_package);
    let (package_ready, set_package_ready) = signal(false);
    let (task_status, set_task_status) = signal("待生成".to_string());
    let (package_error, set_package_error) = signal::<Option<String>>(None);
    let (package_loading, set_package_loading) = signal(false);
    let (image_result, set_image_result) = signal::<Option<GraphicImageResponse>>(None);
    let (image_error, set_image_error) = signal::<Option<String>>(None);
    let (image_loading, set_image_loading) = signal(false);
    let (image_preview_src, set_image_preview_src) = signal::<Option<String>>(None);
    let service = create_ai_store_manager_service(use_app_state().api_client());
    let service = StoredValue::new(service);
    let busy = Signal::derive(move || package_loading.get() || image_loading.get());
    let image_action_enabled = Signal::derive(move || {
        can_generate_graphic_image(
            package_ready.get(),
            package_loading.get(),
            image_loading.get(),
        )
    });

    let generate_package = move || {
        if busy.get_untracked() {
            return;
        }

        let service = service.get_value();
        let current_task = task.get();
        let task_snapshot = current_task.clone();
        let req = CreateGraphicPackageRequest {
            product: current_task.product.clone(),
            selling_points: current_task.selling_points.clone(),
            audience: current_task.audience.clone(),
            price_range: current_task.price_range.clone(),
            platform: current_task.platform.clone(),
            goal: current_task.goal.clone(),
            style: current_task.style.clone(),
        };

        set_package_loading.set(true);
        set_package_error.set(None);
        set_package_ready.set(false);
        set_image_result.set(None);
        set_image_error.set(None);
        set_image_preview_src.set(None);
        set_task_status.set("图文包生成中".to_string());

        spawn_local(async move {
            let response = service.create_graphic_package(&req).await;
            if !graphic_task_matches_snapshot(&task.get_untracked(), &task_snapshot) {
                set_package_ready.set(false);
                set_package_loading.set(false);
                return;
            }

            match response {
                Ok(created) => {
                    set_package.set(created);
                    set_package_ready.set(true);
                    set_task_status.set("待审核".to_string());
                }
                Err(err) => {
                    set_package_error.set(Some(err.to_string()));
                    set_task_status.set("生成失败".to_string());
                }
            }
            set_package_loading.set(false);
        });
    };

    let generate_image = move || {
        if busy.get_untracked() {
            return;
        }
        if !package_ready.get_untracked() {
            set_image_error.set(Some("请先重新生成图文包。".to_string()));
            set_task_status.set("待生成".to_string());
            return;
        }

        let service = service.get_value();
        let current_task = task.get();
        let current_package = package.get();
        let image_prompt = current_package.image_prompt.clone();
        let req = CreateGraphicImageRequest {
            product: current_task.product.clone(),
            platform: current_task.platform.clone(),
            prompt: image_prompt,
            size: current_task.size.clone(),
            quality: current_task.quality.clone(),
        };

        set_image_loading.set(true);
        set_image_error.set(None);
        set_image_result.set(None);
        set_image_preview_src.set(None);
        set_task_status.set("图片生成中".to_string());

        spawn_local(async move {
            let response = service.create_graphic_image(&req).await;
            if !graphic_image_request_matches_task_and_prompt(
                &task.get_untracked(),
                &package.get_untracked().image_prompt,
                &req,
            ) {
                set_image_loading.set(false);
                return;
            }

            match response {
                Ok(created) => {
                    set_image_result.set(Some(created));
                    set_task_status.set("图片已生成".to_string());
                }
                Err(err) => {
                    set_image_error.set(Some(err.to_string()));
                    set_task_status.set("图片生成失败".to_string());
                }
            }
            set_image_loading.set(false);
        });
    };

    view! {
        <Title text="AI 图文营销 - BeeBotOS" />
        <div class=GRAPHIC_PAGE_CLASS>
            <div class="page-header ai-video-marketing-header">
                <div>
                    <h2>"AI 图文营销"</h2>
                    <p>"从商品卖点生成小红书、朋友圈和海报素材。"</p>
                </div>
                <div class="ai-store-manager-actions">
                    <a class="btn btn-secondary" href="/ai-store-manager">"返回 AI 店长"</a>
                    <button class="btn btn-primary" disabled=move || busy.get() on:click=move |_| generate_package()>
                        {move || if package_loading.get() { "生成中..." } else { "生成图文包" }}
                    </button>
                    <button class="btn btn-secondary" disabled=move || !image_action_enabled.get() on:click=move |_| generate_image()>
                        {move || if image_loading.get() { "生成中..." } else { "生成图片" }}
                    </button>
                </div>
            </div>

            <section class=GRAPHIC_WORKSPACE_CLASS>
                <div class=GRAPHIC_PANEL_CLASS>
                    <div class="section-title compact">
                        <h2>"任务配置"</h2>
                    </div>
                    <div class="ai-video-status-row">
                        <span>"当前状态"</span>
                        <strong>{move || task_status.get()}</strong>
                    </div>
                    <div class="ai-video-form-grid">
                        <TextField label="商品" value=Signal::derive(move || task.get().product) on_input=move |value| {
                            set_task.update(|task| task.product = value);
                            let current_task = task.get_untracked();
                            set_package.set(fallback_graphic_package(&current_task));
                            set_package_ready.set(false);
                            set_image_result.set(None);
                            set_image_error.set(None);
                            set_task_status.set("待生成".to_string());
                        } />
                        <TextField label="核心卖点" value=Signal::derive(move || task.get().selling_points) on_input=move |value| {
                            set_task.update(|task| task.selling_points = value);
                            let current_task = task.get_untracked();
                            set_package.set(fallback_graphic_package(&current_task));
                            set_package_ready.set(false);
                            set_image_result.set(None);
                            set_image_error.set(None);
                            set_task_status.set("待生成".to_string());
                        } />
                        <TextField label="目标人群" value=Signal::derive(move || task.get().audience) on_input=move |value| {
                            set_task.update(|task| task.audience = value);
                            let current_task = task.get_untracked();
                            set_package.set(fallback_graphic_package(&current_task));
                            set_package_ready.set(false);
                            set_image_result.set(None);
                            set_image_error.set(None);
                            set_task_status.set("待生成".to_string());
                        } />
                        <TextField label="价格区间" value=Signal::derive(move || task.get().price_range) on_input=move |value| {
                            set_task.update(|task| task.price_range = value);
                            let current_task = task.get_untracked();
                            set_package.set(fallback_graphic_package(&current_task));
                            set_package_ready.set(false);
                            set_image_result.set(None);
                            set_image_error.set(None);
                            set_task_status.set("待生成".to_string());
                        } />
                        <SelectField
                            label="发布平台"
                            value=Signal::derive(move || task.get().platform)
                            options=vec!["小红书", "朋友圈"]
                            on_change=move |value| {
                                set_task.update(|task| task.platform = value);
                                let current_task = task.get_untracked();
                                set_package.set(fallback_graphic_package(&current_task));
                                set_package_ready.set(false);
                                set_image_result.set(None);
                                set_image_error.set(None);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="营销目标"
                            value=Signal::derive(move || task.get().goal)
                            options=vec!["新品种草", "促销转化", "老客复购", "私域引流"]
                            on_change=move |value| {
                                set_task.update(|task| task.goal = value);
                                let current_task = task.get_untracked();
                                set_package.set(fallback_graphic_package(&current_task));
                                set_package_ready.set(false);
                                set_image_result.set(None);
                                set_image_error.set(None);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="内容风格"
                            value=Signal::derive(move || task.get().style)
                            options=vec!["真实测评", "情绪种草", "痛点解决", "礼赠场景"]
                            on_change=move |value| {
                                set_task.update(|task| task.style = value);
                                let current_task = task.get_untracked();
                                set_package.set(fallback_graphic_package(&current_task));
                                set_package_ready.set(false);
                                set_image_result.set(None);
                                set_image_error.set(None);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="图片尺寸"
                            value=Signal::derive(move || task.get().size)
                            options=vec!["1024x1536", "1024x1024", "1536x1024"]
                            on_change=move |value| {
                                set_task.update(|task| task.size = value);
                                let current_task = task.get_untracked();
                                set_package.set(fallback_graphic_package(&current_task));
                                set_package_ready.set(false);
                                set_image_result.set(None);
                                set_image_error.set(None);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="图片质量"
                            value=Signal::derive(move || task.get().quality)
                            options=vec!["medium", "low", "high"]
                            on_change=move |value| {
                                set_task.update(|task| task.quality = value);
                                let current_task = task.get_untracked();
                                set_package.set(fallback_graphic_package(&current_task));
                                set_package_ready.set(false);
                                set_image_result.set(None);
                                set_image_error.set(None);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                    </div>
                </div>

                <div class=GRAPHIC_PANEL_CLASS>
                    <div class="section-title compact">
                        <h2>"图文营销包"</h2>
                    </div>
                    {move || package_error.get().map(|error| view! {
                        <div class="ai-video-task-card error">{error}</div>
                    })}
                    <GraphicPackageCard package=Signal::derive(move || package.get()) />
                </div>

                <section class=GRAPHIC_IMAGE_PANEL_SECTION_CLASS>
                    <div class="section-title compact">
                        <h2>"图片素材"</h2>
                    </div>
                    {move || {
                        if let Some(image) = image_result.get() {
                            view! {
                                <GraphicImageCard
                                    image=image
                                    on_preview=move |src| set_image_preview_src.set(Some(src))
                                />
                            }.into_any()
                        } else if let Some(error) = image_error.get() {
                            view! { <div class="ai-video-task-card error">{error}</div> }.into_any()
                        } else {
                            view! {
                                <div class="ai-video-task-card">
                                    "点击生成图片后，会使用当前图文包的图片 prompt 调用图片中转站。"
                                </div>
                            }.into_any()
                        }
                    }}
                </section>
            </section>

            {move || image_preview_src.get().map(|src| view! {
                <div class="ai-graphic-modal">
                    <div class="ai-graphic-modal-content">
                        <button
                            type="button"
                            class="btn btn-secondary ai-graphic-modal-close"
                            on:click=move |_| set_image_preview_src.set(None)
                        >
                            "关闭"
                        </button>
                        <img src=src alt="AI 图文营销大图预览" />
                    </div>
                </div>
            })}

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"发布前检查"</h2>
                </div>
                <div class="ai-video-check-grid">
                    {move || package
                        .get()
                        .checks
                        .into_iter()
                        .map(|check| view! { <CheckItem check=check /> })
                        .collect_view()}
                </div>
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
fn GraphicPackageCard(package: Signal<GraphicPackageResponse>) -> impl IntoView {
    view! {
        <div class="ai-video-result-list">
            <article class="ai-video-result-item">
                <span>"标题选项"</span>
                <p>{move || package.get().title_options.join(" / ")}</p>
            </article>
            <article class="ai-video-result-item">
                <span>"小红书正文"</span>
                <p>{move || package.get().body}</p>
            </article>
            <article class="ai-video-result-item">
                <span>"朋友圈文案"</span>
                <p>{move || package.get().moments_copy}</p>
            </article>
            <article class="ai-video-result-item">
                <span>"海报文案"</span>
                <p>{move || package.get().poster_copy}</p>
            </article>
            <article class="ai-video-result-item">
                <span>"评论引导"</span>
                <p>{move || package.get().comment_guide}</p>
            </article>
            <article class="ai-video-result-item">
                <span>"图片 Prompt"</span>
                <p>{move || package.get().image_prompt}</p>
            </article>
        </div>
    }
}

#[component]
fn GraphicImageCard(
    image: GraphicImageResponse,
    on_preview: impl Fn(String) + Clone + 'static,
) -> impl IntoView {
    let src = graphic_image_src(&image);
    let download_href = graphic_image_download_href(&image);
    let download_filename = graphic_image_download_filename(&image);

    view! {
        <article class=format!("ai-video-task-card {}", GRAPHIC_PREVIEW_CLASS)>
            <p>{image.message}</p>
            {src.map(|src| {
                let preview_src = src.clone();
                let on_preview = on_preview.clone();
                view! {
                    <button
                        type="button"
                        class="ai-graphic-image-open"
                        on:click=move |_| on_preview(preview_src.clone())
                    >
                        <img src=src alt="AI 图文营销图片" loading="lazy" />
                    </button>
                }
            })}
            <div class="ai-graphic-image-actions">
                {download_href.map(|href| view! {
                    <a class="btn btn-secondary" href=href download=download_filename>"下载图片"</a>
                })}
            </div>
        </article>
    }
}

#[component]
fn CheckItem(check: GraphicMarketingCheck) -> impl IntoView {
    view! {
        <article class="ai-video-check-item">
            <span>{check.label}</span>
            <strong>{check.status}</strong>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_graphic_task_targets_xhs() {
        let task = default_graphic_marketing_task();

        assert_eq!(task.product, "云柑礼盒");
        assert_eq!(task.platform, "小红书");
        assert_eq!(task.size, "1024x1536");
    }

    #[test]
    fn graphic_layout_classes_match_css() {
        assert_eq!(GRAPHIC_PAGE_CLASS, "page ai-graphic-marketing-page");
        assert_eq!(GRAPHIC_WORKSPACE_CLASS, "ai-graphic-workspace");
        assert_eq!(GRAPHIC_IMAGE_PANEL_CLASS, "ai-graphic-image-panel");
        assert_eq!(GRAPHIC_PREVIEW_CLASS, "ai-graphic-preview");
    }

    #[test]
    fn graphic_workspace_children_include_image_panel_as_third_column() {
        assert_eq!(
            graphic_workspace_child_classes(),
            [
                "ai-video-marketing-panel",
                "ai-video-marketing-panel",
                "ai-video-marketing-section ai-graphic-image-panel"
            ]
        );
    }

    #[test]
    fn fallback_package_contains_prompt_and_titles() {
        let task = default_graphic_marketing_task();
        let package = fallback_graphic_package(&task);

        assert_eq!(package.title_options.len(), 3);
        assert!(package.body.contains("云柑礼盒"));
        assert!(package.image_prompt.contains("小红书"));
    }

    #[test]
    fn image_src_prefers_b64_and_falls_back_to_url() {
        let with_b64 = GraphicImageResponse {
            id: "img-1".to_string(),
            provider: "relay".to_string(),
            status: "completed".to_string(),
            message: "ok".to_string(),
            image_url: Some("https://cdn.example/image.png".to_string()),
            b64_json: Some("abc123".to_string()),
        };
        let with_url = GraphicImageResponse {
            b64_json: None,
            ..with_b64.clone()
        };
        let empty = GraphicImageResponse {
            image_url: None,
            b64_json: None,
            ..with_b64.clone()
        };

        assert_eq!(
            graphic_image_src(&with_b64),
            Some("data:image/png;base64,abc123".to_string())
        );
        assert_eq!(
            graphic_image_src(&with_url),
            Some("https://cdn.example/image.png".to_string())
        );
        assert_eq!(graphic_image_src(&empty), None);
    }

    #[test]
    fn image_download_uses_preview_source_and_png_filename() {
        let image = GraphicImageResponse {
            id: "graphic-image-小红书-云柑礼盒".to_string(),
            provider: "relay".to_string(),
            status: "completed".to_string(),
            message: "ok".to_string(),
            image_url: None,
            b64_json: Some("abc123".to_string()),
        };

        assert_eq!(
            graphic_image_download_href(&image),
            Some("data:image/png;base64,abc123".to_string())
        );
        assert_eq!(
            graphic_image_download_filename(&image),
            "graphic-image-小红书-云柑礼盒.png"
        );
    }

    #[test]
    fn image_generation_requires_ready_package_and_idle_state() {
        assert!(can_generate_graphic_image(true, false, false));
        assert!(!can_generate_graphic_image(false, false, false));
        assert!(!can_generate_graphic_image(true, true, false));
        assert!(!can_generate_graphic_image(true, false, true));
    }

    #[test]
    fn graphic_task_matches_snapshot_rejects_stale_package_response() {
        let snapshot = default_graphic_marketing_task();
        let mut changed = snapshot.clone();
        changed.product = "山茶礼盒".to_string();

        assert!(graphic_task_matches_snapshot(&snapshot, &snapshot));
        assert!(!graphic_task_matches_snapshot(&changed, &snapshot));
    }

    #[test]
    fn graphic_image_request_matches_task_and_prompt_rejects_stale_response() {
        let task = default_graphic_marketing_task();
        let package = fallback_graphic_package(&task);
        let req = CreateGraphicImageRequest {
            product: task.product.clone(),
            platform: task.platform.clone(),
            prompt: package.image_prompt.clone(),
            size: task.size.clone(),
            quality: task.quality.clone(),
        };
        let mut changed_task = task.clone();
        changed_task.platform = "朋友圈".to_string();

        assert!(graphic_image_request_matches_task_and_prompt(
            &task,
            &package.image_prompt,
            &req
        ));
        assert!(!graphic_image_request_matches_task_and_prompt(
            &changed_task,
            &package.image_prompt,
            &req
        ));
        assert!(!graphic_image_request_matches_task_and_prompt(
            &task,
            "new prompt",
            &req
        ));
    }
}
