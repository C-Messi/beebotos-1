use leptos::prelude::*;
use leptos::view;
use leptos_meta::Title;

use crate::i18n::I18nContext;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StoreManagerModule {
    pub title_key: &'static str,
    pub summary_key: &'static str,
    pub status_key: &'static str,
    pub icon: &'static str,
    pub action_key: &'static str,
    pub href: Option<&'static str>,
}

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

pub fn ai_store_manager_modules() -> &'static [StoreManagerModule] {
    &STORE_MANAGER_MODULES
}

#[component]
pub fn AiStoreManagerPage() -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("ai-store-manager-title"))} />
        <div class="page ai-store-manager-page">
            <section class="ai-store-manager-layout">
                <div class="ai-store-manager-main">
                    <div class="ai-store-manager-module-grid">
                        {ai_store_manager_modules()
                            .iter()
                            .copied()
                            .map(|module| view! { <ModuleCard module=module /> })
                            .collect_view()}
                    </div>
                </div>

            </section>
        </div>
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
