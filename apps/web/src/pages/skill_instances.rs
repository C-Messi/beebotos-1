//! Skill Instance Management Page
//!
//! Create, manage, and execute skill instances bound to agents.

use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

use crate::api::{CreateInstanceRequest, InstanceInfo};
use crate::i18n::I18nContext;
use crate::state::use_app_state;

#[component]
pub fn SkillInstancesPage() -> impl IntoView {
    let app_state = use_app_state();
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let show_create_form = RwSignal::new(false);
    let create_skill_id = RwSignal::new(String::new());
    let create_agent_id = RwSignal::new(String::new());
    let is_creating = RwSignal::new(false);
    let is_executing = RwSignal::new(None::<String>);

    // Fetch instances
    let instances = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            async move {
                app_state.loading().skills.set(true);
                let result = service.list_instances().await;
                app_state.loading().skills.set(false);
                result
            }
        }
    });

    let reload_instances = {
        let instances = instances.clone();
        move || instances.refetch()
    };

    let create_instance = {
        let app_state = app_state.clone();
        let reload = reload_instances.clone();
        move || {
            let skill_id = create_skill_id.get();
            let agent_id = create_agent_id.get();
            if skill_id.is_empty() || agent_id.is_empty() {
                let i18n = use_context::<I18nContext>().expect("i18n context not found");
                app_state.notify(
                    crate::state::notification::NotificationType::Warning,
                    &i18n.t("skill-instances-missing-fields"),
                    &i18n.t("skill-instances-fill-fields"),
                );
                return;
            }
            is_creating.set(true);
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let reload = reload.clone();
            leptos::task::spawn_local(async move {
                let req = CreateInstanceRequest {
                    skill_id,
                    agent_id,
                    config: std::collections::HashMap::new(),
                };
                match service.create_instance(req).await {
                    Ok(instance) => {
                        let i18n = use_context::<I18nContext>().expect("i18n context not found");
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            &i18n.t("skill-instances-created"),
                            format!("{} {}", i18n.t("skill-instances-created"), instance.instance_id),
                        );
                        create_skill_id.set(String::new());
                        create_agent_id.set(String::new());
                        show_create_form.set(false);
                        reload();
                    }
                    Err(e) => {
                        let i18n = use_context::<I18nContext>().expect("i18n context not found");
                    app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            &i18n.t("skill-instances-creation-failed"),
                            format!("{}: {}", i18n.t("skill-instances-creation-failed"), e),
                        );
                    }
                }
                is_creating.set(false);
            });
        }
    };
    let create_instance_cb = StoredValue::new(create_instance);

    let delete_instance = {
        let app_state = app_state.clone();
        let reload = reload_instances.clone();
        move |instance_id: String| {
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let reload = reload.clone();
            leptos::task::spawn_local(async move {
                match service.delete_instance(&instance_id).await {
                    Ok(()) => {
                        let i18n = use_context::<I18nContext>().expect("i18n context not found");
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            &i18n.t("skill-instances-deleted"),
                            format!("{} {}", i18n.t("skill-instances-table-id"), instance_id),
                        );
                        reload();
                    }
                    Err(e) => {
                        let i18n = use_context::<I18nContext>().expect("i18n context not found");
                    app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            &i18n.t("skill-instances-delete-failed"),
                            format!("{}: {}", i18n.t("skill-instances-delete-failed"), e),
                        );
                    }
                }
            });
        }
    };
    let delete_instance_cb = StoredValue::new(delete_instance);

    let execute_instance = {
        let app_state = app_state.clone();
        move |instance_id: String| {
            is_executing.set(Some(instance_id.clone()));
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            leptos::task::spawn_local(async move {
                match service.execute_instance(&instance_id).await {
                    Ok(resp) => {
                        let msg = if resp.success {
                            format!("Execution completed in {}ms", resp.execution_time_ms)
                        } else {
                            format!("Execution failed: {}", resp.output)
                        };
                        let i18n = use_context::<I18nContext>().expect("i18n context not found");
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            &i18n.t("skill-instances-execution-result"),
                            msg,
                        );
                    }
                    Err(e) => {
                        let i18n = use_context::<I18nContext>().expect("i18n context not found");
                    app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            &i18n.t("skill-instances-execution-failed"),
                            format!("{}: {}", i18n.t("skill-instances-execution-failed"), e),
                        );
                    }
                }
                is_executing.set(None);
            });
        }
    };
    let execute_instance_cb = StoredValue::new(execute_instance);

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("skill-instances-title"))} />
        <div class="page skill-instances-page">
            <div class="page-header">
                <div>
                    <h1>{move || i18n.get().t("skill-instances-title")}</h1>
                    <p class="page-description">{move || i18n.get().t("skill-instances-subtitle")}</p>
                </div>
                <button
                    class="btn btn-primary"
                    on:click=move |_| show_create_form.update(|v| *v = !*v)
                >
                    {move || if show_create_form.get() { format!("✕ {}", i18n.get().t("skill-instances-cancel")) } else { format!("+ {}", i18n.get().t("skill-instances-new")) }}
                </button>
            </div>

            {move || if show_create_form.get() {
                view! {
                    <div class="create-form card">
                        <h3>{move || i18n.get().t("skill-instances-create")}</h3>
                        <div class="form-group">
                            <label>{move || i18n.get().t("skill-instances-skill-id")}</label>
                            <input
                                type="text"
                                placeholder={move || i18n.get().t("skill-instances-skill-id-placeholder")}
                                prop:value=create_skill_id
                                on:input=move |e| create_skill_id.set(event_target_value(&e))
                            />
                        </div>
                        <div class="form-group">
                            <label>{move || i18n.get().t("skill-instances-agent-id")}</label>
                            <input
                                type="text"
                                placeholder={move || i18n.get().t("skill-instances-agent-id-placeholder")}
                                prop:value=create_agent_id
                                on:input=move |e| create_agent_id.set(event_target_value(&e))
                            />
                        </div>
                        <button
                            class="btn btn-primary"
                            disabled=move || is_creating.get()
                            on:click=move |_| create_instance_cb.with_value(|f| f())
                        >
                            {move || if is_creating.get() { i18n.get().t("skill-instances-creating") } else { i18n.get().t("skill-instances-create") }}
                        </button>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            <Suspense fallback=|| view! { <InstancesLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match instances.await {
                            Ok(data) => {
                                if data.is_empty() {
                                    view! { <InstancesEmpty/> }.into_any()
                                } else {
                                    view! {
                                        <InstancesTable
                                            instances=data
                                            on_delete=move |id| delete_instance_cb.with_value(|f| f(id))
                                            on_execute=move |id| execute_instance_cb.with_value(|f| f(id))
                                            executing_id=is_executing.clone()
                                        />
                                    }.into_any()
                                }
                            }
                            Err(e) => view! { <InstancesError message=e.to_string()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn InstancesTable(
    instances: Vec<InstanceInfo>,
    on_delete: impl Fn(String) + Clone + Send + Sync + 'static,
    on_execute: impl Fn(String) + Clone + Send + Sync + 'static,
    executing_id: RwSignal<Option<String>>,
) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    view! {
        <div class="instances-table-wrapper">
            <table class="instances-table">
                <thead>
                    <tr>
                        <th>{move || i18n.get().t("skill-instances-table-id")}</th>
                        <th>{move || i18n.get().t("skill-instances-table-skill")}</th>
                        <th>{move || i18n.get().t("skill-instances-table-agent")}</th>
                        <th>{move || i18n.get().t("skill-instances-table-status")}</th>
                        <th>{move || i18n.get().t("skill-instances-table-usage")}</th>
                        <th>{move || i18n.get().t("skill-instances-table-actions")}</th>
                    </tr>
                </thead>
                <tbody>
                    {instances.into_iter().map(|instance| {
                        let status_class = format!("status-badge status-{}", instance.status.to_lowercase());
                        let is_exec = {
                            let id = instance.instance_id.clone();
                            let executing_id = executing_id.clone();
                            move || executing_id.get().as_ref() == Some(&id)
                        };
                        let is_exec2 = is_exec.clone();
                        view! {
                            <tr>
                                <td class="mono">{instance.instance_id.clone()}</td>
                                <td>{instance.skill_id.clone()}</td>
                                <td>{instance.agent_id.clone()}</td>
                                <td><span class=status_class.clone()>{instance.status.clone()}</span></td>
                                <td>
                                    {format!(
                                        "{} calls · {}ms avg",
                                        instance.usage.total_calls,
                                        instance.usage.avg_latency_ms as u64
                                    )}
                                </td>
                                <td class="actions">
                                    <button
                                        class="btn btn-sm btn-primary"
                                        disabled=is_exec
                                        on:click={
                                            let id = instance.instance_id.clone();
                                            let on_execute = on_execute.clone();
                                            move |_| on_execute(id.clone())
                                        }
                                    >
                                        {move || if is_exec2() { i18n.get().t("skill-instances-running") } else { format!("▶ {}", i18n.get().t("skill-instances-run")) }}
                                    </button>
                                    <button
                                        class="btn btn-sm btn-danger"
                                        on:click={
                                            let id = instance.instance_id.clone();
                                            let on_delete = on_delete.clone();
                                            move |_| on_delete(id.clone())
                                        }
                                    >
                                        {move || i18n.get().t("skill-instances-delete")}
                                    </button>
                                </td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn InstancesLoading() -> impl IntoView {
    view! {
        <div class="instances-table-wrapper">
            <div class="skeleton-table">
                <div class="skeleton-row"></div>
                <div class="skeleton-row"></div>
                <div class="skeleton-row"></div>
            </div>
        </div>
    }
}

#[component]
fn InstancesEmpty() -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    view! {
        <div class="empty-state">
            <div class="empty-icon">"🤖"</div>
            <h3>{move || i18n.get().t("skill-instances-empty-title")}</h3>
            <p>{move || i18n.get().t("skill-instances-empty-desc")}</p>
        </div>
    }
}

#[component]
fn InstancesError(#[prop(into)] message: String) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>{move || i18n.get().t("skill-instances-error-title")}</h3>
            <p>{message}</p>
        </div>
    }
}
