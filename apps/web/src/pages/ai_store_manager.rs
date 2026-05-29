use leptos::prelude::*;
use leptos::view;
use leptos_meta::Title;

use crate::i18n::I18nContext;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerMetric {
    pub label_key: &'static str,
    pub value: &'static str,
    pub trend_key: &'static str,
    pub tone: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerModule {
    pub title_key: &'static str,
    pub summary_key: &'static str,
    pub status_key: &'static str,
    pub icon: &'static str,
    pub action_key: &'static str,
    pub href: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerTask {
    pub title: &'static str,
    pub meta: &'static str,
    pub priority_key: &'static str,
}

const STORE_MANAGER_METRICS: [StoreManagerMetric; 4] = [
    StoreManagerMetric {
        label_key: "ai-store-manager-metric-reach",
        value: "1,286",
        trend_key: "ai-store-manager-metric-reach-trend",
        tone: "warning",
    },
    StoreManagerMetric {
        label_key: "ai-store-manager-metric-assets",
        value: "42",
        trend_key: "ai-store-manager-metric-assets-trend",
        tone: "danger",
    },
    StoreManagerMetric {
        label_key: "ai-store-manager-metric-leads",
        value: "86",
        trend_key: "ai-store-manager-metric-leads-trend",
        tone: "info",
    },
    StoreManagerMetric {
        label_key: "ai-store-manager-metric-revenue",
        value: "¥18,960",
        trend_key: "ai-store-manager-metric-revenue-trend",
        tone: "success",
    },
];

const STORE_MANAGER_MODULES: [StoreManagerModule; 3] = [
    StoreManagerModule {
        title_key: "ai-store-manager-video-marketing",
        summary_key: "ai-store-manager-video-desc",
        status_key: "ai-store-manager-video-core",
        icon: "🎬",
        action_key: "ai-store-manager-create-video",
        href: Some("/ai-store-manager/video-marketing"),
    },
    StoreManagerModule {
        title_key: "ai-store-manager-graphic-marketing",
        summary_key: "ai-store-manager-graphic-desc",
        status_key: "ai-store-manager-graphic-core",
        icon: "🖼️",
        action_key: "ai-store-manager-create-graphic",
        href: Some("/ai-store-manager/graphic-marketing"),
    },
    StoreManagerModule {
        title_key: "ai-store-manager-phone-marketing",
        summary_key: "ai-store-manager-phone-desc",
        status_key: "ai-store-manager-phone-core",
        icon: "📞",
        action_key: "ai-store-manager-create-phone",
        href: None,
    },
];

const STORE_MANAGER_TASKS: [StoreManagerTask; 4] = [
    StoreManagerTask {
        title: "3 条视频脚本待确认",
        meta: "抖音 · 新品种草",
        priority_key: "priority-high",
    },
    StoreManagerTask {
        title: "5 篇图文素材待审核",
        meta: "小红书 · 周末活动",
        priority_key: "priority-high",
    },
    StoreManagerTask {
        title: "120 位老客待生成电话话术",
        meta: "私域会员 · 复购提醒",
        priority_key: "priority-medium",
    },
    StoreManagerTask {
        title: "本周复购活动待选择人群",
        meta: "电话营销 · 本周",
        priority_key: "priority-medium",
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
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("ai-store-manager-title"))} />
        <div class="page ai-store-manager-page">
            <div class="page-header ai-store-manager-header">
                <div>
                    <h2>{move || i18n.get().t("ai-store-manager-title")}</h2>
                    <p>{move || i18n.get().t("ai-store-manager-subtitle")}</p>
                </div>
                <div class="ai-store-manager-actions">
                    <button class="btn btn-secondary">{move || i18n.get().t("ai-store-manager-import-products")}</button>
                    <button class="btn btn-primary">{move || i18n.get().t("ai-store-manager-create-task")}</button>
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
                        <h2>{move || i18n.get().t("ai-store-manager-marketing-entries")}</h2>
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
                        <h2>{move || i18n.get().t("ai-store-manager-todo")}</h2>
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
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));

    view! {
        <article class={format!("ai-store-manager-metric tone-{}", metric.tone)}>
            <span>{move || i18n.get().t(metric.label_key)}</span>
            <strong>{metric.value}</strong>
            <small>{move || i18n.get().t(metric.trend_key)}</small>
        </article>
    }
}

#[component]
fn ModuleCard(module: StoreManagerModule) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));

    let action = match module.href {
        Some(href) => view! {
            <a class="btn btn-secondary btn-block" href=href>{move || i18n.get().t(module.action_key)}</a>
        }
        .into_any(),
        None => view! {
            <button class="btn btn-secondary btn-block">{move || i18n.get().t(module.action_key)}</button>
        }
        .into_any(),
    };

    view! {
        <article class="ai-store-manager-module">
            <div class="ai-store-manager-module-head">
                <div class="ai-store-manager-module-icon">{module.icon}</div>
                <span class="status-badge status-pending">{move || i18n.get().t(module.status_key)}</span>
            </div>
            <h3>{move || i18n.get().t(module.title_key)}</h3>
            <p>{move || i18n.get().t(module.summary_key)}</p>
            {action}
        </article>
    }
}

#[component]
fn TaskItem(task: StoreManagerTask) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));

    let priority_class = if task.priority_key == "priority-high" {
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
            <span class=priority_class>{move || i18n.get().t(task.priority_key)}</span>
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
            .map(|module| module.title_key)
            .collect();

        assert_eq!(
            modules,
            vec![
                "ai-store-manager-video-marketing",
                "ai-store-manager-graphic-marketing",
                "ai-store-manager-phone-marketing"
            ]
        );

        let graphic = ai_store_manager_modules()
            .iter()
            .find(|module| module.title_key == "ai-store-manager-graphic-marketing")
            .expect("graphic marketing module exists");
        assert_eq!(graphic.href, Some("/ai-store-manager/graphic-marketing"));
    }
}
