//! LLM Model Settings Page
//!
//! Allows users to select and configure the active LLM model provider,
//! choose model versions (e.g. kimi-k2.6 thinking/fast), and hot-reload config.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;

use crate::api::{LlmGlobalConfig, UpdateLlmConfigRequest};
use crate::components::InlineLoading;
use crate::state::use_app_state;
use crate::i18n::I18nContext;

/// Predefined model options for thinking-capable providers.
/// `thinking` maps directly to the backend TOML value.
const KIMI_MODELS: &[(&str, &str, f32, &str, Option<&str>)] = &[
    ("kimi-k2.6", "kimi-k2.6 思考版", 1.0, "enabled", None),
    ("kimi-k2.6", "kimi-k2.6 快速版", 0.6, "disabled", None),
    ("kimi-k2.5", "kimi-k2.5 思考版", 1.0, "enabled", None),
    ("kimi-k2.5", "kimi-k2.5 快速版", 0.6, "disabled", None),
];

const DEEPSEEK_MODELS: &[(&str, &str, f32, &str, Option<&str>)] = &[
    (
        "deepseek-v4-flash",
        "DeepSeek V4 Flash 思考版",
        0.7,
        "enabled",
        Some("high"),
    ),
    (
        "deepseek-v4-flash",
        "DeepSeek V4 Flash 非思考版",
        0.7,
        "disabled",
        None,
    ),
    (
        "deepseek-v4-pro",
        "DeepSeek V4 Pro 思考版",
        0.7,
        "enabled",
        Some("high"),
    ),
    (
        "deepseek-v4-pro",
        "DeepSeek V4 Pro 非思考版",
        0.7,
        "disabled",
        None,
    ),
];

/// Find the display label for a given model + temperature combo.
fn find_variant_label(
    variants: &'static [(&str, &str, f32, &str, Option<&str>)],
    model: &str,
    thinking: Option<&str>,
    temperature: f32,
) -> Option<&'static str> {
    variants
        .iter()
        .find(|(m, _, t, th, _)| {
            *m == model
                && thinking.map(|v| v == *th).unwrap_or(true)
                && (*t - temperature).abs() < 0.01
        })
        .map(|(_, label, _, _, _)| *label)
}

#[component]
pub fn LlmSettingsPage() -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let config: RwSignal<Option<LlmGlobalConfig>> = RwSignal::new(None);
    let loading = RwSignal::new(true);
    let saving = RwSignal::new(false);
    let reloading = RwSignal::new(false);
    let message: RwSignal<Option<String>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let save_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Form state
    let selected_provider = RwSignal::new(String::new());
    let selected_model = RwSignal::new(String::new());
    let selected_temperature = RwSignal::new(1.0_f32);
    let selected_thinking = RwSignal::new(String::new());
    let selected_reasoning_effort = RwSignal::new(String::new());
    let selected_variant_label = RwSignal::new(String::new());

    let fetch_config = move || {
        let service = use_app_state().llm_config_service();
        loading.set(true);
        error.set(None);
        message.set(None);
        save_error.set(None);
        spawn_local(async move {
            match service.get_config().await {
                Ok(c) => {
                    // Auto-select default provider
                    let default = c.default_provider.clone();
                    if let Some(provider) = c.providers.iter().find(|p| p.name == default) {
                        selected_provider.set(provider.name.clone());
                        selected_model.set(provider.model.clone());
                        selected_temperature.set(provider.temperature);
                        selected_thinking.set(provider.thinking.clone().unwrap_or_default());
                        selected_reasoning_effort
                            .set(provider.reasoning_effort.clone().unwrap_or_default());
                        // Try to match variant label
                        if provider.name == "kimi" {
                            let thinking = provider.thinking.as_deref().or(Some("disabled"));
                            let label = find_variant_label(
                                KIMI_MODELS,
                                &provider.model,
                                thinking,
                                provider.temperature,
                            )
                            .unwrap_or(&provider.model);
                            selected_variant_label.set(label.to_string());
                        } else if provider.name == "deepseek" {
                            let thinking = provider.thinking.as_deref().or(Some("enabled"));
                            let label = find_variant_label(
                                DEEPSEEK_MODELS,
                                &provider.model,
                                thinking,
                                provider.temperature,
                            )
                            .unwrap_or(&provider.model);
                            selected_variant_label.set(label.to_string());
                        } else {
                            selected_variant_label.set(provider.model.clone());
                        }
                    }
                    config.set(Some(c));
                }
                Err(e) => error.set(Some(format!("加载配置失败: {}", e))),
            }
            loading.set(false);
        });
    };

    let fetch_stored = StoredValue::new(fetch_config);

    Effect::new(move |_| {
        fetch_stored.get_value()();
    });

    // When provider changes, reset model selection
    let on_provider_change = move |provider: String| {
        selected_provider.set(provider.clone());
        if let Some(existing) = config
            .get()
            .and_then(|c| c.providers.into_iter().find(|p| p.name == provider))
        {
            selected_model.set(existing.model.clone());
            selected_temperature.set(existing.temperature);
            selected_thinking.set(existing.thinking.clone().unwrap_or_default());
            selected_reasoning_effort.set(existing.reasoning_effort.clone().unwrap_or_default());
            if existing.name == "kimi" {
                let label = find_variant_label(
                    KIMI_MODELS,
                    &existing.model,
                    existing.thinking.as_deref().or(Some("disabled")),
                    existing.temperature,
                )
                .unwrap_or(&existing.model);
                selected_variant_label.set(label.to_string());
            } else if existing.name == "deepseek" {
                let label = find_variant_label(
                    DEEPSEEK_MODELS,
                    &existing.model,
                    existing.thinking.as_deref().or(Some("enabled")),
                    existing.temperature,
                )
                .unwrap_or(&existing.model);
                selected_variant_label.set(label.to_string());
            } else {
                selected_variant_label.set(existing.model);
            }
        } else if provider == "kimi" {
            if let Some((model, label, temp, thinking, effort)) = KIMI_MODELS.first() {
                selected_model.set(model.to_string());
                selected_variant_label.set(label.to_string());
                selected_temperature.set(*temp);
                selected_thinking.set(thinking.to_string());
                selected_reasoning_effort.set(effort.unwrap_or("").to_string());
            }
        } else if provider == "deepseek" {
            if let Some((model, label, temp, thinking, effort)) = DEEPSEEK_MODELS.first() {
                selected_model.set(model.to_string());
                selected_variant_label.set(label.to_string());
                selected_temperature.set(*temp);
                selected_thinking.set(thinking.to_string());
                selected_reasoning_effort.set(effort.unwrap_or("").to_string());
            }
        } else {
            selected_model.set(String::new());
            selected_variant_label.set(String::new());
            selected_thinking.set(String::new());
            selected_reasoning_effort.set(String::new());
            selected_temperature.set(1.0);
        }
        save_error.set(None);
    };

    // When model variant changes for kimi
    let on_kimi_variant_change = move |label: String| {
        selected_variant_label.set(label.clone());
        if let Some((model, _, temp, thinking, effort)) =
            KIMI_MODELS.iter().find(|(_, l, _, _, _)| *l == label)
        {
            selected_model.set(model.to_string());
            selected_temperature.set(*temp);
            selected_thinking.set(thinking.to_string());
            selected_reasoning_effort.set(effort.unwrap_or("").to_string());
        }
    };

    let on_deepseek_variant_change = move |label: String| {
        selected_variant_label.set(label.clone());
        if let Some((model, _, temp, thinking, effort)) =
            DEEPSEEK_MODELS.iter().find(|(_, l, _, _, _)| *l == label)
        {
            selected_model.set(model.to_string());
            selected_temperature.set(*temp);
            selected_thinking.set(thinking.to_string());
            selected_reasoning_effort.set(effort.unwrap_or("").to_string());
        }
    };

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("llm-settings-title"))} />
        <div class="page llm-settings-page">
            <div class="page-header">
                <h1>{move || i18n.get().t("llm-settings-title")}</h1>
                <p class="page-description">{move || i18n.get().t("llm-settings-subtitle")}</p>
            </div>

            {move || if loading.get() {
                view! { <InlineLoading /> }.into_any()
            } else if let Some(err) = error.get() {
                view! {
                    <div class="error-state">
                        <div class="error-icon">"⚠️"</div>
                        <p>{err}</p>
                        <button class="btn btn-primary" on:click=move |_| fetch_stored.get_value()()>
                            {move || i18n.get().t("llm-settings-retry")}
                        </button>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="llm-settings-grid">
                        // Provider Selection
                        <section class="card llm-settings-section">
                            <h2>{move || i18n.get().t("llm-settings-provider")}</h2>
                            <div class="form-group">
                                <label>{move || i18n.get().t("llm-settings-select-provider")}</label>
                                <select
                                    prop:value=selected_provider
                                    on:change=move |e| {
                                        let val = crate::utils::event_target_value(&e);
                                        on_provider_change(val);
                                    }
                                >
                                    <option value="">"-- 请选择 --"</option>
                                    {move || {
                                        config.get()
                                            .map(|c| c.providers)
                                            .unwrap_or_default()
                                            .into_iter()
                                            .map(|p| {
                                                let name = p.name.clone();
                                                view! {
                                                    <option value={name.clone()}>{name.clone()}</option>
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                    }}
                                </select>
                            </div>
                        </section>

                        // Model Selection
                        <section class="card llm-settings-section">
                            <h2>{move || i18n.get().t("llm-settings-model-version")}</h2>
                            {move || {
                                let provider = selected_provider.get();
                                if provider == "kimi" {
                                    view! {
                                        <div class="form-group">
                                            <label>{move || i18n.get().t("llm-settings-select-kimi")}</label>
                                            <select
                                                prop:value=selected_variant_label
                                                on:change=move |e| {
                                                    let val = crate::utils::event_target_value(&e);
                                                    on_kimi_variant_change(val);
                                                }
                                            >
                                                <option value="">"-- 请选择 --"</option>
                                                {KIMI_MODELS.iter().map(|(_, label, _, _, _)| {
                                                    let label = label.to_string();
                                                    view! {
                                                        <option value={label.clone()}>{label.clone()}</option>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </select>
                                            <p class="form-help">
                                                {move || i18n.get().t("llm-settings-kimi-hint")}
                                            </p>
                                        </div>
                                    }.into_any()
                                } else if provider == "deepseek" {
                                    view! {
                                        <div class="form-group">
                                            <label>{move || i18n.get().t("llm-settings-select-deepseek")}</label>
                                            <select
                                                prop:value=selected_variant_label
                                                on:change=move |e| {
                                                    let val = crate::utils::event_target_value(&e);
                                                    on_deepseek_variant_change(val);
                                                }
                                            >
                                                <option value="">"-- 请选择 --"</option>
                                                {DEEPSEEK_MODELS.iter().map(|(_, label, _, _, _)| {
                                                    let label = label.to_string();
                                                    view! {
                                                        <option value={label.clone()}>{label.clone()}</option>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </select>
                                            <p class="form-help">
                                                {move || i18n.get().t("llm-settings-deepseek-hint")}
                                            </p>
                                        </div>
                                    }.into_any()
                                } else if !provider.is_empty() {
                                    view! {
                                        <div class="form-group">
                                            <label>{move || i18n.get().t("llm-settings-model-name")}</label>
                                            <input
                                                type="text"
                                                prop:value=selected_model
                                                on:input=move |e| {
                                                    selected_model.set(crate::utils::event_target_value(&e));
                                                }
                                                placeholder={move || i18n.get().t("llm-settings-model-placeholder")}
                                            />
                                        </div>
                                        <div class="form-group">
                                            <label>{move || i18n.get().t("llm-settings-temperature")}</label>
                                            <input
                                                type="number"
                                                step="0.1"
                                                min="0"
                                                max="2"
                                                prop:value=move || format!("{:.1}", selected_temperature.get())
                                                on:input=move |e| {
                                                    if let Ok(v) = crate::utils::event_target_value(&e).parse::<f32>() {
                                                        selected_temperature.set(v.clamp(0.0, 2.0));
                                                    }
                                                }
                                            />
                                            <p class="form-help">
                                                {move || i18n.get().t("llm-settings-temperature-hint")}
                                            </p>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <p class="form-help">{move || i18n.get().t("llm-settings-select-provider-first")}</p>
                                    }.into_any()
                                }
                            }}
                        </section>

                        // Current Parameters Summary
                        <section class="card llm-settings-section">
                            <h2>{move || i18n.get().t("llm-settings-current-params")}</h2>
                            <div class="info-grid">
                                <div class="info-row">
                                    <span>{move || i18n.get().t("llm-settings-provider-label")}</span>
                                    <span class="info-value">{move || selected_provider.get()}</span>
                                </div>
                                <div class="info-row">
                                    <span>{move || i18n.get().t("llm-settings-model-label")}</span>
                                    <span class="info-value">{move || selected_model.get()}</span>
                                </div>
                                <div class="info-row">
                                    <span>{move || i18n.get().t("llm-settings-temperature")}</span>
                                    <span class="info-value">{move || format!("{:.1}", selected_temperature.get())}</span>
                                </div>
                                <div class="info-row">
                                    <span>{move || i18n.get().t("llm-settings-thinking-label")}</span>
                                    <span class="info-value">{move || selected_thinking.get()}</span>
                                </div>
                                <div class="info-row">
                                    <span>{move || i18n.get().t("llm-settings-reasoning-label")}</span>
                                    <span class="info-value">{move || selected_reasoning_effort.get()}</span>
                                </div>
                            </div>
                        </section>

                        // Actions
                        <section class="card llm-settings-section">
                            <h2>{move || i18n.get().t("llm-settings-actions")}</h2>
                            {move || message.get().map(|msg| view! {
                                <div class="save-message success">{msg}</div>
                            })}
                            {move || save_error.get().map(|err| view! {
                                <div class="save-message error">{err}</div>
                            })}
                            <div class="form-actions">
                                <button
                                    class="btn btn-primary"
                                    on:click=move |_| {
                                        if selected_provider.get().is_empty() || selected_model.get().is_empty() {
                                            save_error.set(Some("请选择提供商和模型".to_string()));
                                            return;
                                        }

                                        let req = UpdateLlmConfigRequest {
                                            provider: selected_provider.get(),
                                            model: selected_model.get(),
                                            temperature: selected_temperature.get(),
                                            thinking: {
                                                let value = selected_thinking.get();
                                                if value.is_empty() { None } else { Some(value) }
                                            },
                                            reasoning_effort: {
                                                let value = selected_reasoning_effort.get();
                                                if value.is_empty() { None } else { Some(value) }
                                            },
                                            set_default: Some(true),
                                        };

                                        saving.set(true);
                                        save_error.set(None);
                                        message.set(None);

                                        let service = use_app_state().llm_config_service();
                                        spawn_local(async move {
                                            match service.update_config(&req).await {
                                                Ok(resp) => {
                                                    let msg = resp
                                                        .get("message")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("保存成功");
                                                    message.set(Some(msg.to_string()));
                                                    fetch_stored.get_value()();
                                                }
                                                Err(e) => save_error.set(Some(format!("保存失败: {}", e))),
                                            }
                                            saving.set(false);
                                        });
                                    }
                                    disabled=saving
                                >
                                    {move || if saving.get() { i18n.get().t("llm-settings-saving") } else { i18n.get().t("llm-settings-save-config") }}
                                </button>
                                <button
                                    class="btn btn-secondary"
                                    on:click=move |_| {
                                        reloading.set(true);
                                        save_error.set(None);
                                        message.set(None);

                                        let client = use_app_state().api_client();
                                        spawn_local(async move {
                                            match client
                                                .post::<serde_json::Value, _>("/admin/config/reload", &serde_json::json!({}))
                                                .await
                                            {
                                                Ok(resp) => {
                                                    let msg = resp
                                                        .get("message")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("配置已重载");
                                                    message.set(Some(msg.to_string()));
                                                    fetch_stored.get_value()();
                                                }
                                                Err(e) => save_error.set(Some(format!("重载失败: {}", e))),
                                            }
                                            reloading.set(false);
                                        });
                                    }
                                    disabled=reloading
                                >
                                    {move || if reloading.get() { i18n.get().t("llm-settings-reloading") } else { i18n.get().t("llm-settings-reload-restart") }}
                                </button>
                            </div>
                            <p class="form-help">
                                {move || format!("{} {}", i18n.get().t("llm-settings-reload-hint"), i18n.get().t("llm-settings-reload-restart"))}
                            </p>
                        </section>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
