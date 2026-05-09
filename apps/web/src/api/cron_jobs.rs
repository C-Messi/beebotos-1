//! Cron Job API Service
//!
//! Frontend API client for Gateway cron job endpoints

use serde::{Deserialize, Serialize};

use super::client::{ApiClient, ApiError};

/// Schedule type for cron jobs
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleType {
    At,
    Every,
    Cron,
}

/// Context execution mode
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContextMode {
    Main,
    Isolated,
}

/// Cron job model from backend
#[derive(Clone, Debug, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub schedule_type: ScheduleType,
    pub schedule_expr: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    pub prompt: String,
    pub enabled: bool,
    #[serde(default = "default_context_mode")]
    pub context_mode: ContextMode,
    #[serde(default)]
    pub delivery_channel: String,
    #[serde(default)]
    pub delivery_target: String,
    pub max_runs: Option<i64>,
    pub run_count: i64,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_context_mode() -> ContextMode {
    ContextMode::Isolated
}

/// Cron job run record
#[derive(Clone, Debug, Deserialize)]
pub struct CronJobRun {
    pub id: String,
    pub job_id: String,
    pub status: String,
    pub output: String,
    pub error: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub triggered_by: String,
}

/// Create/update request
#[derive(Clone, Debug, Serialize)]
pub struct CronJobRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub schedule_type: ScheduleType,
    pub schedule_expr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_mode: Option<ContextMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<i64>,
}

/// API response wrapper
#[derive(Clone, Debug, Deserialize)]
pub struct CronJobResponse {
    pub success: bool,
    pub job: Option<CronJob>,
}

/// Toggle response
#[derive(Clone, Debug, Deserialize)]
pub struct ToggleResponse {
    pub success: bool,
    pub enabled: bool,
}

/// Run response
#[derive(Clone, Debug, Deserialize)]
pub struct RunResponse {
    pub success: bool,
    pub run_id: String,
}

/// Cron Job API Service
#[derive(Clone)]
pub struct CronJobApiService {
    client: ApiClient,
}

impl CronJobApiService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// List all cron jobs
    pub async fn list_jobs(&self) -> Result<Vec<CronJob>, ApiError> {
        self.client.get("/cron/jobs").await
    }

    /// Get a single job
    pub async fn get_job(&self, id: &str) -> Result<CronJob, ApiError> {
        self.client
            .get(&format!("/cron/jobs/{}", js_sys::encode_uri_component(id)))
            .await
    }

    /// Create a new job
    pub async fn create_job(&self, req: &CronJobRequest) -> Result<CronJobResponse, ApiError> {
        self.client.post("/cron/jobs", req).await
    }

    /// Update a job
    pub async fn update_job(
        &self,
        id: &str,
        req: &CronJobRequest,
    ) -> Result<CronJobResponse, ApiError> {
        self.client
            .put(
                &format!("/cron/jobs/{}", js_sys::encode_uri_component(id)),
                req,
            )
            .await
    }

    /// Delete a job
    pub async fn delete_job(&self, id: &str) -> Result<(), ApiError> {
        self.client
            .delete(&format!("/cron/jobs/{}", js_sys::encode_uri_component(id)))
            .await
    }

    /// Toggle enabled status
    pub async fn toggle_job(&self, id: &str) -> Result<ToggleResponse, ApiError> {
        self.client
            .post(
                &format!("/cron/jobs/{}/toggle", js_sys::encode_uri_component(id)),
                &serde_json::json!({}),
            )
            .await
    }

    /// Manually trigger a job
    pub async fn run_job(&self, id: &str) -> Result<RunResponse, ApiError> {
        self.client
            .post(
                &format!("/cron/jobs/{}/run", js_sys::encode_uri_component(id)),
                &serde_json::json!({}),
            )
            .await
    }

    /// List execution history
    pub async fn list_runs(&self, id: &str) -> Result<Vec<CronJobRun>, ApiError> {
        self.client
            .get(&format!(
                "/cron/jobs/{}/runs",
                js_sys::encode_uri_component(id)
            ))
            .await
    }
}
