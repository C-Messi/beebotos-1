//! Cron Jobs Page
//!
//! Manage scheduled cron jobs: create, edit, enable/disable, run manually,
//! and view execution history.

use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::*;

use crate::api::cron_jobs::{ContextMode, CronJob, CronJobRequest, ScheduleType};
use crate::components::Modal;
use crate::i18n::I18nContext;
use crate::state::use_app_state;

const POLL_INTERVAL_MS: u32 = 10_000;

// ============================================================================
// Main Page
// ============================================================================

#[component]
pub fn CronJobsPage() -> impl IntoView {
    let app_state = use_app_state();
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let refresh_seq = RwSignal::new(0_u64);
    let refreshing = RwSignal::new(false);
    let refresh_error = RwSignal::new(None::<String>);

    // ---- Fetch job list ----
    let jobs = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let _ = refresh_seq.get();
            let service = app_state.cron_job_service();
            async move {
                let result = service.list_jobs().await.map_err(|e| e.to_string());
                refreshing.set(false);
                match &result {
                    Ok(_) => refresh_error.set(None),
                    Err(e) => refresh_error.set(Some(e.clone())),
                }
                result
            }
        }
    });

    // ---- Auto-refresh polling ----
    let should_poll = RwSignal::new(true);
    let jobs_refetch = jobs.clone();
    Effect::new(move |_| {
        let jobs_r = jobs_refetch.clone();
        leptos::task::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
                if !should_poll.get() {
                    break;
                }
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if document.hidden() {
                        continue;
                    }
                }
                refresh_seq.update(|n| *n += 1);
                jobs_r.refetch();
            }
        });
        on_cleanup(move || {
            should_poll.set(false);
        });
    });

    let refresh = move || {
        refreshing.set(true);
        refresh_error.set(None);
        refresh_seq.update(|n| *n += 1);
        jobs.refetch();
    };

    // ---- Modal states ----
    let show_create = RwSignal::new(false);
    let editing_job = RwSignal::new(None::<CronJob>);
    let show_history = RwSignal::new(None::<String>);

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n.get().t("cron-jobs-title"))} />
        <div class="page cron-jobs-page">
            <div class="page-header">
                <div>
                    <h1>{move || i18n.get().t("cron-jobs-title")}</h1>
                    <p class="page-description">{move || i18n.get().t("cron-jobs-subtitle")}</p>
                </div>
                <div class="page-header-actions">
                    <button
                        class="btn btn-secondary"
                        disabled=move || refreshing.get()
                        on:click=move |_| refresh()
                    >
                        {move || if refreshing.get() { i18n.get().t("cron-jobs-refreshing") } else { format!("🔄 {}", i18n.get().t("cron-jobs-refresh")) }}
                    </button>
                    <button
                        class="btn btn-primary"
                        on:click=move |_| {
                            editing_job.set(None);
                            show_create.set(true);
                        }
                    >
                        {move || format!("➕ {}", i18n.get().t("cron-jobs-new"))}
                    </button>
                </div>
            </div>

            <Show when=move || refresh_error.get().is_some()>
                <div class="error-box">{move || refresh_error.get().unwrap_or_default()}</div>
            </Show>

            <Suspense fallback=|| view! { <TableSkeleton rows=5 /> }>
                {move || {
                    jobs.get().map(|result| {
                        result.map(|list| {
                            view! {
                                <JobList
                                    jobs=list
                                    refresh=refresh.clone()
                                    on_edit=move |job: CronJob| {
                                        editing_job.set(Some(job));
                                        show_create.set(true);
                                    }
                                    on_history=move |id: String| {
                                        show_history.set(Some(id));
                                    }
                                />
                            }
                            .into_any()
                        })
                        .unwrap_or_else(|_| view! { <JobsError /> }.into_any())
                    })
                    .unwrap_or_else(|| view! { <TableSkeleton rows=5 /> }.into_any())
                }}
            </Suspense>

            // Create/Edit Modal
            <Show when=move || show_create.get()>
                <Modal
                    title={move || if editing_job.get().is_some() { i18n.get().t("cron-jobs-edit") } else { i18n.get().t("cron-jobs-create") }}
                    on_close=move || show_create.set(false)
                >
                    <JobForm
                        job=editing_job.get()
                        on_save={
                            let refresh = refresh.clone();
                            move || {
                                show_create.set(false);
                                editing_job.set(None);
                                refresh();
                            }
                        }
                        on_cancel=move || {
                            show_create.set(false);
                            editing_job.set(None);
                        }
                    />
                </Modal>
            </Show>

            // History Modal
            <Show when=move || show_history.get().is_some()>
                <Modal
                    title={move || use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-history")}
                    on_close=move || show_history.set(None)
                >
                    {move || {
                        show_history.get().map(|id| {
                            view! { <JobHistory job_id=id /> }
                        })
                    }}
                </Modal>
            </Show>
        </div>
    }
}

// ============================================================================
// Job List
// ============================================================================

#[component]
fn JobList(
    jobs: Vec<CronJob>,
    refresh: impl Fn() + Clone + Send + Sync + 'static,
    on_edit: impl Fn(CronJob) + Clone + Send + Sync + 'static,
    on_history: impl Fn(String) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    if jobs.is_empty() {
        return Either::Left(view! {
            <div class="empty-state">
                <span class="empty-icon">"⏰"</span>
                <p>"暂无定时任务"</p>
                <p class="empty-hint">{move || i18n.get().t("cron-jobs-empty-hint")}</p>
            </div>
        });
    }

    Either::Right(view! {
        <div class="card">
            <table class="data-table">
                <thead>
                    <tr>
                        <th>{move || i18n.get().t("cron-jobs-table-name")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-schedule")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-expression")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-status")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-runs")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-next")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-actions")}</th>
                    </tr>
                </thead>
                <tbody>
                    {jobs.into_iter().map(|job| {
                        let job_id_toggle = job.id.clone();
                        let job_id_run = job.id.clone();
                        let job_id_delete = job.id.clone();
                        let job_id_history = job.id.clone();
                        let job_clone_edit = job.clone();
                        let refresh_toggle = refresh.clone();
                        let refresh_run = refresh.clone();
                        let refresh_delete = refresh.clone();
                        let on_edit_cb = on_edit.clone();
                        let on_history_cb = on_history.clone();

                        let schedule_label = match job.schedule_type {
                            ScheduleType::At => i18n.get().t("cron-jobs-type-timed"),
                            ScheduleType::Every => i18n.get().t("cron-jobs-type-interval"),
                            ScheduleType::Cron => i18n.get().t("cron-jobs-type-cron"),
                        };

                        let status_class = if job.enabled { "status-badge active" } else { "status-badge disabled" };
                        let status_text = if job.enabled { i18n.get().t("cron-jobs-enable") } else { i18n.get().t("cron-jobs-disable") };

                        view! {
                            <tr>
                                <td>
                                    <div class="job-name">{job.name.clone()}</div>
                                    <div class="job-desc">{job.description.clone()}</div>
                                </td>
                                <td>{schedule_label}</td>
                                <td><code class="code-inline">{job.schedule_expr.clone()}</code></td>
                                <td><span class={status_class}>{status_text}</span></td>
                                <td>{job.run_count.to_string()}</td>
                                <td>{job.next_run_at.clone().unwrap_or_else(|| "—".to_string())}</td>
                                <td>
                                    <div class="action-buttons">
                                        <button
                                            class="btn btn-sm btn-secondary"
                                            on:click=move |_| {
                                                let svc = use_app_state().cron_job_service();
                                                let refresh = refresh_toggle.clone();
                                                let id = job_id_toggle.clone();
                                                leptos::task::spawn_local(async move {
                                                    let _ = svc.toggle_job(&id).await;
                                                    refresh();
                                                });
                                            }
                                        >
                                            {if job.enabled { i18n.get().t("cron-jobs-disable") } else { i18n.get().t("cron-jobs-enable") }}
                                        </button>
                                        <button
                                            class="btn btn-sm btn-primary"
                                            on:click=move |_| {
                                                let svc = use_app_state().cron_job_service();
                                                let refresh = refresh_run.clone();
                                                let id = job_id_run.clone();
                                                leptos::task::spawn_local(async move {
                                                    let _ = svc.run_job(&id).await;
                                                    refresh();
                                                });
                                            }
                                        >
                                            {move || i18n.get().t("cron-jobs-run")}
                                        </button>
                                        <button
                                            class="btn btn-sm btn-secondary"
                                            on:click=move |_| {
                                                on_edit_cb(job_clone_edit.clone());
                                            }
                                        >
                                            {move || i18n.get().t("cron-jobs-edit-action")}
                                        </button>
                                        <button
                                            class="btn btn-sm btn-secondary"
                                            on:click=move |_| {
                                                on_history_cb(job_id_history.clone());
                                            }
                                        >
                                            {move || i18n.get().t("cron-jobs-history-action")}
                                        </button>
                                        <button
                                            class="btn btn-sm btn-danger"
                                            on:click=move |_| {
                                                let svc = use_app_state().cron_job_service();
                                                let refresh = refresh_delete.clone();
                                                let id = job_id_delete.clone();
                                                leptos::task::spawn_local(async move {
                                                    let _ = svc.delete_job(&id).await;
                                                    refresh();
                                                });
                                            }
                                        >
                                            {move || i18n.get().t("cron-jobs-delete")}
                                        </button>
                                    </div>
                                </td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    })
}

// ============================================================================
// Job Form (Create / Edit)
// ============================================================================

#[component]
fn JobForm(
    job: Option<CronJob>,
    on_save: impl Fn() + Clone + Send + Sync + 'static,
    on_cancel: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    let is_edit = job.is_some();

    let name = RwSignal::new(job.as_ref().map(|j| j.name.clone()).unwrap_or_default());
    let description = RwSignal::new(
        job.as_ref()
            .map(|j| j.description.clone())
            .unwrap_or_default(),
    );
    let schedule_type = RwSignal::new(
        job.as_ref()
            .map(|j| j.schedule_type.clone())
            .unwrap_or(ScheduleType::Cron),
    );
    let schedule_expr = RwSignal::new(
        job.as_ref()
            .map(|j| j.schedule_expr.clone())
            .unwrap_or_default(),
    );
    let timezone = RwSignal::new(
        job.as_ref()
            .map(|j| j.timezone.clone())
            .unwrap_or_else(|| "Asia/Shanghai".to_string()),
    );
    let prompt = RwSignal::new(job.as_ref().map(|j| j.prompt.clone()).unwrap_or_default());
    let enabled = RwSignal::new(job.as_ref().map(|j| j.enabled).unwrap_or(true));
    let context_mode = RwSignal::new(
        job.as_ref()
            .map(|j| j.context_mode.clone())
            .unwrap_or(ContextMode::Isolated),
    );
    let max_runs = RwSignal::new(
        job.as_ref()
            .and_then(|j| j.max_runs)
            .map(|v| v.to_string())
            .unwrap_or_default(),
    );
    let delivery_channel = RwSignal::new(
        job.as_ref()
            .map(|j| j.delivery_channel.clone())
            .unwrap_or_default(),
    );
    let delivery_target = RwSignal::new(
        job.as_ref()
            .map(|j| j.delivery_target.clone())
            .unwrap_or_default(),
    );

    let saving = RwSignal::new(false);
    let error_msg = RwSignal::new(None::<String>);

    let save = {
        let on_save = on_save.clone();
        move || {
            let svc = use_app_state().cron_job_service();
            let on_save = on_save.clone();
            let job_id = job.as_ref().map(|j| j.id.clone());

            let req = CronJobRequest {
                name: name.get(),
                description: Some(description.get()),
                schedule_type: schedule_type.get(),
                schedule_expr: schedule_expr.get(),
                timezone: Some(timezone.get()),
                prompt: prompt.get(),
                enabled: Some(enabled.get()),
                context_mode: Some(context_mode.get()),
                delivery_channel: Some(delivery_channel.get()),
                delivery_target: Some(delivery_target.get()),
                max_runs: max_runs.get().parse().ok(),
            };

            saving.set(true);
            error_msg.set(None);

            leptos::task::spawn_local(async move {
                let result = if let Some(id) = job_id {
                    svc.update_job(&id, &req).await
                } else {
                    svc.create_job(&req).await
                };

                saving.set(false);
                match result {
                    Ok(_) => on_save(),
                    Err(e) => error_msg.set(Some(format!("保存失败: {}", e))),
                }
            });
        }
    };

    let type_options = vec![
        (
            ScheduleType::Cron,
            "Cron 表达式",
            "标准 5 字段 cron，如 */5 * * * *",
        ),
        (ScheduleType::Every, "固定间隔", "如 30m, 1h, 4h, 1d"),
        (
            ScheduleType::At,
            "定时一次",
            "北京时间，如 2026-05-06 09:00:00",
        ),
    ];

    view! {
        <div class="form-container">
            <Show when=move || error_msg.get().is_some()>
                <div class="error-box">{move || error_msg.get().unwrap_or_default()}</div>
            </Show>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-name")}</label>
                <input
                    type="text"
                    prop:value=move || name.get()
                    on:input:target=move |ev| name.set(ev.target().value())
                    placeholder={move || i18n.get().t("cron-jobs-form-name-placeholder")}
                />
            </div>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-desc")}</label>
                <input
                    type="text"
                    prop:value=move || description.get()
                    on:input:target=move |ev| description.set(ev.target().value())
                    placeholder={move || i18n.get().t("cron-jobs-form-desc-placeholder")}
                />
            </div>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-schedule-type")}</label>
                <select
                    prop:value={
                        let st = schedule_type.get();
                        match st {
                            ScheduleType::At => "at",
                            ScheduleType::Every => "every",
                            ScheduleType::Cron => "cron",
                        }.to_string()
                    }
                    on:change:target=move |ev| {
                        let val = ev.target().value();
                        let t = match val.as_str() {
                            "at" => ScheduleType::At,
                            "every" => ScheduleType::Every,
                            _ => ScheduleType::Cron,
                        };
                        schedule_type.set(t);
                    }
                >
                    {type_options.into_iter().map(|(t, label, _hint)| {
                        let val = match t {
                            ScheduleType::At => "at",
                            ScheduleType::Every => "every",
                            ScheduleType::Cron => "cron",
                        };
                        view! { <option value={val}>{label}</option> }
                    }).collect::<Vec<_>>()}
                </select>
            </div>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-expression")}</label>
                <input
                    type="text"
                    prop:value=move || schedule_expr.get()
                    on:input:target=move |ev| schedule_expr.set(ev.target().value())
                    placeholder={
                        match schedule_type.get() {
                            ScheduleType::Cron => "*/5 * * * *".to_string(),
                            ScheduleType::Every => "30m".to_string(),
                            ScheduleType::At => "2026-05-06 09:00:00".to_string(),
                        }
                    }
                />
                <span class="form-hint">
                    {move || {
                        match schedule_type.get() {
                            ScheduleType::Cron => use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-form-cron-hint"),
                            ScheduleType::Every => use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-form-interval-hint"),
                            ScheduleType::At => "未带时区时按北京时间解析".to_string(),
                        }
                    }}
                </span>
            </div>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-timezone")}</label>
                <input
                    type="text"
                    prop:value=move || timezone.get()
                    on:input:target=move |ev| timezone.set(ev.target().value())
                    placeholder={move || i18n.get().t("cron-jobs-form-timezone-placeholder")}
                />
            </div>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-prompt")}</label>
                <textarea
                    prop:value=move || prompt.get()
                    on:input:target=move |ev| prompt.set(ev.target().value())
                    rows=4
                    placeholder={move || i18n.get().t("cron-jobs-form-prompt-placeholder")}
                />
            </div>

            <div class="form-row">
                <div class="form-group">
                    <label>{move || i18n.get().t("cron-jobs-form-context-mode")}</label>
                    <select
                        prop:value={
                            match context_mode.get() {
                                ContextMode::Main => "main",
                                ContextMode::Isolated => "isolated",
                            }.to_string()
                        }
                        on:change:target=move |ev| {
                            let val = ev.target().value();
                            context_mode.set(match val.as_str() {
                                "main" => ContextMode::Main,
                                _ => ContextMode::Isolated,
                            });
                        }
                    >
                        <option value="isolated">{move || i18n.get().t("cron-jobs-form-context-standalone")}</option>
                        <option value="main">{move || i18n.get().t("cron-jobs-form-context-shared")}</option>
                    </select>
                </div>

                <div class="form-group">
                    <label>{move || i18n.get().t("cron-jobs-form-max-runs")}</label>
                    <input
                        type="number"
                        prop:value=move || max_runs.get()
                        on:input:target=move |ev| max_runs.set(ev.target().value())
                        placeholder={move || i18n.get().t("cron-jobs-form-max-runs-placeholder")}
                    />
                </div>
            </div>

            <div class="form-group">
                <label>{move || i18n.get().t("cron-jobs-form-channel")}</label>
                <select
                    prop:value=move || delivery_channel.get()
                    on:change:target=move |ev| delivery_channel.set(ev.target().value())
                >
                    <option value="">"不发送通知"</option>
                    <option value="webchat">"WebChat"</option>
                    <option value="webhook">"Webhook"</option>
                </select>
            </div>

            <Show when=move || !delivery_channel.get().is_empty()>
                <div class="form-group">
                    <label>{move || i18n.get().t("cron-jobs-form-target")}</label>
                    <input
                        type="text"
                        prop:value=move || delivery_target.get()
                        on:input:target=move |ev| delivery_target.set(ev.target().value())
                        placeholder={
                            match delivery_channel.get().as_str() {
                                "webchat" => "webchat",
                                "webhook" => "https://example.com/webhook",
                                _ => "",
                            }
                        }
                    />
                    <span class="form-hint">
                        {move || {
                            match delivery_channel.get().as_str() {
                                "webchat" => use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-form-target-webchat-hint"),
                                "webhook" => use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-form-target-webhook-hint"),
                                _ => "".to_string(),
                            }
                        }}
                    </span>
                </div>
            </Show>

            <div class="form-group checkbox-group">
                <label>
                    <input
                        type="checkbox"
                        prop:checked=move || enabled.get()
                        on:change:target=move |ev| enabled.set(ev.target().checked())
                    />
                    {move || i18n.get().t("cron-jobs-form-enabled")}
                </label>
            </div>

            <div class="form-actions">
                <button class="btn btn-secondary" on:click=move |_| on_cancel()>
                    {move || i18n.get().t("cron-jobs-form-cancel")}
                </button>
                <button
                    class="btn btn-primary"
                    on:click=move |_| save()
                    disabled=move || saving.get() || name.get().trim().is_empty() || prompt.get().trim().is_empty() || schedule_expr.get().trim().is_empty()
                >
                    {move || if saving.get() { i18n.get().t("cron-jobs-form-saving") } else if is_edit { i18n.get().t("cron-jobs-form-save") } else { i18n.get().t("cron-jobs-form-create") }}
                </button>
            </div>
        </div>
    }
}

// ============================================================================
// Job History
// ============================================================================

#[component]
fn JobHistory(job_id: String) -> impl IntoView {
    let runs = LocalResource::new({
        let job_id = job_id.clone();
        move || {
            let svc = use_app_state().cron_job_service();
            let id = job_id.clone();
            async move { svc.list_runs(&id).await.ok() }
        }
    });

    view! {
        <Suspense fallback=|| view! { <div>{move || use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-history-loading")}</div> }>
            {move || {
                runs.get().map(|r| {
                    r.map(|list| {
                        if list.is_empty() {
                            return view! {
                                <div class="empty-state">
                                    <span class="empty-icon">"📋"</span>
                                    <p>"暂无执行记录"</p>
                                </div>
                            }
                            .into_any();
                        }
                        view! {
                            <div class="run-history">
                                {list.into_iter().map(|run| {
                                    let status_class = match run.status.as_str() {
                                        "success" => "status-badge active",
                                        "failed" => "status-badge error",
                                        _ => "status-badge pending",
                                    };
                                    let run_output = run.output.clone();
                                    let run_error = run.error.clone();
                                    let run_status = run.status.clone();
                                    let run_started_at = run.started_at.clone();
                                    let run_output2 = run_output.clone();
                                    let run_error2 = run_error.clone();
                                    view! {
                                        <div class="run-item">
                                            <div class="run-header">
                                                <span class={status_class}>{run_status}</span>
                                                <span class="run-time">{run_started_at}</span>
                                            </div>
                                            <Show when=move || !run_output.is_empty()>
                                                <pre class="run-output">{run_output2.clone()}</pre>
                                            </Show>
                                            <Show when=move || !run_error.is_empty()>
                                                <pre class="run-error">{run_error2.clone()}</pre>
                                            </Show>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }
                        .into_any()
                    })
                    .unwrap_or_else(|| view! { <div class="error-box">{move || use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-history-error")}</div> }.into_any())
                })
                .unwrap_or_else(|| view! { <div>{move || use_context::<I18nContext>().expect("i18n context not found").t("cron-jobs-history-loading")}</div> }.into_any())
            }}
        </Suspense>
    }
}

// ============================================================================
// Skeleton & Error States
// ============================================================================

#[component]
fn TableSkeleton(rows: usize) -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    view! {
        <div class="card">
            <table class="data-table">
                <thead>
                    <tr>
                        <th>{move || i18n.get().t("cron-jobs-table-name")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-schedule")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-expression")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-status")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-runs")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-next")}</th>
                        <th>{move || i18n.get().t("cron-jobs-table-actions")}</th>
                    </tr>
                </thead>
                <tbody>
                    {(0..rows).map(|_| {
                        view! {
                            <tr>
                                <td><div class="skeleton" style="width: 120px"/></td>
                                <td><div class="skeleton" style="width: 60px"/></td>
                                <td><div class="skeleton" style="width: 100px"/></td>
                                <td><div class="skeleton" style="width: 50px"/></td>
                                <td><div class="skeleton" style="width: 40px"/></td>
                                <td><div class="skeleton" style="width: 120px"/></td>
                                <td><div class="skeleton" style="width: 150px"/></td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn JobsError() -> impl IntoView {
    let i18n = RwSignal::new(use_context::<I18nContext>().expect("i18n context not found"));
    view! {
        <div class="error-box">{move || i18n.get().t("cron-jobs-error-load")}</div>
    }
}
