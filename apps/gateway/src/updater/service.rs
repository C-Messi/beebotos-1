//! Gateway update service

use std::sync::Arc;

use beebotos_update_client::client::{ConsoleProgress, NativeUpdateClient, UpdateClient};
use beebotos_update_client::config::UpdateConfig;
use beebotos_update_client::models::{UpdateState, UpdateStatus, VersionInfo};
use tokio::sync::RwLock;

/// Gateway update service
#[derive(Clone)]
pub struct GatewayUpdateService {
    client: Arc<NativeUpdateClient>,
    state: Arc<RwLock<UpdateState>>,
    scheduler_state: Arc<RwLock<Option<SchedulerState>>>,
}

#[derive(Debug)]
struct SchedulerState {
    _handle: tokio::task::JoinHandle<()>,
}

impl GatewayUpdateService {
    pub fn new(config: UpdateConfig) -> Result<Self, beebotos_update_client::error::UpdateError> {
        let client = Arc::new(NativeUpdateClient::new(config)?);
        let initial_state = client.state();
        Ok(Self {
            client,
            state: Arc::new(RwLock::new(initial_state)),
            scheduler_state: Arc::new(RwLock::new(None)),
        })
    }

    /// Start scheduled update checks
    pub async fn start_scheduler(&self) {
        let client = self.client.clone();
        let check_cron = client.config().check_cron.clone();

        let handle = tokio::spawn(async move {
            loop {
                let interval = parse_check_interval(&check_cron);
                tokio::time::sleep(interval).await;

                tracing::info!("Scheduled update check triggered");
                match client.check_update().await {
                    Ok(Some(info)) => {
                        tracing::info!("Update available: {}", info.version);
                        if info.mandatory || client.config().auto_install {
                            tracing::info!(
                                "Auto-install enabled, would install {} (requires restart)",
                                info.version
                            );
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("No update available");
                    }
                    Err(e) => {
                        tracing::warn!("Scheduled update check failed: {}", e);
                    }
                }
            }
        });

        let mut sched = self.scheduler_state.write().await;
        *sched = Some(SchedulerState { _handle: handle });
    }

    /// Check for available updates
    pub async fn check_update(
        &self,
    ) -> Result<Option<VersionInfo>, beebotos_update_client::error::UpdateError> {
        let result = self.client.check_update().await;
        if let Ok(Some(ref info)) = result {
            let mut state = self.state.write().await;
            state.target_version = Some(info.version.clone());
            state.status = UpdateStatus::Idle;
        }
        result
    }

    /// Get current update state
    pub async fn get_state(&self) -> UpdateState {
        let client_state = self.client.state();
        let mut state = self.state.write().await;
        *state = client_state;
        state.clone()
    }

    /// Trigger an update check (can be called from scheduler or API)
    pub async fn trigger_check(
        &self,
    ) -> Result<Option<VersionInfo>, beebotos_update_client::error::UpdateError> {
        self.check_update().await
    }

    /// Download the update package
    pub async fn download_update(
        &self,
        info: &VersionInfo,
    ) -> Result<std::path::PathBuf, beebotos_update_client::error::UpdateError> {
        let package = select_package(&info.packages)?;
        let progress = ConsoleProgress;
        self.client.download(&package, &progress).await
    }

    /// Verify the downloaded package
    pub async fn verify_package(
        &self,
        path: &std::path::Path,
        info: &VersionInfo,
    ) -> Result<bool, beebotos_update_client::error::UpdateError> {
        let package = select_package(&info.packages)?;
        self.client.verify(path, &package).await
    }

    /// Report update status to BeeWeb server
    pub async fn report_status(
        &self,
        target_version: &str,
        status: UpdateStatus,
        duration_secs: u64,
        error: Option<String>,
    ) -> Result<(), beebotos_update_client::error::UpdateError> {
        self.client
            .report_status(target_version, status, duration_secs, error)
            .await
    }

    /// Health check: verify gateway is running properly
    pub async fn health_check(&self) -> bool {
        // In production, this should verify:
        // 1. Database connectivity
        // 2. Key API endpoints respond
        // 3. Memory usage is within bounds
        // For now, we assume healthy if we can read our own state
        let state = self.get_state().await;
        !matches!(state.status, UpdateStatus::Failed)
    }
}

/// Select the best package for current platform
fn select_package(
    packages: &[beebotos_update_client::models::PackageInfo],
) -> Result<beebotos_update_client::models::PackageInfo, beebotos_update_client::error::UpdateError>
{
    beebotos_update_client::select_package(packages)
}

/// Parse cron-like check interval (simplified: treat as seconds for demo)
fn parse_check_interval(cron: &Option<String>) -> std::time::Duration {
    // For production, use a proper cron parser like `cron` crate
    // Default: check every 24 hours
    match cron.as_deref() {
        Some("0 0 3 * * *") => std::time::Duration::from_secs(24 * 60 * 60),
        Some(_) => std::time::Duration::from_secs(60 * 60), // hourly for custom
        None => std::time::Duration::from_secs(24 * 60 * 60),
    }
}
