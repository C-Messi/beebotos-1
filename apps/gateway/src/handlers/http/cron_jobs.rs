//! Cron Job HTTP Handlers
//!
//! REST API for managing scheduled cron jobs with tokio-cron-scheduler integration.

use axum::extract::{Path, State};
use axum::Json;
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::GatewayError;
use crate::services::cron_job_service::CronJobRequest;
use crate::AppState;
use gateway::middleware::{require_any_role, AuthUser};

/// List all cron jobs
pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let jobs = svc.list_jobs().await?;
    Ok(Json(jobs.into_iter().map(|j| serde_json::json!(j)).collect()))
}

/// Get a single cron job
pub async fn get_job(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let job = svc.get_job(&id).await?;
    Ok(Json(serde_json::json!(job)))
}

/// Create a new cron job
pub async fn create_job(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CronJobRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let job = svc.create_job(req, &user.user_id).await?;

    // Register with tokio-cron-scheduler if enabled
    if let Err(e) = register_job_with_scheduler(&state, &job).await {
        warn!("Failed to register cron job with scheduler: {}", e);
    }

    info!("Created cron job {} by user {}", job.id, user.user_id);
    Ok(Json(serde_json::json!({
        "success": true,
        "job": job,
    })))
}

/// Update a cron job
pub async fn update_job(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<CronJobRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    // Remove old scheduler registration
    if let Some(old_uuid) = svc.get_scheduler_uuid(&id).await {
        if let Some(scheduler) = state.workflow_cron_scheduler.as_ref() {
            let _ = scheduler.remove(&old_uuid).await;
        }
        let _ = svc.remove_scheduler_uuid(&id).await;
    }

    let job = svc.update_job(&id, req).await?;

    // Re-register with scheduler if enabled
    if job.enabled {
        if let Err(e) = register_job_with_scheduler(&state, &job).await {
            warn!("Failed to re-register cron job with scheduler: {}", e);
        }
    }

    info!("Updated cron job {} by user {}", id, user.user_id);
    Ok(Json(serde_json::json!({
        "success": true,
        "job": job,
    })))
}

/// Delete a cron job
pub async fn delete_job(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    // Remove from scheduler
    if let Some(old_uuid) = svc.get_scheduler_uuid(&id).await {
        if let Some(scheduler) = state.workflow_cron_scheduler.as_ref() {
            let _ = scheduler.remove(&old_uuid).await;
        }
        let _ = svc.remove_scheduler_uuid(&id).await;
    }

    svc.delete_job(&id).await?;
    info!("Deleted cron job {} by user {}", id, user.user_id);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Cron job deleted",
    })))
}

/// Toggle enabled status
pub async fn toggle_job(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let enabled = svc.toggle_enabled(&id).await?;

    let job = svc.get_job(&id).await?;
    if enabled {
        if let Err(e) = register_job_with_scheduler(&state, &job).await {
            warn!("Failed to register cron job after enable: {}", e);
        }
    } else if let Some(old_uuid) = svc.get_scheduler_uuid(&id).await {
        if let Some(scheduler) = state.workflow_cron_scheduler.as_ref() {
            let _ = scheduler.remove(&old_uuid).await;
        }
        let _ = svc.remove_scheduler_uuid(&id).await;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "enabled": enabled,
    })))
}

/// Manually trigger a job
pub async fn run_job(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let job = svc.get_job(&id).await?;
    let run_id = svc.record_run_start(&id).await?;
    let run_id_for_spawn = run_id.clone();

    // Spawn the job execution asynchronously
    let svc_clone = svc.clone();
    let job_clone = job.clone();
    let state_clone = state.clone();
    tokio::spawn(async move {
        let result = execute_cron_job(&state_clone, &job_clone).await;
        match result {
            Ok(output) => {
                let _ = svc_clone.record_run_complete(&run_id_for_spawn, "success", &output, "").await;
            }
            Err(e) => {
                let _ = svc_clone.record_run_complete(&run_id_for_spawn, "failed", "", &e.to_string()).await;
            }
        }
    });

    info!("Manually triggered cron job {} by user {}", id, user.user_id);
    Ok(Json(serde_json::json!({
        "success": true,
        "run_id": run_id,
        "message": "Job triggered",
    })))
}

/// List execution history for a job
pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let runs = svc.list_runs(&id, 50).await?;
    Ok(Json(runs.into_iter().map(|r| serde_json::json!(r)).collect()))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn register_job_with_scheduler(
    state: &Arc<AppState>,
    job: &crate::services::cron_job_service::CronJob,
) -> Result<(), GatewayError> {
    let scheduler = state.workflow_cron_scheduler.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron scheduler not initialized"))?;
    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    // Build cron expression for tokio-cron-scheduler
    let schedule_str = match job.schedule_type {
        crate::services::cron_job_service::ScheduleType::At => {
            // One-shot: schedule as a one-time job
            return Ok(()); // One-shot jobs handled separately
        }
        crate::services::cron_job_service::ScheduleType::Every => {
            // Convert interval to cron-like expression
            parse_every_to_cron(&job.schedule_expr)
                .ok_or_else(|| GatewayError::bad_request("Invalid interval expression"))?
        }
        crate::services::cron_job_service::ScheduleType::Cron => {
            job.schedule_expr.clone()
        }
    };

    let job_id = job.id.clone();
    let state_clone = state.clone();
    let job_clone = job.clone();

    let ts_job = tokio_cron_scheduler::Job::new_async(&schedule_str, move |_uuid, _l| {
        let state = state_clone.clone();
        let job = job_clone.clone();
        Box::pin(async move {
            let svc = state.cron_job_service.as_ref();
            if svc.is_none() { return; }
            let svc = svc.unwrap();

            // Check max runs
            if let Some(max) = job.max_runs {
                if job.run_count >= max {
                    info!("Cron job {} reached max runs, skipping", job.id);
                    return;
                }
            }

            let run_id = match svc.record_run_start(&job.id).await {
                Ok(id) => id,
                Err(e) => {
                    warn!("Failed to record run start for job {}: {}", job.id, e);
                    return;
                }
            };

            let result = execute_cron_job(&state, &job).await;
            match result {
                Ok(output) => {
                    let _ = svc.record_run_complete(&run_id, "success", &output, "").await;
                    info!("Cron job {} executed successfully", job.id);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let _ = svc.record_run_complete(&run_id, "failed", "", &err_str).await;
                    warn!("Cron job {} failed: {}", job.id, err_str);

                    // Exponential backoff retry (max 3 retries within 24h)
                    let fail_count = match svc.get_recent_failure_count(&job.id).await {
                        Ok(n) => n,
                        Err(_) => 0,
                    };
                    if fail_count < 3 {
                        let retry_num = fail_count; // 0-based after this run
                        if let Err(sched_err) = svc.schedule_retry(&job.id, retry_num).await {
                            warn!("Failed to schedule retry for cron job {}: {}", job.id, sched_err);
                        } else {
                            info!("Scheduled retry {} for cron job {} (backoff: {} min)", retry_num + 1, job.id, (2i64.pow(retry_num.min(5) as u32)).min(60));
                        }
                    } else {
                        warn!("Cron job {} reached max retries (3), disabling", job.id);
                        let _ = svc.disable_job(&job.id).await;
                    }
                }
            }
        })
    }).map_err(|e| GatewayError::internal(format!("Failed to create cron job: {}", e)))?;

    let job_uuid = ts_job.guid();
    scheduler.add(ts_job).await
        .map_err(|e| GatewayError::internal(format!("Failed to add cron job to scheduler: {}", e)))?;

    svc.track_scheduler_uuid(&job_id, job_uuid).await;
    Ok(())
}

/// Convert interval expressions like "5m", "30m", "1h", "1d" to cron expressions
fn parse_every_to_cron(expr: &str) -> Option<String> {
    let expr = expr.trim();
    if expr.len() < 2 {
        return None;
    }
    let (num_str, unit) = expr.split_at(expr.len() - 1);
    let num: u32 = num_str.parse().ok()?;
    match unit {
        "m" => {
            if num >= 60 {
                let hours = num / 60;
                let mins = num % 60;
                Some(format!("{} */{} * * *", mins, hours))
            } else {
                Some(format!("*/{} * * * *", num))
            }
        }
        "h" => Some(format!("0 */{} * * *", num)),
        "d" => Some(format!("0 0 */{} * *", num)),
        _ => None,
    }
}

/// Execute a cron job by dispatching to the agent system
///
/// Features:
/// - Timeout: 60 seconds max execution time
/// - Notification: delivers result to configured channel (webchat/webhook)
async fn execute_cron_job(
    state: &Arc<AppState>,
    job: &crate::services::cron_job_service::CronJob,
) -> Result<String, GatewayError> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        execute_cron_job_inner(state, job),
    ).await;

    match result {
        Ok(Ok(output)) => {
            // Notify success
            let _ = notify_cron_result(state, job, "success", &output, "").await;
            Ok(output)
        }
        Ok(Err(e)) => {
            let err_str = e.to_string();
            let _ = notify_cron_result(state, job, "failed", "", &err_str).await;
            Err(e)
        }
        Err(_) => {
            let err = "Cron job execution timed out after 60 seconds".to_string();
            let _ = notify_cron_result(state, job, "timeout", "", &err).await;
            Err(GatewayError::internal(err))
        }
    }
}

async fn execute_cron_job_inner(
    state: &Arc<AppState>,
    job: &crate::services::cron_job_service::CronJob,
) -> Result<String, GatewayError> {
    if let Some(ref processor) = state.message_processor {
        use beebotos_agents::communication::{Message, MessageType, PlatformType};
        use std::collections::HashMap;

        let platform = PlatformType::WebChat;
        let channel_id = format!("cron:{}", job.id);
        let message = Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform,
            message_type: MessageType::Text,
            content: job.prompt.clone(),
            metadata: {
                let mut m = HashMap::new();
                m.insert("sender_id".to_string(), "cron".to_string());
                m.insert("cron_job_id".to_string(), job.id.clone());
                m.insert("cron_job_name".to_string(), job.name.clone());
                m
            },
            timestamp: chrono::Utc::now(),
        };

        let _ = processor.process_event(beebotos_agents::communication::channel::ChannelEvent::MessageReceived {
            platform,
            channel_id,
            message,
        }).await;

        Ok(format!("Dispatched cron job '{}' to agent system", job.name))
    } else {
        Err(GatewayError::internal("Message processor not available"))
    }
}

/// Notify cron job execution result to configured delivery channel
async fn notify_cron_result(
    state: &Arc<AppState>,
    job: &crate::services::cron_job_service::CronJob,
    status: &str,
    output: &str,
    error: &str,
) -> Result<(), GatewayError> {
    if job.delivery_channel.is_empty() {
        return Ok(());
    }

    let msg = match status {
        "success" => format!("✅ 定时任务 [{}] 执行成功\n\n{}", job.name, output),
        "timeout" => format!("⏰ 定时任务 [{}] 执行超时\n\n{}", job.name, error),
        _ => format!("❌ 定时任务 [{}] 执行失败\n\n{}", job.name, error),
    };

    match job.delivery_channel.as_str() {
        "webchat" => {
            // Send via WebSocket if target is a webchat session
            if let Some(ref ws) = state.ws_manager {
                let payload = serde_json::json!({
                    "type": "cron_notification",
                    "job_id": job.id,
                    "job_name": job.name,
                    "status": status,
                    "message": msg,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                let channel = if job.delivery_target.is_empty() { "webchat" } else { &job.delivery_target };
                let _ = ws.broadcast_to_channel(channel, payload).await;
            }
        }
        "webhook" => {
            if !job.delivery_target.is_empty() {
                let client = reqwest::Client::new();
                let body = serde_json::json!({
                    "job_id": job.id,
                    "job_name": job.name,
                    "status": status,
                    "output": output,
                    "error": error,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                let _ = client.post(&job.delivery_target)
                    .json(&body)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await;
            }
        }
        _ => {
            // Unknown channel: log only
            info!("Cron job {} notification to unsupported channel '{}' skipped", job.id, job.delivery_channel);
        }
    }

    Ok(())
}

/// Bulk register all enabled jobs on startup
pub async fn register_all_enabled_jobs(state: &Arc<AppState>) -> Result<(), GatewayError> {
    let svc = state.cron_job_service.as_ref()
        .ok_or_else(|| GatewayError::internal("Cron job service not initialized"))?;

    let jobs = svc.get_enabled_jobs().await?;
    let mut registered = 0;

    for job in jobs {
        if matches!(job.schedule_type, crate::services::cron_job_service::ScheduleType::At) {
            // One-shot jobs are not registered with the scheduler
            continue;
        }
        match register_job_with_scheduler(state, &job).await {
            Ok(()) => registered += 1,
            Err(e) => warn!("Failed to register cron job {} on startup: {}", job.id, e),
        }
    }

    info!("Registered {} cron jobs with scheduler", registered);
    Ok(())
}

/// Start a background task that checks for pending one-shot (at) jobs
pub async fn start_at_job_checker(state: Arc<AppState>, interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;

            let svc = match state.cron_job_service.as_ref() {
                Some(s) => s.clone(),
                None => continue,
            };

            let jobs = match svc.get_pending_at_jobs().await {
                Ok(j) => j,
                Err(e) => {
                    warn!("Failed to get pending at-jobs: {}", e);
                    continue;
                }
            };

            for job in jobs {
                info!("Executing one-shot at-job {}: '{}'", job.id, job.name);

                let run_id = match svc.record_run_start(&job.id).await {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Failed to record run start for at-job {}: {}", job.id, e);
                        continue;
                    }
                };

                let result = execute_cron_job(&state, &job).await;
                match result {
                    Ok(output) => {
                        let _ = svc.record_run_complete(&run_id, "success", &output, "").await;
                        info!("At-job {} executed successfully", job.id);
                    }
                    Err(e) => {
                        let _ = svc.record_run_complete(&run_id, "failed", "", &e.to_string()).await;
                        warn!("At-job {} failed: {}", job.id, e);
                    }
                }

                // Disable one-shot job after execution
                if let Err(e) = svc.disable_job(&job.id).await {
                    warn!("Failed to disable at-job {} after execution: {}", job.id, e);
                }
            }
        }
    });
}
