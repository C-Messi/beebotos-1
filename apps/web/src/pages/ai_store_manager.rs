use leptos::prelude::*;
use leptos::view;
use leptos_meta::Title;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerMetric {
    pub label: &'static str,
    pub value: &'static str,
    pub trend: &'static str,
    pub tone: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerModule {
    pub title: &'static str,
    pub summary: &'static str,
    pub status: &'static str,
    pub icon: &'static str,
    pub action: &'static str,
    pub href: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerTask {
    pub title: &'static str,
    pub meta: &'static str,
    pub priority: &'static str,
}

const STORE_MANAGER_METRICS: [StoreManagerMetric; 4] = [
    StoreManagerMetric {
        label: "今日触达",
        value: "1,286",
        trend: "覆盖 3 个渠道",
        tone: "warning",
    },
    StoreManagerMetric {
        label: "生成素材",
        value: "42",
        trend: "12 条待审核",
        tone: "danger",
    },
    StoreManagerMetric {
        label: "转化线索",
        value: "86",
        trend: "23 位高意向",
        tone: "info",
    },
    StoreManagerMetric {
        label: "预计成交",
        value: "¥18,960",
        trend: "ROI 3.4",
        tone: "success",
    },
];

const STORE_MANAGER_MODULES: [StoreManagerModule; 3] = [
    StoreManagerModule {
        title: "AI 视频营销",
        summary: "按商品、卖点和平台生成短视频脚本、分镜、标题、口播词和字幕。",
        status: "核心能力",
        icon: "🎬",
        action: "创建视频任务",
        href: Some("/ai-store-manager/video-marketing"),
    },
    StoreManagerModule {
        title: "AI 图文营销",
        summary: "生成种草文案、朋友圈内容、海报文案和商品详情优化建议。",
        status: "核心能力",
        icon: "🖼️",
        action: "创建图文任务",
        href: None,
    },
    StoreManagerModule {
        title: "AI 电话营销",
        summary: "面向老客复购、活动通知和高意向线索生成外呼话术与跟进任务。",
        status: "核心能力",
        icon: "📞",
        action: "创建外呼任务",
        href: None,
    },
];

const STORE_MANAGER_TASKS: [StoreManagerTask; 4] = [
    StoreManagerTask {
        title: "3 条视频脚本待确认",
        meta: "抖音 · 新品种草",
        priority: "高",
    },
    StoreManagerTask {
        title: "5 篇图文素材待审核",
        meta: "小红书 · 周末活动",
        priority: "高",
    },
    StoreManagerTask {
        title: "120 位老客待生成电话话术",
        meta: "私域会员 · 复购提醒",
        priority: "中",
    },
    StoreManagerTask {
        title: "本周复购活动待选择人群",
        meta: "电话营销 · 本周",
        priority: "中",
    },
];

pub fn ai_store_manager_modules() -> &'static [StoreManagerModule] {
    &STORE_MANAGER_MODULES
}

fn store_manager_metrics() -> &'static [StoreManagerMetric] {
    &STORE_MANAGER_METRICS
}

fn store_manager_tasks() -> &'static [StoreManagerTask] {
    &STORE_MANAGER_TASKS
}

#[component]
pub fn AiStoreManagerPage() -> impl IntoView {
    view! {
        <Title text="AI 店长 - BeeBotOS" />
        <div class="page ai-store-manager-page">
            <div class="page-header ai-store-manager-header">
                <div>
                    <h2>"AI 店长"</h2>
                    <p>"用 AI 批量生成视频、图文和电话营销任务。"</p>
                </div>
                <div class="ai-store-manager-actions">
                    <button class="btn btn-secondary">"导入商品"</button>
                    <button class="btn btn-primary">"创建营销任务"</button>
                </div>
            </div>

            <section class="ai-store-manager-metrics">
                {store_manager_metrics()
                    .iter()
                    .copied()
                    .map(|metric| view! { <MetricCard metric=metric /> })
                    .collect_view()}
            </section>

            <section class="ai-store-manager-layout">
                <div class="ai-store-manager-main">
                    <div class="section-title compact">
                        <h2>"营销入口"</h2>
                    </div>
                    <div class="ai-store-manager-module-grid">
                        {ai_store_manager_modules()
                            .iter()
                            .copied()
                            .map(|module| view! { <ModuleCard module=module /> })
                            .collect_view()}
                    </div>
                </div>

                <aside class="ai-store-manager-side">
                    <div class="section-title compact">
                        <h2>"营销待办"</h2>
                    </div>
                    <div class="ai-store-manager-task-list">
                        {store_manager_tasks()
                            .iter()
                            .copied()
                            .map(|task| view! { <TaskItem task=task /> })
                            .collect_view()}
                    </div>
                </aside>
            </section>
        </div>
    }
}

#[component]
fn MetricCard(metric: StoreManagerMetric) -> impl IntoView {
    view! {
        <article class={format!("ai-store-manager-metric tone-{}", metric.tone)}>
            <span>{metric.label}</span>
            <strong>{metric.value}</strong>
            <small>{metric.trend}</small>
        </article>
    }
}

#[component]
fn ModuleCard(module: StoreManagerModule) -> impl IntoView {
    let action = match module.href {
        Some(href) => view! {
            <a class="btn btn-secondary btn-block" href=href>{module.action}</a>
        }
        .into_any(),
        None => view! {
            <button class="btn btn-secondary btn-block">{module.action}</button>
        }
        .into_any(),
    };

    view! {
        <article class="ai-store-manager-module">
            <div class="ai-store-manager-module-head">
                <div class="ai-store-manager-module-icon">{module.icon}</div>
                <span class="status-badge status-pending">{module.status}</span>
            </div>
            <h3>{module.title}</h3>
            <p>{module.summary}</p>
            {action}
        </article>
    }
}

#[component]
fn TaskItem(task: StoreManagerTask) -> impl IntoView {
    let priority_class = if task.priority == "高" {
        "priority high"
    } else {
        "priority medium"
    };

    view! {
        <article class="ai-store-manager-task">
            <div>
                <h3>{task.title}</h3>
                <p>{task.meta}</p>
            </div>
            <span class=priority_class>{task.priority}</span>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_manager_modules_cover_marketing_channels() {
        let modules: Vec<_> = ai_store_manager_modules()
            .iter()
            .map(|module| module.title)
            .collect();

        assert_eq!(modules, vec!["AI 视频营销", "AI 图文营销", "AI 电话营销"]);
    }
}
