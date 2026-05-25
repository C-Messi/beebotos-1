use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::Title;

use crate::api::{
    create_ai_store_manager_service, create_client, CreateVideoTaskRequest, VideoTaskResponse,
};
use crate::utils::event_target_value;

#[derive(Clone, PartialEq, Eq)]
pub struct VideoMarketingResult {
    pub label: &'static str,
    pub content: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VideoMarketingVersion {
    pub name: &'static str,
    pub focus: &'static str,
    pub hook: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VideoMarketingCheck {
    pub label: &'static str,
    pub status: &'static str,
}

#[derive(Clone, PartialEq, Eq)]
pub struct VideoMarketingTask {
    pub product: String,
    pub selling_points: String,
    pub audience: String,
    pub goal: String,
    pub platform: String,
    pub duration: String,
    pub style: String,
}

const VIDEO_VERSIONS: [VideoMarketingVersion; 3] = [
    VideoMarketingVersion {
        name: "强转化版",
        focus: "突出限时优惠和立即下单理由。",
        hook: "今天下单，周末前把清甜送到家。",
    },
    VideoMarketingVersion {
        name: "种草版",
        focus: "突出真实开箱、试吃反馈和生活场景。",
        hook: "这盒云柑，是办公室最容易被分完的水果。",
    },
    VideoMarketingVersion {
        name: "直播引流版",
        focus: "突出直播间福利和互动口令。",
        hook: "今晚直播间拍，送礼盒包装和试吃装。",
    },
];

const PRE_PUBLISH_CHECKS: [VideoMarketingCheck; 4] = [
    VideoMarketingCheck {
        label: "商品卖点完整",
        status: "已覆盖",
    },
    VideoMarketingCheck {
        label: "行动引导明确",
        status: "已覆盖",
    },
    VideoMarketingCheck {
        label: "平台风格匹配",
        status: "已覆盖",
    },
    VideoMarketingCheck {
        label: "人工审核",
        status: "待确认",
    },
];

pub fn video_marketing_versions() -> &'static [VideoMarketingVersion] {
    &VIDEO_VERSIONS
}

pub fn default_video_marketing_task() -> VideoMarketingTask {
    VideoMarketingTask {
        product: "云柑礼盒".to_string(),
        selling_points: "当季鲜果、顺丰冷链、送礼体面".to_string(),
        audience: "25-40 岁办公室人群".to_string(),
        goal: "新品种草".to_string(),
        platform: "抖音".to_string(),
        duration: "30 秒".to_string(),
        style: "真实测评".to_string(),
    }
}

pub fn generate_video_marketing_package(
    task: &VideoMarketingTask,
    version: &str,
) -> Vec<VideoMarketingResult> {
    let (title, hook, angle) = match version {
        "强转化版" => (
            format!(
                "{}限时福利，适合{}的下单理由来了。",
                task.product, task.audience
            ),
            format!(
                "今天下单，{}把{}直接送到家。",
                task.product, task.selling_points
            ),
            "突出优惠、信任背书和立即行动",
        ),
        "直播引流版" => (
            format!("{}直播间专属福利，今晚别错过。", task.product),
            format!("进直播间看{}实拍，福利只留给在线的人。", task.product),
            "突出直播间福利、互动口令和限时节奏",
        ),
        _ => (
            format!("{}真实体验，{}也会愿意转发。", task.product, task.audience),
            format!("别再只看参数了，先看{}真实开箱。", task.product),
            "突出真实体验、使用场景和自然种草",
        ),
    };

    vec![
        VideoMarketingResult {
            label: "爆款标题",
            content: title,
        },
        VideoMarketingResult {
            label: "3 秒钩子",
            content: hook,
        },
        VideoMarketingResult {
            label: "口播脚本",
            content: format!(
                "这次给{}做一条{}短视频，面向{}，主打{}。开头先给真实场景，再展示核心卖点，\
                 最后明确引导用户行动。",
                task.product, task.platform, task.audience, task.selling_points
            ),
        },
        VideoMarketingResult {
            label: "分镜脚本",
            content: format!(
                "场景痛点 -> {}开箱 -> 卖点特写 -> 使用反馈 -> {}行动引导。",
                task.product, task.platform
            ),
        },
        VideoMarketingResult {
            label: "字幕文案",
            content: format!(
                "{} / {} / {} / {}",
                task.product, task.selling_points, angle, task.duration
            ),
        },
        VideoMarketingResult {
            label: "话题标签",
            content: format!(
                "#{} #{} #{} #AI视频营销",
                task.product, task.platform, task.goal
            ),
        },
    ]
}

fn pre_publish_checks() -> &'static [VideoMarketingCheck] {
    &PRE_PUBLISH_CHECKS
}

fn build_seedance_prompt(task: &VideoMarketingTask, results: &[VideoMarketingResult]) -> String {
    let script = results
        .iter()
        .map(|result| format!("{}：{}", result.label, result.content))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "为{}生成{}营销短视频，平台：{}，风格：{}，目标：{}。\n{}",
        task.product, task.duration, task.platform, task.style, task.goal, script
    )
}

#[component]
pub fn AiVideoMarketingPage() -> impl IntoView {
    let (task, set_task) = signal(default_video_marketing_task());
    let (selected_version, set_selected_version) = signal("种草版".to_string());
    let (task_status, set_task_status) = signal("待生成".to_string());
    let initial_results = generate_video_marketing_package(&task.get_untracked(), "种草版");
    let (results, set_results) = signal(initial_results);
    let (video_task, set_video_task) = signal::<Option<VideoTaskResponse>>(None);
    let (video_error, set_video_error) = signal::<Option<String>>(None);
    let (video_loading, set_video_loading) = signal(false);
    let video_service = create_ai_store_manager_service(create_client());
    let video_service = StoredValue::new(video_service);

    let generate = move || {
        let package = generate_video_marketing_package(&task.get(), &selected_version.get());
        set_results.set(package);
        set_task_status.set("待审核".to_string());
    };

    let create_video = move || {
        let service = video_service.get_value();
        let current_task = task.get();
        let current_results = results.get();
        let version = selected_version.get();
        let req = CreateVideoTaskRequest {
            product: current_task.product.clone(),
            platform: current_task.platform.clone(),
            version,
            prompt: build_seedance_prompt(&current_task, &current_results),
        };

        set_video_loading.set(true);
        set_video_error.set(None);
        set_video_task.set(None);
        set_task_status.set("视频生成中".to_string());

        spawn_local(async move {
            match service.create_video_task(&req).await {
                Ok(created) => match service.get_video_task(&created.id).await {
                    Ok(status) => {
                        set_video_task.set(Some(status));
                        set_task_status.set("视频已生成".to_string());
                    }
                    Err(err) => {
                        set_video_error.set(Some(err.to_string()));
                        set_task_status.set("生成失败".to_string());
                    }
                },
                Err(err) => {
                    set_video_error.set(Some(err.to_string()));
                    set_task_status.set("生成失败".to_string());
                }
            }
            set_video_loading.set(false);
        });
    };

    view! {
        <Title text="AI 视频营销 - BeeBotOS" />
        <div class="page ai-video-marketing-page">
            <div class="page-header ai-video-marketing-header">
                <div>
                    <h2>"AI 视频营销"</h2>
                    <p>"从商品卖点生成短视频脚本、分镜、口播、字幕和发布素材。"</p>
                </div>
                <div class="ai-store-manager-actions">
                    <a class="btn btn-secondary" href="/ai-store-manager">"返回 AI 店长"</a>
                    <button class="btn btn-primary" on:click=move |_| generate()>
                        "生成视频文案包"
                    </button>
                    <button class="btn btn-secondary" disabled=move || video_loading.get() on:click=move |_| create_video()>
                        {move || if video_loading.get() { "生成中..." } else { "生成视频" }}
                    </button>
                </div>
            </div>

            <section class="ai-video-marketing-workspace">
                <div class="ai-video-marketing-panel">
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
                            set_task_status.set("待生成".to_string());
                        } />
                        <TextField label="核心卖点" value=Signal::derive(move || task.get().selling_points) on_input=move |value| {
                            set_task.update(|task| task.selling_points = value);
                            set_task_status.set("待生成".to_string());
                        } />
                        <TextField label="目标人群" value=Signal::derive(move || task.get().audience) on_input=move |value| {
                            set_task.update(|task| task.audience = value);
                            set_task_status.set("待生成".to_string());
                        } />
                        <SelectField
                            label="营销目标"
                            value=Signal::derive(move || task.get().goal)
                            options=vec!["新品种草", "促销转化", "老客复购", "直播预热"]
                            on_change=move |value| {
                                set_task.update(|task| task.goal = value);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="发布平台"
                            value=Signal::derive(move || task.get().platform)
                            options=vec!["抖音", "快手", "视频号", "小红书"]
                            on_change=move |value| {
                                set_task.update(|task| task.platform = value);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="视频时长"
                            value=Signal::derive(move || task.get().duration)
                            options=vec!["15 秒", "30 秒", "60 秒"]
                            on_change=move |value| {
                                set_task.update(|task| task.duration = value);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                        <SelectField
                            label="内容风格"
                            value=Signal::derive(move || task.get().style)
                            options=vec!["真实测评", "情绪种草", "痛点解决", "价格促销"]
                            on_change=move |value| {
                                set_task.update(|task| task.style = value);
                                set_task_status.set("待生成".to_string());
                            }
                        />
                    </div>
                </div>

                <div class="ai-video-marketing-panel">
                    <div class="section-title compact">
                        <h2>"AI 生成结果"</h2>
                    </div>
                    <div class="ai-video-result-list">
                        {move || results
                            .get()
                            .into_iter()
                            .map(|result| view! { <ResultItem result=result /> })
                            .collect_view()}
                    </div>
                </div>
            </section>

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"素材版本"</h2>
                </div>
                <div class="ai-video-version-grid">
                    {video_marketing_versions()
                        .iter()
                        .copied()
                        .map(|version| {
                            view! {
                                <VersionCard
                                    version=version
                                    selected_version=selected_version
                                    on_select=move |name| {
                                        set_selected_version.set(name.to_string());
                                        let package = generate_video_marketing_package(&task.get(), name);
                                        set_results.set(package);
                                        set_task_status.set("待审核".to_string());
                                    }
                                />
                            }
                        })
                        .collect_view()}
                </div>
            </section>

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"Seedance 任务"</h2>
                </div>
                {move || {
                    if let Some(task) = video_task.get() {
                        view! { <VideoTaskCard task=task /> }.into_any()
                    } else if let Some(error) = video_error.get() {
                        view! { <div class="ai-video-task-card error">{error}</div> }.into_any()
                    } else {
                        view! {
                            <div class="ai-video-task-card">
                                "点击生成视频后，会先创建 Seedance 预留任务；当前使用 mock provider。"
                            </div>
                        }.into_any()
                    }
                }}
            </section>

            <section class="ai-video-marketing-section">
                <div class="section-title compact">
                    <h2>"发布前检查"</h2>
                </div>
                <div class="ai-video-check-grid">
                    {pre_publish_checks()
                        .iter()
                        .copied()
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
fn ResultItem(result: VideoMarketingResult) -> impl IntoView {
    view! {
        <article class="ai-video-result-item">
            <span>{result.label}</span>
            <p>{result.content}</p>
        </article>
    }
}

#[component]
fn VersionCard(
    version: VideoMarketingVersion,
    selected_version: ReadSignal<String>,
    on_select: impl Fn(&'static str) + Clone + 'static,
) -> impl IntoView {
    let class_name = move || {
        if selected_version.get() == version.name {
            "ai-video-version-card selected"
        } else {
            "ai-video-version-card"
        }
    };

    view! {
        <button class=class_name on:click=move |_| on_select(version.name)>
            <h3>{version.name}</h3>
            <p>{version.focus}</p>
            <strong>{version.hook}</strong>
        </button>
    }
}

#[component]
fn VideoTaskCard(task: VideoTaskResponse) -> impl IntoView {
    view! {
        <article class="ai-video-task-card">
            <div>
                <span>"任务 ID"</span>
                <strong>{task.id}</strong>
            </div>
            <div>
                <span>"Provider"</span>
                <strong>{task.provider}</strong>
            </div>
            <div>
                <span>"状态"</span>
                <strong>{task.status}</strong>
            </div>
            <p>{task.message}</p>
            {task.preview_url.map(|url| view! {
                <a class="btn btn-secondary" href=url target="_blank" rel="noopener noreferrer">"查看预览"</a>
            })}
        </article>
    }
}

#[component]
fn CheckItem(check: VideoMarketingCheck) -> impl IntoView {
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
    fn video_marketing_versions_cover_core_use_cases() {
        let versions: Vec<_> = video_marketing_versions()
            .iter()
            .map(|version| version.name)
            .collect();

        assert_eq!(versions, vec!["强转化版", "种草版", "直播引流版"]);
    }

    #[test]
    fn generated_package_changes_with_version() {
        let task = default_video_marketing_task();

        let conversion = generate_video_marketing_package(&task, "强转化版");
        let live = generate_video_marketing_package(&task, "直播引流版");

        assert_ne!(conversion[0].content, live[0].content);
        assert!(live[1].content.contains("直播间"));
    }

    #[test]
    fn seedance_prompt_includes_task_and_script() {
        let task = default_video_marketing_task();
        let results = generate_video_marketing_package(&task, "种草版");
        let prompt = build_seedance_prompt(&task, &results);

        assert!(prompt.contains("云柑礼盒"));
        assert!(prompt.contains("爆款标题"));
        assert!(prompt.contains("抖音"));
    }
}
