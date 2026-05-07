//! Gateway internal update API handlers
//!
//! Routes:
//! - GET /api/v1/system/updates/status
//! - POST /api/v1/system/updates/check
//! - POST /api/v1/system/updates/apply
//! - POST /api/v1/system/updates/rollback

use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::updater::service::GatewayUpdateService;

/// Shared gateway application state with updater
#[derive(Clone)]
pub struct UpdaterState {
    pub updater: Arc<GatewayUpdateService>,
}

impl UpdaterState {
    pub fn new(updater: Arc<GatewayUpdateService>) -> Self {
        Self { updater }
    }
}

/// Update status response
#[derive(Serialize)]
pub struct UpdateStatusResponse {
    pub status: String,
    pub current_version: String,
    pub target_version: Option<String>,
    pub download_progress: u8,
    pub error: Option<String>,
    pub retry_count: u32,
}

/// Check update response
#[derive(Serialize)]
pub struct CheckUpdateResponse {
    pub has_update: bool,
    pub version_info: Option<beebotos_update_client::models::VersionInfo>,
}

/// Apply update request
#[derive(Deserialize)]
pub struct ApplyUpdateRequest {
    #[allow(dead_code)]
    pub force: Option<bool>,
}

/// Apply update response
#[derive(Serialize)]
pub struct ApplyUpdateResponse {
    pub success: bool,
    pub message: String,
}

/// Rollback response
#[derive(Serialize)]
pub struct RollbackResponse {
    pub success: bool,
    pub message: String,
}

/// GET /api/v1/system/updates/status
pub async fn get_update_status(
    State(state): State<UpdaterState>,
) -> impl IntoResponse {
    let update_state = state.updater.get_state().await;
    Json(UpdateStatusResponse {
        status: format!("{:?}", update_state.status).to_lowercase(),
        current_version: update_state.current_version.to_string(),
        target_version: update_state.target_version.map(|v| v.to_string()),
        download_progress: update_state.download_progress,
        error: update_state.error,
        retry_count: update_state.retry_count,
    })
}

/// POST /api/v1/system/updates/check
pub async fn check_update(
    State(state): State<UpdaterState>,
) -> impl IntoResponse {
    match state.updater.trigger_check().await {
        Ok(info) => {
            let has_update = info.is_some();
            Json(CheckUpdateResponse { has_update, version_info: info })
        }
        Err(_e) => {
            Json(CheckUpdateResponse {
                has_update: false,
                version_info: None,
            })
        }
    }
}

/// POST /api/v1/system/updates/apply
pub async fn apply_update(
    State(state): State<UpdaterState>,
    Json(_req): Json<ApplyUpdateRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // First check if update is available
    let info = match state.updater.check_update().await {
        Ok(Some(info)) => info,
        Ok(None) => {
            return Json(ApplyUpdateResponse {
                success: false,
                message: "No update available".to_string(),
            });
        }
        Err(e) => {
            return Json(ApplyUpdateResponse {
                success: false,
                message: format!("Update check failed: {}", e),
            });
        }
    };

    // Pre-install health check
    if !state.updater.health_check().await {
        return Json(ApplyUpdateResponse {
            success: false,
            message: "Gateway health check failed before update".to_string(),
        });
    }

    // Download
    let path = match state.updater.download_update(&info).await {
        Ok(p) => p,
        Err(e) => {
            let _ = state.updater.report_status(
                &info.version.to_string(),
                beebotos_update_client::models::UpdateStatus::Failed,
                start.elapsed().as_secs(),
                Some(format!("Download failed: {}", e)),
            ).await;
            return Json(ApplyUpdateResponse {
                success: false,
                message: format!("Download failed: {}", e),
            });
        }
    };

    // Verify
    match state.updater.verify_package(&path, &info).await {
        Ok(true) => {}
        Ok(false) => {
            let _ = state.updater.report_status(
                &info.version.to_string(),
                beebotos_update_client::models::UpdateStatus::Failed,
                start.elapsed().as_secs(),
                Some("Verification failed".to_string()),
            ).await;
            return Json(ApplyUpdateResponse {
                success: false,
                message: "Package verification failed".to_string(),
            });
        }
        Err(e) => {
            let _ = state.updater.report_status(
                &info.version.to_string(),
                beebotos_update_client::models::UpdateStatus::Failed,
                start.elapsed().as_secs(),
                Some(format!("Verification error: {}", e)),
            ).await;
            return Json(ApplyUpdateResponse {
                success: false,
                message: format!("Verification error: {}", e),
            });
        }
    }

    // Note: Actual installation requires process restart which is platform-specific
    // The orchestrator (K8s/systemd) should:
    // 1. Replace binary
    // 2. Restart service
    // 3. Run post-install health check (via /health endpoint)
    // 4. If health check fails, trigger rollback
    let _ = state.updater.report_status(
        &info.version.to_string(),
        beebotos_update_client::models::UpdateStatus::Installing,
        start.elapsed().as_secs(),
        None,
    ).await;

    Json(ApplyUpdateResponse {
        success: true,
        message: format!(
            "Update {} downloaded and verified (took {}s). Restart gateway to apply. Health check will run post-restart.",
            info.version,
            start.elapsed().as_secs()
        ),
    })
}

/// POST /api/v1/system/updates/rollback
pub async fn rollback_update(
    State(_state): State<UpdaterState>,
) -> impl IntoResponse {
    Json(RollbackResponse {
        success: true,
        message: "Rollback requested. This requires manual intervention or orchestrator restart.".to_string(),
    })
}
