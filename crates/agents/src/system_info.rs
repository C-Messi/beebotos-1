//! System Information Provider
//!
//! Abstract trait for querying system-level information that lives outside
//! the agents crate (e.g. Gateway-layer cron jobs, agents, metrics, etc.).
//! The Gateway layer implements this trait and injects it into the Agent.

use serde::{Deserialize, Serialize};

/// Information about a single cron job managed by the Gateway layer
/// (frontend control panel → CronJobService).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayCronJobInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// "at" / "every" / "cron"
    pub schedule_type: String,
    pub schedule_expr: String,
    pub timezone: String,
    pub enabled: bool,
    pub run_count: i64,
    pub last_run_at: Option<String>,
}

/// Request to create a Gateway-managed cron job from an agent tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGatewayCronJobRequest {
    pub name: String,
    pub description: Option<String>,
    /// "at" / "every" / "cron"
    pub schedule_type: String,
    pub schedule_expr: String,
    pub timezone: Option<String>,
    pub prompt: String,
    pub enabled: Option<bool>,
    /// "main" / "isolated"
    pub context_mode: Option<String>,
    pub delivery_channel: Option<String>,
    pub delivery_target: Option<String>,
    pub max_runs: Option<i64>,
    pub created_by: Option<String>,
}

/// Summary information about an agent in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummaryInfo {
    pub agent_id: String,
    pub state: String,
    pub registered_at: Option<String>,
    pub state_changed_at: Option<String>,
    pub total_tasks: u64,
    pub successful_tasks: u64,
    pub failed_tasks: u64,
    pub last_error: Option<String>,
}

/// Trait for querying system information from external layers.
#[async_trait::async_trait]
pub trait SystemInfoProvider: Send + Sync {
    /// List cron jobs created via the Gateway frontend control panel.
    async fn list_gateway_cron_jobs(&self) -> Result<Vec<GatewayCronJobInfo>, String>;

    /// Create a Gateway-managed cron job.
    async fn create_gateway_cron_job(
        &self,
        _request: CreateGatewayCronJobRequest,
    ) -> Result<GatewayCronJobInfo, String> {
        Err("Creating cron jobs is not supported by this runtime".to_string())
    }

    /// List all agents registered in the system.
    /// Default implementation returns empty list for backward compatibility.
    async fn list_agents(&self) -> Result<Vec<AgentSummaryInfo>, String> {
        Ok(Vec::new())
    }
}
