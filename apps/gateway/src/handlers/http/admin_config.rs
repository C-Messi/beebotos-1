//! Admin Configuration HTTP Handlers
//!
//! Provides configuration export and hot-reload for operators.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde_json::json;

use crate::AppState;

/// Export current effective configuration
pub async fn get_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let loaded_config = if let Some(manager) = &state.config_manager {
        manager.config().await.clone()
    } else {
        state.config.clone()
    };

    // Serialize the current effective config
    let config_json = serde_json::to_value(&loaded_config)
        .map_err(|e| GatewayError::internal(format!("Failed to serialize config: {}", e)))?;

    Ok(Json(json!({
        "config": config_json,
        "source": "runtime",
        "reloadable": state.config_manager.is_some(),
    })))
}

/// Hot-reload configuration
pub async fn reload_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["admin"])?;

    if let Some(ref manager) = state.config_manager {
        match manager.reload().await {
            Ok(_) => {
                let current_config = manager.config().await.clone();
                match state.llm_service.reload_config(current_config).await {
                    Ok(()) => Ok((
                        StatusCode::OK,
                        Json(json!({
                            "message": "Configuration and LLM providers reloaded successfully",
                            "status": "ok",
                        })),
                    )),
                    Err(e) => Ok((
                        StatusCode::OK,
                        Json(json!({
                            "message": format!("Configuration reloaded but LLM provider reload failed: {}", e),
                            "status": "partial",
                        })),
                    )),
                }
            }
            Err(e) => Ok((
                StatusCode::OK,
                Json(json!({
                    "message": format!("ConfigCenter reload failed: {}. Local config remains active.", e),
                    "status": "partial",
                })),
            )),
        }
    } else {
        Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "message": "Config manager not initialized",
                "status": "unavailable",
            })),
        ))
    }
}
