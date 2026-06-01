use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;

use crate::api::{Settings as ApiSettings, SettingsService, Theme};
use crate::i18n::I18nContext;
use crate::state::use_app_state;
use crate::utils::{
    event_target_checked, event_target_value, use_theme, FormValidator, StringValidators,
};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let app_state = use_app_state();
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let theme_manager = use_theme();
    let settings = app_state.settings();
    let saving = RwSignal::new(false);
    let save_message = RwSignal::new(None::<String>);
    let validator = RwSignal::new(FormValidator::new());
    let error_message = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);

    // Form signals — local frontend-only settings
    let theme = RwSignal::new(Theme::Dark);
    let language = RwSignal::new("en".to_string());
    let notifications_enabled = RwSignal::new(true);
    let auto_update = RwSignal::new(true);
    let api_endpoint = RwSignal::new(String::new());
    let wallet_address = RwSignal::new(String::new());

    // Load from backend or localStorage fallback
    let app_state_for_load = app_state.clone();
    Effect::new(move |_| {
        let app_state = app_state_for_load.clone();
        loading.set(true);
        spawn_local(async move {
            let service = SettingsService::new(app_state.api_client());
            match service.get().await {
                Ok(s) => {
                    app_state.settings.set(s);
                    loading.set(false);
                }
                Err(_) => {
                    // Fallback to localStorage
                    let result: Result<String, _> =
                        gloo_storage::LocalStorage::get("beebotos_settings");
                    if let Ok(stored) = result {
                        if let Ok(parsed) = serde_json::from_str::<ApiSettings>(&stored) {
                            app_state.settings.set(parsed);
                        }
                    }
                    loading.set(false);
                }
            }
        });
    });

    // Sync form signals from app state
    Effect::new(move |_| {
        let s = settings.get();
        theme.set(s.theme.clone());
        language.set(s.language.clone());
        notifications_enabled.set(s.notifications_enabled);
        auto_update.set(s.auto_update);
        api_endpoint.set(s.api_endpoint.clone().unwrap_or_default());
        wallet_address.set(s.wallet_address.clone().unwrap_or_default());
    });

    // Apply theme when changed
    Effect::new(move |_| {
        let t = theme.get();
        theme_manager.set_theme(t);
    });

    let validate = move || {
        let mut v = FormValidator::new();

        // Validate API endpoint if provided
        if !api_endpoint.get().is_empty() {
            v.validate(StringValidators::url("api_endpoint", &api_endpoint.get()));
        }

        // Validate wallet address if provided
        if !wallet_address.get().is_empty() {
            v.validate(StringValidators::ethereum_address(
                "wallet_address",
                &wallet_address.get(),
            ));
        }

        validator.set(v.clone());
        v.is_valid()
    };

    let on_save = move || {
        if !validate() {
            return;
        }

        let new_settings = ApiSettings {
            theme: theme.get(),
            language: language.get(),
            notifications_enabled: notifications_enabled.get(),
            auto_update: auto_update.get(),
            api_endpoint: if api_endpoint.get().is_empty() {
                None
            } else {
                Some(api_endpoint.get())
            },
            wallet_address: if wallet_address.get().is_empty() {
                None
            } else {
                Some(wallet_address.get())
            },
        };

        saving.set(true);
        save_message.set(None);
        error_message.set(None);

        // Save to backend and localStorage
        let app_state = app_state.clone();
        let settings_for_storage = new_settings.clone();
        spawn_local(async move {
            let service = SettingsService::new(app_state.api_client());
            match service.update(&settings_for_storage).await {
                Ok(_) => {
                    app_state.settings.set(settings_for_storage.clone());
                    let _ = gloo_storage::LocalStorage::set(
                        "beebotos_settings",
                        serde_json::to_string(&settings_for_storage).unwrap_or_default(),
                    );
                    saving.set(false);
                    save_message.set(Some("Settings saved successfully".to_string()));
                }
                Err(e) => {
                    // Fallback: save to localStorage
                    let _ = gloo_storage::LocalStorage::set(
                        "beebotos_settings",
                        serde_json::to_string(&settings_for_storage).unwrap_or_default(),
                    );
                    app_state.settings.set(settings_for_storage);
                    saving.set(false);
                    save_message.set(Some(format!("Saved locally (backend: {})", e)));
                }
            }
        });

        // Clear message after 3 seconds
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            save_message.set(None);
        });
    };
    let on_save_stored = StoredValue::new(on_save);

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("settings-page-title"))} />
        <div class="page settings-page">
            <div class="page-header">
                <h1>{move || i18n.get().t("settings-page-title")}</h1>
                <p class="page-description">{move || i18n.get().t("settings-page-subtitle")}</p>
            </div>

            {move || if loading.get() {
                view! {
                    <div class="loading-state">
                        <div class="spinner"></div>
                        <p>{move || i18n.get().t("settings-loading")}</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <>
                        {move || error_message.get().map(|msg| view! {
                            <div class="alert alert-error">{msg}</div>
                        })}

                        <div class="settings-grid">
                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-appearance")}</h2>

                                <div class="form-group">
                                    <label>{move || i18n.get().t("settings-theme")}</label>
                                    <div class="theme-selector">
                                        <ThemeOption
                                            label={move || i18n.get().t("settings-dark")}
                                            value=Theme::Dark
                                            current=theme
                                            icon="🌙"
                                        />
                                        <ThemeOption
                                            label={move || i18n.get().t("settings-light")}
                                            value=Theme::Light
                                            current=theme
                                            icon="☀️"
                                        />
                                        <ThemeOption
                                            label={move || i18n.get().t("settings-system")}
                                            value=Theme::System
                                            current=theme
                                            icon="💻"
                                        />
                                    </div>
                                </div>

                                <div class="form-group">
                                    <label>{move || i18n.get().t("settings-language")}</label>
                                    <select
                                        prop:value=language
                                        on:change=move |e| language.set(event_target_value(&e))
                                    >
                                        <option value="en">{move || i18n.get().t("settings-english")}</option>
                                        <option value="zh">{move || i18n.get().t("settings-chinese")}</option>
                                        <option value="ja">{move || i18n.get().t("settings-japanese")}</option>
                                        <option value="ko">{move || i18n.get().t("settings-korean")}</option>
                                    </select>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-notifications")}</h2>

                                <div class="form-group checkbox-group">
                                    <label class="checkbox-label">
                                        <input
                                            type="checkbox"
                                            prop:checked=notifications_enabled
                                            on:change=move |e| notifications_enabled.set(event_target_checked(&e))
                                        />
                                        <span>{move || i18n.get().t("settings-enable-notifications")}</span>
                                    </label>
                                    <p class="form-help">{move || i18n.get().t("settings-notifications-hint")}</p>
                                </div>

                                <div class="form-group checkbox-group">
                                    <label class="checkbox-label">
                                        <input
                                            type="checkbox"
                                            prop:checked=auto_update
                                            on:change=move |e| auto_update.set(event_target_checked(&e))
                                        />
                                        <span>{move || i18n.get().t("settings-auto-update")}</span>
                                    </label>
                                    <p class="form-help">{move || i18n.get().t("settings-auto-update-hint")}</p>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-network")}</h2>

                                <div class=move || format!("form-group {}",
                                    if validator.get().has_error("api_endpoint") { "has-error" } else { "" })>
                                    <label>{move || i18n.get().t("settings-api-endpoint")}</label>
                                    <input
                                        type="text"
                                        placeholder="https://api.beebotos.dev"
                                        prop:value=api_endpoint
                                        on:input=move |e| {
                                            api_endpoint.set(event_target_value(&e));
                                            validator.update(|v| {
                                                if !api_endpoint.get().is_empty() {
                                                    v.validate(StringValidators::url("api_endpoint", &api_endpoint.get()));
                                                }
                                            });
                                        }
                                    />
                                    <p class="form-help">{move || i18n.get().t("settings-api-endpoint-hint")}</p>
                                    {move || validator.get().first_error_message("api_endpoint").map(|msg| view! {
                                        <span class="form-error">{msg}</span>
                                    })}
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-wallet")}</h2>

                                <div class=move || format!("form-group {}",
                                    if validator.get().has_error("wallet_address") { "has-error" } else { "" })>
                                    <label>{move || i18n.get().t("settings-wallet-address")}</label>
                                    <input
                                        type="text"
                                        placeholder={move || i18n.get().t("settings-wallet-placeholder")}
                                        prop:value=wallet_address
                                        on:input=move |e| {
                                            wallet_address.set(event_target_value(&e));
                                            validator.update(|v| {
                                                if !wallet_address.get().is_empty() {
                                                    v.validate(StringValidators::ethereum_address("wallet_address", &wallet_address.get()));
                                                }
                                            });
                                        }
                                    />
                                    <p class="form-help">{move || i18n.get().t("settings-wallet-hint")}</p>
                                    {move || validator.get().first_error_message("wallet_address").map(|msg| view! {
                                        <span class="form-error">{msg}</span>
                                    })}
                                </div>

                                <div class="wallet-actions">
                                    <button class="btn btn-secondary">{move || i18n.get().t("settings-connect-wallet")}</button>
                                    <button class="btn btn-secondary">{move || i18n.get().t("settings-disconnect")}</button>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-ai-config")}</h2>
                                <p class="form-help">{move || i18n.get().t("settings-ai-config-hint")}</p>
                                <button
                                    class="btn btn-secondary"
                                    on:click=move |_| {
                                        let navigate = leptos_router::hooks::use_navigate();
                                        navigate("/llm-config", Default::default());
                                    }
                                >
                                    {move || i18n.get().t("settings-open-llm")}
                                </button>
                            </section>

                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-gateway")}</h2>
                                <p class="form-help">{move || i18n.get().t("settings-gateway-hint")}</p>
                                <button
                                    class="btn btn-secondary"
                                    on:click=move |_| {
                                        let navigate = leptos_router::hooks::use_navigate();
                                        navigate("/settings/wizard", Default::default());
                                    }
                                >
                                    {move || i18n.get().t("settings-open-wizard")}
                                </button>
                            </section>

                            <section class="card settings-section">
                                <h2>{move || i18n.get().t("settings-system")}</h2>

                                <div class="system-info">
                                    <div class="info-row">
                                        <span>{move || i18n.get().t("settings-version")}</span>
                                        <span>"v2.0.0"</span>
                                    </div>
                                    <div class="info-row">
                                        <span>{move || i18n.get().t("settings-build")}</span>
                                        <span>"release-2024.03.22"</span>
                                    </div>
                                    <div class="info-row">
                                        <span>{move || i18n.get().t("settings-platform")}</span>
                                        <span>"WebAssembly"</span>
                                    </div>
                                </div>

                                <div class="system-actions">
                                    <button class="btn btn-secondary">{move || i18n.get().t("settings-check-updates")}</button>
                                    <button
                                        class="btn btn-secondary"
                                        on:click=move |_| {
                                            let client = crate::api::create_client();
                                            spawn_local(async move {
                                                match client.post::<serde_json::Value, _>("/admin/config/reload", &serde_json::json!({})).await {
                                                    Ok(resp) => {
                                                        let msg = resp.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| use_context::<I18nContext>().expect("i18n context not found").t("settings-config-reloaded"));
                                                        save_message.set(Some(msg.to_string()));
                                                    }
                                                    Err(e) => {
                                                        error_message.set(Some(format!("Reload failed: {}", e)));
                                                    }
                                                }
                                            });
                                        }
                                    >
                                        {move || i18n.get().t("settings-reload-config")}
                                    </button>
                                    <button class="btn btn-danger">{move || i18n.get().t("settings-reset-defaults")}</button>
                                </div>
                            </section>
                        </div>

                        <div class="settings-footer">
                            {move || save_message.get().map(|msg| view! {
                                <div class="save-message success">{msg}</div>
                            })}
                            {move || error_message.get().map(|msg| view! {
                                <div class="save-message error">{msg}</div>
                            })}

                            <div class="settings-actions">
                                <button
                                    class="btn btn-primary"
                                    on:click=move |_| on_save_stored.get_value()()
                                    disabled=saving
                                >
                                    {move || if saving.get() { i18n.get().t("settings-saving") } else { i18n.get().t("settings-save-changes") }}
                                </button>
                            </div>
                        </div>
                    </>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn ThemeOption(
    label: impl IntoView,
    value: Theme,
    current: RwSignal<Theme>,
    #[prop(into)] icon: String,
) -> impl IntoView {
    let value_for_check = value.clone();
    let is_selected = move || current.get() == value_for_check;

    view! {
        <button
            class=move || format!("theme-option {}", if is_selected() { "selected" } else { "" })
            on:click=move |_| current.set(value.clone())
        >
            <span class="theme-icon">{icon}</span>
            <span class="theme-label">{label}</span>
        </button>
    }
}
