//! Skills Marketplace Page
//!
//! Browse, install, and manage WASM skills from ClawHub/BeeHub or local
//! registry.

use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

use crate::api::{ApiService, InstallSkillRequest, SkillCategory, SkillInfo};
use crate::components::{Modal, StarRating};
use crate::i18n::I18nContext;
use crate::state::use_app_state;

#[component]
pub fn SkillsPage() -> impl IntoView {
    let app_state = use_app_state();
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let search_input = RwSignal::new(String::new());
    let active_search = RwSignal::new(String::new());
    // "all" | "local" | "clawhub" | "beehub"
    let selected_tab = RwSignal::new("local".to_string());
    let selected_category = RwSignal::new(None::<SkillCategory>);
    let show_details = RwSignal::new(None::<SkillInfo>);

    // Refresh counter to force LocalResource re-evaluation
    let refresh_counter = RwSignal::new(0u64);

    // Fetch skills - use LocalResource for CSR
    let skills = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let _trigger = refresh_counter.get();
            let service = app_state.skill_service();
            let tab = selected_tab.get();
            let search = active_search.get();
            let app_state = app_state.clone();
            async move {
                app_state.loading().skills.set(true);
                let hub = if tab == "all" || tab == "local" {
                    None
                } else {
                    Some(tab.as_str())
                };
                let result = service.list(hub, Some(&search)).await;
                app_state.loading().skills.set(false);
                result
            }
        }
    });

    // Helper to reload skills after install/uninstall or search/hub change
    let reload_skills = {
        let skills = skills.clone();
        let refresh_counter = refresh_counter.clone();
        move || {
            refresh_counter.update(|n| *n += 1);
            skills.refetch();
        }
    };

    let perform_search = {
        let active_search = active_search.clone();
        let search_input = search_input.clone();
        let reload = reload_skills.clone();
        move || {
            active_search.set(search_input.get());
            reload();
        }
    };

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("skills-title"))} />
        <div class="page skills-page">
            <div class="page-header">
                <div>
                    <h1>{move || i18n.get().t("skills-title")}</h1>
                    <p class="page-description">{move || i18n.get().t("skills-subtitle")}</p>
                </div>
            </div>

            <section class="skills-controls">
                // === Hub Selector ===
                <div class="hub-selector">
                    <HubButton
                        label={move || i18n.get().t("skills-tab-all")}
                        is_active={
                            let selected = selected_tab.clone();
                            move || selected.get() == "all"
                        }
                        on_click={
                            let selected = selected_tab.clone();
                            let reload = reload_skills.clone();
                            move || { selected.set("all".to_string()); reload(); }
                        }
                    />
                    <HubButton
                        label={move || i18n.get().t("skills-tab-local")}
                        is_active={
                            let selected = selected_tab.clone();
                            move || selected.get() == "local"
                        }
                        on_click={
                            let selected = selected_tab.clone();
                            let reload = reload_skills.clone();
                            move || { selected.set("local".to_string()); reload(); }
                        }
                    />
                    <HubButton
                        label={move || i18n.get().t("skills-tab-clawhub")}
                        is_active={
                            let selected = selected_tab.clone();
                            move || selected.get() == "clawhub"
                        }
                        on_click={
                            let selected = selected_tab.clone();
                            let reload = reload_skills.clone();
                            move || { selected.set("clawhub".to_string()); reload(); }
                        }
                    />
                </div>

                // === Search Bar with Button ===
                <div class="search-bar">
                    <input
                        type="text"
                        placeholder={move || i18n.get().t("skills-search-placeholder")}
                        prop:value=search_input
                        on:input=move |e| search_input.set(event_target_value(&e))
                        on:keyup=move |e| {
                            if e.key() == "Enter" {
                                perform_search();
                            }
                        }
                    />
                    <button class="search-btn" on:click=move |_| perform_search()>
                        <span class="search-icon">"🔍"</span>
                        {move || i18n.get().t("skills-search")}
                    </button>
                </div>

                <div class="category-filters">
                    <CategoryFilter
                        label={move || i18n.get().t("skills-category-all")}
                        is_active={
                            let selected = selected_category;
                            move || selected.get().is_none()
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(None)
                        }
                    />
                    <CategoryFilter
                        label={move || i18n.get().t("skills-category-trading")}
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Trading)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Trading))
                        }
                    />
                    <CategoryFilter
                        label={move || i18n.get().t("skills-category-data")}
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Data)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Data))
                        }
                    />
                    <CategoryFilter
                        label={move || i18n.get().t("skills-category-social")}
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Social)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Social))
                        }
                    />
                    <CategoryFilter
                        label={move || i18n.get().t("skills-category-automation")}
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Automation)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Automation))
                        }
                    />
                    <CategoryFilter
                        label={move || i18n.get().t("skills-category-analysis")}
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Analysis)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Analysis))
                        }
                    />
                </div>
            </section>

            {move || {
                match skills.get() {
                    None => view! { <SkillsLoading tab=selected_tab.get()/> }.into_any(),
                    Some(Ok(data)) => {
                        let filtered: Vec<_> = data.into_iter()
                            .filter(|s| {
                                let matches_category = selected_category.with(|c| {
                                    c.as_ref().map(|cat| {
                                        let tag = format!("{:?}", cat).to_lowercase();
                                        s.tags.iter().any(|t| t.to_lowercase() == tag) ||
                                        s.capabilities.iter().any(|cap| cap.to_lowercase().contains(&tag))
                                    }).unwrap_or(true)
                                });
                                matches_category
                            })
                            .collect();

                        if filtered.is_empty() {
                            view! { <SkillsEmpty tab=selected_tab.get() search=active_search.get()/> }.into_any()
                        } else {
                            let reload = reload_skills.clone();
                            let selected_tab = selected_tab.clone();
                            let on_show_details = {
                                let show_details = show_details.clone();
                                move |s: SkillInfo| show_details.set(Some(s))
                            };
                            view! {
                                <div class="skills-grid">
                                    <For
                                        each=move || filtered.clone()
                                        key=|skill: &SkillInfo| skill.id.clone()
                                        let:skill
                                    >
                                        <SkillCard skill=skill reload=reload.clone() selected_tab=selected_tab.clone() on_show_details=on_show_details.clone() />
                                    </For>
                                </div>
                            }.into_any()
                        }
                    }
                    Some(Err(e)) => view! { <SkillsError message=e.to_string() /> }.into_any(),
                }
            }}

            // === Skill Detail Modal ===
            {move || show_details.get().map(|skill| {
                view! {
                    <SkillDetailModal skill=skill on_close=move || show_details.set(None)/>
                }
            })}
        </div>
    }
}

#[component]
fn HubButton(
    #[prop(into)] label: Signal<String>,
    is_active: impl Fn() -> bool + Clone + Send + Sync + 'static,
    on_click: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class=move || format!("hub-btn {}", if is_active() { "active" } else { "" })
            on:click=move |_| on_click()
        >
            {move || label.get()}
        </button>
    }
}

#[component]
fn CategoryFilter(
    #[prop(into)] label: Signal<String>,
    is_active: impl Fn() -> bool + Clone + Send + Sync + 'static,
    on_click: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class=move || format!("category-filter {}", if is_active() { "active" } else { "" })
            on:click=move |_| on_click()
        >
            {move || label.get()}
        </button>
    }
}

#[component]
fn SkillsGrid(
    skills: Vec<SkillInfo>,
    reload: impl Fn() + Clone + Send + Sync + 'static,
    selected_tab: RwSignal<String>,
    on_show_details: impl Fn(SkillInfo) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <div class="skills-grid">
            {skills.into_iter().map(|skill| {
                view! {
                    <SkillCard skill=skill reload=reload.clone() selected_tab=selected_tab.clone() on_show_details=on_show_details.clone() />
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn SkillCard(
    #[prop(into)] skill: SkillInfo,
    reload: impl Fn() + Clone + Send + Sync + 'static,
    selected_tab: RwSignal<String>,
    on_show_details: impl Fn(SkillInfo) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let app_state = use_app_state();
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let is_installing = RwSignal::new(false);
    let is_uninstalling = RwSignal::new(false);

    // Capture values from skill prop upfront
    let skill_id = skill.id.clone();
    let skill_name = skill.name.clone();
    let skill_desc = skill.description.clone();
    let skill_author = skill.author.clone();
    let skill_version = skill.version.clone();
    let is_installed = skill.installed;
    let tab = selected_tab.get();
    let is_hub = tab == "clawhub" || tab == "beehub";
    let hub_url = if tab == "clawhub" {
        format!("https://clawhub.ai/skills/{}", skill_id)
    } else if tab == "beehub" {
        format!("https://beehub.io/skills/{}", skill_id)
    } else {
        String::new()
    };

    let category_icon = if skill.tags.iter().any(|t| t.to_lowercase() == "trading")
        || skill.capabilities.iter().any(|c| c.to_lowercase().contains("trade"))
    {
        "📈"
    } else if skill.tags.iter().any(|t| t.to_lowercase() == "data")
        || skill.capabilities.iter().any(|c| c.to_lowercase().contains("data"))
    {
        "📊"
    } else if skill.tags.iter().any(|t| t.to_lowercase() == "social")
        || skill.capabilities.iter().any(|c| c.to_lowercase().contains("social"))
    {
        "💬"
    } else if skill.tags.iter().any(|t| t.to_lowercase() == "automation")
        || skill.capabilities.iter().any(|c| c.to_lowercase().contains("auto"))
    {
        "⚙️"
    } else if skill.tags.iter().any(|t| t.to_lowercase() == "analysis")
        || skill.capabilities.iter().any(|c| c.to_lowercase().contains("analy"))
    {
        "🔍"
    } else {
        "📦"
    };

    view! {
        <div class="card skill-card">
            <div class="skill-header">
                <div class="skill-icon">{category_icon}</div>
                <div class="skill-meta">
                    <h3>{skill_name.clone()}</h3>
                    <div class="skill-stats">
                        <span class="skill-version">{format!("v{}", skill_version)}</span>
                    </div>
                </div>
                {if is_installed {
                    view! {
                        <span class="installed-badge">{format!("✓ {}", i18n.get().t("skill-installed"))}</span>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </div>

            <p class="skill-description">{skill_desc.clone()}</p>

            <div class="skill-footer">
                <span class="skill-author">{format!("by {}", skill_author)}</span>
                <div class="skill-actions">
                    <button
                        class="btn btn-secondary btn-sm"
                        on:click={let skill=skill.clone();move|_|on_show_details(skill.clone())}
                    >
                        {move || i18n.get().t("skill-details")}
                    </button>
                    {if is_installed {
                        let app_state = app_state.clone();
                        let skill_id = skill_id.clone();
                        let skill_name = skill_name.clone();
                        let reload = reload.clone();
                        let i18n_ctx = i18n.get();
                        view! {
                            <button
                                class="btn btn-danger btn-sm"
                                disabled=move || is_uninstalling.get()
                                on:click=move |_| {
                                    is_uninstalling.set(true);
                                    let service = app_state.skill_service();
                                    let app_state = app_state.clone();
                                    let skill_name = skill_name.clone();
                                    let skill_id = skill_id.clone();
                                    let reload = reload.clone();
                                    let i18n_ctx = i18n_ctx.clone();
                                    leptos::task::spawn_local(async move {
                                        match service.uninstall(&skill_id).await {
                                            Ok(()) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    &i18n_ctx.t("notification-success"),
                                                    format!("{} {}", skill_name, i18n_ctx.t("skill-uninstall")),
                                                );
                                                app_state.skill_service().client().clear_cache();
                                                reload();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    &i18n_ctx.t("notification-error"),
                                                    format!("{} {}: {}", i18n_ctx.t("skill-uninstall"), skill_name, e),
                                                );
                                            }
                                        }
                                        is_uninstalling.set(false);
                                    });
                                }
                            >
                                {move || if is_uninstalling.get() {
                                    i18n.get().t("skill-removing")
                                } else {
                                    i18n.get().t("skill-uninstall")
                                }}
                            </button>
                        }.into_any()
                    } else if is_hub {
                        let app_state = app_state.clone();
                        let skill_id = skill_id.clone();
                        let skill_name = skill_name.clone();
                        let reload = reload.clone();
                        let tab = tab.clone();
                        let hub_url = hub_url.clone();
                        let i18n_ctx = i18n.get();
                        view! {
                            <>
                                <a
                                    class="btn btn-primary btn-sm"
                                    href=hub_url
                                    target="_blank"
                                >
                                    {move || i18n.get().t("skill-view-on-hub")}
                                </a>
                                <button
                                    class="btn btn-success btn-sm"
                                    disabled=move || is_installing.get()
                                    on:click=move |_| {
                                        is_installing.set(true);
                                        let service = app_state.skill_service();
                                        let app_state = app_state.clone();
                                        let skill_name = skill_name.clone();
                                        let skill_id = skill_id.clone();
                                        let reload = reload.clone();
                                        let tab = tab.clone();
                                        let i18n_ctx = i18n_ctx.clone();
                                        leptos::task::spawn_local(async move {
                                            let req = InstallSkillRequest {
                                                source: skill_id.clone(),
                                                agent_id: None,
                                                version: None,
                                                hub: Some(tab).filter(|h| !h.is_empty()),
                                            };
                                            match service.install(req).await {
                                                Ok(resp) => {
                                                    app_state.notify(
                                                        crate::state::notification::NotificationType::Success,
                                                        &i18n_ctx.t("notification-success"),
                                                        format!("{} {}", resp.name, i18n_ctx.t("skill-install")),
                                                    );
                                                    app_state.skill_service().client().clear_cache();
                                                    reload();
                                                }
                                                Err(e) => {
                                                    app_state.notify(
                                                        crate::state::notification::NotificationType::Error,
                                                        &i18n_ctx.t("notification-error"),
                                                        format!("{} {}: {}", i18n_ctx.t("skill-install"), skill_name, e),
                                                    );
                                                }
                                            }
                                            is_installing.set(false);
                                        });
                                    }
                                >
                                    {move || if is_installing.get() {
                                        i18n.get().t("skill-installing")
                                    } else {
                                        i18n.get().t("skill-install")
                                    }}
                                </button>
                            </>
                        }.into_any()
                    } else {
                        let app_state = app_state.clone();
                        let skill_id = skill_id.clone();
                        let skill_name = skill_name.clone();
                        let reload = reload.clone();
                        let tab = tab.clone();
                        let i18n_ctx = i18n.get();
                        view! {
                            <button
                                class="btn btn-primary btn-sm"
                                disabled=move || is_installing.get()
                                on:click=move |_| {
                                    is_installing.set(true);
                                    let service = app_state.skill_service();
                                    let app_state = app_state.clone();
                                    let skill_name = skill_name.clone();
                                    let skill_id = skill_id.clone();
                                    let reload = reload.clone();
                                    let tab = tab.clone();
                                    let i18n_ctx = i18n_ctx.clone();
                                    leptos::task::spawn_local(async move {
                                        let req = InstallSkillRequest {
                                            source: skill_id.clone(),
                                            agent_id: None,
                                            version: None,
                                            hub: Some(tab).filter(|h| !h.is_empty()),
                                        };
                                        match service.install(req).await {
                                            Ok(resp) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    &i18n_ctx.t("notification-success"),
                                                    format!("{} {}", resp.name, i18n_ctx.t("skill-install")),
                                                );
                                                app_state.skill_service().client().clear_cache();
                                                reload();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    &i18n_ctx.t("notification-error"),
                                                    format!("{} {}: {}", i18n_ctx.t("skill-install"), skill_name, e),
                                                );
                                            }
                                        }
                                        is_installing.set(false);
                                    });
                                }
                            >
                                {move || if is_installing.get() {
                                    i18n.get().t("skill-installing")
                                } else {
                                    i18n.get().t("skill-install")
                                }}
                            </button>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

// === Skill Detail Modal ===
#[component]
fn SkillDetailModal(
    #[prop(into)] skill: SkillInfo,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    view! {
        <Modal title=skill.name.clone() on_close=move || on_close()>
            <div class="modal-body skill-detail-body">
                <div class="detail-row">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-version"))}</span>
                    <span class="detail-value">{format!("v{}", skill.version)}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-author"))}</span>
                    <span class="detail-value">{skill.author.clone()}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-license"))}</span>
                    <span class="detail-value">{skill.license.clone()}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-downloads"))}</span>
                    <span class="detail-value">{skill.downloads.to_string()}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-rating"))}</span>
                    <span class="detail-value"><StarRating rating=skill.rating as f64 />{format!(" {}  ", skill.rating)}</span>
                </div>
                <div class="detail-section">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-description"))}</span>
                    <p class="detail-description">{skill.description.clone()}</p>
                </div>
                <div class="detail-section">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-capabilities"))}</span>
                    <div class="detail-tags">
                        {if skill.capabilities.is_empty() {
                            view! { <span class="tag empty">{move || i18n.get().t("skill-none-listed")}</span> }.into_any()
                        } else {
                            skill.capabilities.iter().map(|c| {
                                view! { <span class="tag capability">{c.clone()}</span> }
                            }).collect::<Vec<_>>().into_any()
                        }}
                    </div>
                </div>
                <div class="detail-section">
                    <span class="detail-label">{move || format!("{}:", i18n.get().t("skill-tags"))}</span>
                    <div class="detail-tags">
                        {if skill.tags.is_empty() {
                            view! { <span class="tag empty">{move || i18n.get().t("skill-none-listed")}</span> }.into_any()
                        } else {
                            skill.tags.iter().map(|t| {
                                view! { <span class="tag">{t.clone()}</span> }
                            }).collect::<Vec<_>>().into_any()
                        }}
                    </div>
                </div>
            </div>
        </Modal>
    }
}

#[component]
fn SkillsLoading(#[prop(default = String::new())] tab: String) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let loading_text = match tab.as_str() {
        "clawhub" => "正在从 ClawHub 加载技能...",
        "beehub" => "正在从 BeeHub 加载技能...",
        _ => "正在加载技能...",
    };
    view! {
        <div class="skills-loading-container">
            <div class="skills-loading-spinner">
                <div class="spinner"></div>
                <p class="skills-loading-text">{loading_text}</p>
            </div>
            <div class="skills-grid">
                <div class="card skill-card skeleton">
                    <div class="skeleton-header"></div>
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
                <div class="card skill-card skeleton">
                    <div class="skeleton-header"></div>
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
                <div class="card skill-card skeleton">
                    <div class="skeleton-header"></div>
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
                <div class="card skill-card skeleton">
                    <div class="skeleton-header"></div>
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn SkillsEmpty(
    #[prop(default = String::new())] tab: String,
    #[prop(default = String::new())] search: String,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    view! {
        <div class="empty-state">
            <div class="empty-icon">"📦"</div>
            {match tab.as_str() {
                "clawhub" | "beehub" if search.is_empty() => view! {
                    <>
                        <h3>{i18n.t("skills-search-hint")}</h3>
                        <p>{i18n.t("skills-try-different")}</p>
                    </>
                }.into_any(),
                "clawhub" | "beehub" => view! {
                    <>
                        <h3>{i18n.t("skills-no-results")}</h3>
                        <p>{i18n.t("skills-try-different")}</p>
                    </>
                }.into_any(),
                _ => view! {
                    <>
                        <h3>{i18n.t("skills-no-skills-found")}</h3>
                        <p>{i18n.t("skills-adjust-search")}</p>
                    </>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn SkillsError(
    #[prop(into)] message: String,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let is_hub_unavailable =
        message.contains("502") || message.contains("503") || message.contains("unavailable");
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>{i18n.t("skills-failed-load")}</h3>
            {if is_hub_unavailable {
                view! {
                    <>
                        <p>{i18n.t("skills-hub-unreachable")}</p>
                        <p class="text-muted">{i18n.t("skills-check-network")}</p>
                    </>
                }.into_any()
            } else {
                view! { <p>{message}</p> }.into_any()
            }}
            <button
                class="btn btn-primary"
                on:click=move |_| {
                    let window = web_sys::window().expect("window not available");
                    let _ = window.location().reload();
                }
            >
                {i18n.t("skills-retry")}
            </button>
        </div>
    }
}
