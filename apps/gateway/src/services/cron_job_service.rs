//! Cron Job Service
//!
//! Manages scheduled tasks using tokio-cron-scheduler with SQLite persistence.
//! Supports three schedule types: at (one-shot), every (interval), cron
//! (expression).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::AppError;

/// Schedule type for cron jobs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleType {
    /// One-time execution at ISO 8601 time
    At,
    /// Fixed interval (e.g. "30m", "1h", "1d")
    Every,
    /// Standard 5-field cron expression
    Cron,
}

impl std::fmt::Display for ScheduleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleType::At => write!(f, "at"),
            ScheduleType::Every => write!(f, "every"),
            ScheduleType::Cron => write!(f, "cron"),
        }
    }
}

/// Context execution mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContextMode {
    /// Share main session context
    Main,
    /// Run in isolated session
    Isolated,
}

/// Cron job model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub description: String,
    pub schedule_type: ScheduleType,
    pub schedule_expr: String,
    pub timezone: String,
    pub prompt: String,
    pub enabled: bool,
    pub context_mode: ContextMode,
    pub delivery_channel: String,
    pub delivery_target: String,
    pub max_runs: Option<i64>,
    pub run_count: i64,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Cron job run record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobRun {
    pub id: String,
    pub job_id: String,
    pub status: String,
    pub output: String,
    pub error: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub triggered_by: String,
}

/// Create/update request
#[derive(Debug, Deserialize)]
pub struct CronJobRequest {
    pub name: String,
    pub description: Option<String>,
    pub schedule_type: ScheduleType,
    pub schedule_expr: String,
    pub timezone: Option<String>,
    pub prompt: String,
    pub enabled: Option<bool>,
    pub context_mode: Option<ContextMode>,
    pub delivery_channel: Option<String>,
    pub delivery_target: Option<String>,
    pub max_runs: Option<i64>,
}

impl CronJobRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.name.trim().is_empty() {
            return Err(AppError::bad_request("Job name cannot be empty"));
        }
        if self.prompt.trim().is_empty() {
            return Err(AppError::bad_request("Prompt cannot be empty"));
        }
        if self.schedule_expr.trim().is_empty() {
            return Err(AppError::bad_request("Schedule expression cannot be empty"));
        }
        // Validate cron expression if type is cron
        if self.schedule_type == ScheduleType::Cron {
            let parts: Vec<&str> = self.schedule_expr.split_whitespace().collect();
            if parts.len() != 5 {
                return Err(AppError::bad_request(
                    "Cron expression must have exactly 5 fields (min hour day month dow)",
                ));
            }
        }
        Ok(())
    }
}

/// Service for managing cron jobs
#[derive(Clone)]
pub struct CronJobService {
    db: SqlitePool,
    scheduler_jobs: Arc<RwLock<HashMap<String, uuid::Uuid>>>,
}

impl CronJobService {
    pub fn new(db: SqlitePool) -> Self {
        Self {
            db,
            scheduler_jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// List all cron jobs
    pub async fn list_jobs(&self) -> Result<Vec<CronJob>, AppError> {
        let rows = sqlx::query_as::<_, CronJobRow>(
            r#"
            SELECT id, name, description, schedule_type, schedule_expr, timezone,
                   prompt, enabled, context_mode, delivery_channel, delivery_target,
                   max_runs, run_count, last_run_at, next_run_at, created_by, created_at, updated_at
            FROM cron_jobs
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(AppError::database)?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get a single job by ID
    pub async fn get_job(&self, id: &str) -> Result<CronJob, AppError> {
        let row = sqlx::query_as::<_, CronJobRow>(
            r#"
            SELECT id, name, description, schedule_type, schedule_expr, timezone,
                   prompt, enabled, context_mode, delivery_channel, delivery_target,
                   max_runs, run_count, last_run_at, next_run_at, created_by, created_at, updated_at
            FROM cron_jobs WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::database)?
        .ok_or_else(|| AppError::not_found("CronJob", id))?;

        Ok(row.into())
    }

    /// Create a new cron job
    pub async fn create_job(
        &self,
        req: CronJobRequest,
        created_by: &str,
    ) -> Result<CronJob, AppError> {
        req.validate()?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let schedule_type_str = match req.schedule_type {
            ScheduleType::At => "at",
            ScheduleType::Every => "every",
            ScheduleType::Cron => "cron",
        };
        let context_mode_str = match req.context_mode.as_ref().unwrap_or(&ContextMode::Isolated) {
            ContextMode::Main => "main",
            ContextMode::Isolated => "isolated",
        };
        let next_run = self.compute_next_run(
            &req.schedule_type,
            &req.schedule_expr,
            req.timezone.as_deref().unwrap_or("UTC"),
        );

        sqlx::query(
            r#"
            INSERT INTO cron_jobs (
                id, name, description, schedule_type, schedule_expr, timezone,
                prompt, enabled, context_mode, delivery_channel, delivery_target,
                max_runs, next_run_at, created_by, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
            "#,
        )
        .bind(&id)
        .bind(&req.name)
        .bind(req.description.as_deref().unwrap_or(""))
        .bind(schedule_type_str)
        .bind(&req.schedule_expr)
        .bind(req.timezone.as_deref().unwrap_or("UTC"))
        .bind(&req.prompt)
        .bind(if req.enabled.unwrap_or(true) { 1 } else { 0 })
        .bind(context_mode_str)
        .bind(req.delivery_channel.as_deref().unwrap_or(""))
        .bind(req.delivery_target.as_deref().unwrap_or(""))
        .bind(req.max_runs)
        .bind(next_run.as_ref().map(|d| d.to_rfc3339()))
        .bind(created_by)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;

        self.get_job(&id).await
    }

    /// Update an existing job
    pub async fn update_job(&self, id: &str, req: CronJobRequest) -> Result<CronJob, AppError> {
        req.validate()?;
        let now = Utc::now().to_rfc3339();
        let schedule_type_str = match req.schedule_type {
            ScheduleType::At => "at",
            ScheduleType::Every => "every",
            ScheduleType::Cron => "cron",
        };
        let context_mode_str = match req.context_mode.as_ref().unwrap_or(&ContextMode::Isolated) {
            ContextMode::Main => "main",
            ContextMode::Isolated => "isolated",
        };
        let next_run = self.compute_next_run(
            &req.schedule_type,
            &req.schedule_expr,
            req.timezone.as_deref().unwrap_or("UTC"),
        );

        let result = sqlx::query(
            r#"
            UPDATE cron_jobs SET
                name = ?1, description = ?2, schedule_type = ?3, schedule_expr = ?4,
                timezone = ?5, prompt = ?6, enabled = ?7, context_mode = ?8,
                delivery_channel = ?9, delivery_target = ?10, max_runs = ?11,
                next_run_at = ?12, updated_at = ?13
            WHERE id = ?14
            "#,
        )
        .bind(&req.name)
        .bind(req.description.as_deref().unwrap_or(""))
        .bind(schedule_type_str)
        .bind(&req.schedule_expr)
        .bind(req.timezone.as_deref().unwrap_or("UTC"))
        .bind(&req.prompt)
        .bind(if req.enabled.unwrap_or(true) { 1 } else { 0 })
        .bind(context_mode_str)
        .bind(req.delivery_channel.as_deref().unwrap_or(""))
        .bind(req.delivery_target.as_deref().unwrap_or(""))
        .bind(req.max_runs)
        .bind(next_run.as_ref().map(|d| d.to_rfc3339()))
        .bind(&now)
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("CronJob", id));
        }

        self.get_job(id).await
    }

    /// Delete a job
    pub async fn delete_job(&self, id: &str) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM cron_jobs WHERE id = ?1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(AppError::database)?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("CronJob", id));
        }
        Ok(())
    }

    /// Toggle enabled status
    pub async fn toggle_enabled(&self, id: &str) -> Result<bool, AppError> {
        let job = self.get_job(id).await?;
        let new_enabled = !job.enabled;
        let next_run = if new_enabled {
            self.compute_next_run(&job.schedule_type, &job.schedule_expr, &job.timezone)
        } else {
            None
        };

        sqlx::query(
            "UPDATE cron_jobs SET enabled = ?1, next_run_at = ?2, updated_at = ?3 WHERE id = ?4",
        )
        .bind(if new_enabled { 1 } else { 0 })
        .bind(next_run.as_ref().map(|d| d.to_rfc3339()))
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;

        Ok(new_enabled)
    }

    /// Record a job run start
    pub async fn record_run_start(&self, job_id: &str) -> Result<String, AppError> {
        let run_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO cron_job_runs (id, job_id, status, started_at) VALUES (?1, ?2, \
             'running', ?3)",
        )
        .bind(&run_id)
        .bind(job_id)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;

        // Update job last_run and run_count
        sqlx::query(
            "UPDATE cron_jobs SET last_run_at = ?1, run_count = run_count + 1, updated_at = ?1 \
             WHERE id = ?2",
        )
        .bind(&now)
        .bind(job_id)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;

        Ok(run_id)
    }

    /// Record a job run completion
    pub async fn record_run_complete(
        &self,
        run_id: &str,
        status: &str,
        output: &str,
        error: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE cron_job_runs SET status = ?1, output = ?2, error = ?3, completed_at = ?4 \
             WHERE id = ?5",
        )
        .bind(status)
        .bind(output)
        .bind(error)
        .bind(&now)
        .bind(run_id)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;
        Ok(())
    }

    /// List runs for a job
    pub async fn list_runs(&self, job_id: &str, limit: i64) -> Result<Vec<CronJobRun>, AppError> {
        let rows = sqlx::query_as::<_, CronJobRunRow>(
            r#"
            SELECT id, job_id, status, output, error, started_at, completed_at, triggered_by
            FROM cron_job_runs WHERE job_id = ?1 ORDER BY started_at DESC LIMIT ?2
            "#,
        )
        .bind(job_id)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::database)?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get enabled jobs that should be scheduled
    pub async fn get_enabled_jobs(&self) -> Result<Vec<CronJob>, AppError> {
        let rows = sqlx::query_as::<_, CronJobRow>(
            r#"
            SELECT id, name, description, schedule_type, schedule_expr, timezone,
                   prompt, enabled, context_mode, delivery_channel, delivery_target,
                   max_runs, run_count, last_run_at, next_run_at, created_by, created_at, updated_at
            FROM cron_jobs WHERE enabled = 1
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(AppError::database)?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Compute next run time (simplified)
    fn compute_next_run(
        &self,
        schedule_type: &ScheduleType,
        expr: &str,
        _tz: &str,
    ) -> Option<DateTime<Utc>> {
        match schedule_type {
            ScheduleType::At => DateTime::parse_from_rfc3339(expr)
                .ok()
                .map(|d| d.with_timezone(&Utc)),
            ScheduleType::Every => {
                // Parse duration like "30m", "1h", "1d"
                let dur = parse_duration(expr).unwrap_or(chrono::Duration::minutes(5));
                Some(Utc::now() + dur)
            }
            ScheduleType::Cron => {
                // For cron, we return now + 1 minute as a placeholder
                // Real next-run computation is handled by tokio-cron-scheduler
                Some(Utc::now() + chrono::Duration::minutes(1))
            }
        }
    }

    /// 🆕 FIX (P1): Clean up completed one-shot at-jobs older than 30 days
    /// to prevent table bloat from accumulating disabled historical records.
    async fn cleanup_completed_at_jobs(&self) -> Result<u64, AppError> {
        let result = sqlx::query(
            r#"
            DELETE FROM cron_jobs
            WHERE enabled = 0
              AND schedule_type = 'at'
              AND updated_at < datetime('now', '-30 days')
            "#,
        )
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            info!(
                "Cleaned up {} completed one-shot at-jobs older than 30 days",
                deleted
            );
        }
        Ok(deleted)
    }

    /// Get pending at-jobs whose scheduled time has arrived
    /// 🆕 FIX (P1): Runs cleanup before query to keep table lean.
    /// 🆕 FIX (P2): Selects only necessary fields + LIMIT 50 to reduce I/O.
    pub async fn get_pending_at_jobs(&self) -> Result<Vec<CronJob>, AppError> {
        // P1: Clean up old completed jobs first
        let _ = self.cleanup_completed_at_jobs().await;

        let now = Utc::now().to_rfc3339();
        let rows = sqlx::query_as::<_, PendingAtJobRow>(
            r#"
            SELECT id, name, prompt, context_mode, delivery_channel, delivery_target,
                   max_runs, run_count
            FROM cron_jobs
            WHERE enabled = 1 AND schedule_type = 'at' AND next_run_at <= ?1
            ORDER BY next_run_at ASC
            LIMIT 50
            "#,
        )
        .bind(&now)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::database)?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Disable a job (typically after a one-shot at-job completes)
    pub async fn disable_job(&self, id: &str) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE cron_jobs SET enabled = 0, next_run_at = NULL, updated_at = ?1 WHERE id = ?2",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(AppError::database)?;
        Ok(())
    }

    /// Count consecutive failed runs for a job (within last 24h)
    pub async fn get_recent_failure_count(&self, job_id: &str) -> Result<i64, AppError> {
        let since = (Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cron_job_runs WHERE job_id = ?1 AND status = 'failed' AND \
             started_at > ?2",
        )
        .bind(job_id)
        .bind(&since)
        .fetch_one(&self.db)
        .await
        .map_err(AppError::database)?;
        Ok(count)
    }

    /// Update next_run_at for a retry (exponential backoff)
    pub async fn schedule_retry(&self, job_id: &str, retry_count: i64) -> Result<(), AppError> {
        let backoff_minutes = (2i64.pow(retry_count.min(5) as u32)).min(60);
        let next_run = Utc::now() + chrono::Duration::minutes(backoff_minutes);
        sqlx::query("UPDATE cron_jobs SET next_run_at = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(next_run.to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(job_id)
            .execute(&self.db)
            .await
            .map_err(AppError::database)?;
        Ok(())
    }

    /// Track scheduler job UUID
    pub async fn track_scheduler_uuid(&self, job_id: &str, uuid: uuid::Uuid) {
        self.scheduler_jobs
            .write()
            .await
            .insert(job_id.to_string(), uuid);
    }

    /// Remove tracked scheduler UUID
    pub async fn remove_scheduler_uuid(&self, job_id: &str) -> Option<uuid::Uuid> {
        self.scheduler_jobs.write().await.remove(job_id)
    }

    /// Get tracked scheduler UUID
    pub async fn get_scheduler_uuid(&self, job_id: &str) -> Option<uuid::Uuid> {
        self.scheduler_jobs.read().await.get(job_id).copied()
    }
}

/// Parse duration string like "30m", "1h", "4h", "1d"
fn parse_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if s.len() < 2 {
        return None;
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str.parse().ok()?;
    match unit {
        "s" => Some(chrono::Duration::seconds(num)),
        "m" => Some(chrono::Duration::minutes(num)),
        "h" => Some(chrono::Duration::hours(num)),
        "d" => Some(chrono::Duration::days(num)),
        _ => {
            // Try parsing as seconds if no unit
            s.parse::<i64>().ok().map(chrono::Duration::seconds)
        }
    }
}

// ---------------------------------------------------------------------------
// SQLite row mappings
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct CronJobRow {
    id: String,
    name: String,
    description: String,
    schedule_type: String,
    schedule_expr: String,
    timezone: String,
    prompt: String,
    enabled: i32,
    context_mode: String,
    delivery_channel: String,
    delivery_target: String,
    max_runs: Option<i64>,
    run_count: i64,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl From<CronJobRow> for CronJob {
    fn from(row: CronJobRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            schedule_type: match row.schedule_type.as_str() {
                "at" => ScheduleType::At,
                "every" => ScheduleType::Every,
                _ => ScheduleType::Cron,
            },
            schedule_expr: row.schedule_expr,
            timezone: row.timezone,
            prompt: row.prompt,
            enabled: row.enabled != 0,
            context_mode: match row.context_mode.as_str() {
                "main" => ContextMode::Main,
                _ => ContextMode::Isolated,
            },
            delivery_channel: row.delivery_channel,
            delivery_target: row.delivery_target,
            max_runs: row.max_runs,
            run_count: row.run_count,
            last_run_at: row.last_run_at.and_then(|s| s.parse().ok()),
            next_run_at: row.next_run_at.and_then(|s| s.parse().ok()),
            created_by: row.created_by,
            created_at: row.created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: row.updated_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }
}

/// 🆕 FIX (P2): Lightweight row for pending at-jobs query — only fetches
/// necessary fields
#[derive(sqlx::FromRow)]
struct PendingAtJobRow {
    id: String,
    name: String,
    prompt: String,
    context_mode: String,
    delivery_channel: String,
    delivery_target: String,
    max_runs: Option<i64>,
    run_count: i64,
}

impl From<PendingAtJobRow> for CronJob {
    fn from(row: PendingAtJobRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: String::new(),
            schedule_type: ScheduleType::At,
            schedule_expr: String::new(),
            timezone: String::from("UTC"),
            prompt: row.prompt,
            enabled: true,
            context_mode: match row.context_mode.as_str() {
                "main" => ContextMode::Main,
                _ => ContextMode::Isolated,
            },
            delivery_channel: row.delivery_channel,
            delivery_target: row.delivery_target,
            max_runs: row.max_runs,
            run_count: row.run_count,
            last_run_at: None,
            next_run_at: None,
            created_by: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CronJobRunRow {
    id: String,
    job_id: String,
    status: String,
    output: String,
    error: String,
    started_at: String,
    completed_at: Option<String>,
    triggered_by: String,
}

impl From<CronJobRunRow> for CronJobRun {
    fn from(row: CronJobRunRow) -> Self {
        Self {
            id: row.id,
            job_id: row.job_id,
            status: row.status,
            output: row.output,
            error: row.error,
            started_at: row.started_at.parse().unwrap_or_else(|_| Utc::now()),
            completed_at: row.completed_at.and_then(|s| s.parse().ok()),
            triggered_by: row.triggered_by,
        }
    }
}
