//! HTTP handlers for BeeWeb Update Server

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use prometheus::Encoder;
use serde::Deserialize;

use crate::metrics::UpdateMetrics;
use crate::models::{
    UpdateCheckRequest, UpdateCheckResponse, UpdateMetricRecord, UpdateReportRequest,
    UpdateReportResponse, UpdateStatus,
};
use crate::storage::Storage;

/// Application state shared across handlers
#[derive(Debug, Clone)]
pub struct AppState {
    pub storage: Storage,
    pub metrics: Arc<UpdateMetrics>,
    pub packages_dir: String,
}

/// Query parameters for version check
#[derive(Debug, Deserialize)]
pub struct CheckQuery {
    pub app: String,
    pub version: String,
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_channel() -> String {
    "stable".to_string()
}

/// Health check endpoint
pub async fn health() -> &'static str {
    "ok"
}

/// Index endpoint
pub async fn index() -> &'static str {
    "BeeWeb Update Server"
}

/// Check for available updates
///
/// Supports both GET query parameters and POST JSON body
pub async fn check_update_get(
    State(state): State<AppState>,
    Query(query): Query<CheckQuery>,
) -> Result<Json<UpdateCheckResponse>, StatusCode> {
    state.metrics.update_check_total.inc();

    let current_version = match semver::Version::parse(&query.version) {
        Ok(v) => v,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    let release = state
        .storage
        .find_latest_release(&query.app, &query.channel)
        .await;

    let response = match release {
        Some(record) => {
            let latest_version = semver::Version::parse(&record.version.to_string())
                .unwrap_or_else(|_| semver::Version::new(0, 0, 0));

            if latest_version > current_version {
                state
                    .metrics
                    .record_available_update(&query.app, &query.channel);

                UpdateCheckResponse {
                    has_update: true,
                    version_info: Some(record.version_info),
                }
            } else {
                UpdateCheckResponse {
                    has_update: false,
                    version_info: None,
                }
            }
        }
        None => UpdateCheckResponse {
            has_update: false,
            version_info: None,
        },
    };

    Ok(Json(response))
}

/// Check for available updates via POST
pub async fn check_update_post(
    State(state): State<AppState>,
    Json(req): Json<UpdateCheckRequest>,
) -> Result<Json<UpdateCheckResponse>, StatusCode> {
    state.metrics.update_check_total.inc();

    let current_version = match semver::Version::parse(&req.current_version) {
        Ok(v) => v,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    let release = state
        .storage
        .find_latest_release(&req.app_name, &req.channel)
        .await;

    let response = match release {
        Some(record) => {
            let latest_version = semver::Version::parse(&record.version.to_string())
                .unwrap_or_else(|_| semver::Version::new(0, 0, 0));

            // Filter packages by platform if specified
            let mut version_info = record.version_info;
            if !req.platform.is_empty() {
                version_info.packages.retain(|pkg| {
                    format!("{:?}", pkg.platform).to_lowercase() == req.platform.to_lowercase()
                        || match pkg.platform {
                            crate::models::Platform::Linux => req.platform.contains("linux"),
                            crate::models::Platform::Windows => req.platform.contains("windows"),
                            crate::models::Platform::MacOS => {
                                req.platform.contains("macos") || req.platform.contains("darwin")
                            }
                            crate::models::Platform::Wasm => req.platform.contains("wasm"),
                        }
                });
            }

            if latest_version > current_version && !version_info.packages.is_empty() {
                state
                    .metrics
                    .record_available_update(&req.app_name, &req.channel);

                UpdateCheckResponse {
                    has_update: true,
                    version_info: Some(version_info),
                }
            } else {
                UpdateCheckResponse {
                    has_update: false,
                    version_info: None,
                }
            }
        }
        None => UpdateCheckResponse {
            has_update: false,
            version_info: None,
        },
    };

    Ok(Json(response))
}

/// Download a package by ID
///
/// Supports Range header for resumable downloads
pub async fn download_package(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    let start = Instant::now();

    // Validate package_id to prevent path traversal
    if package_id.contains("..") || package_id.contains('/') || package_id.contains('\\') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (release, _package) = state
        .storage
        .find_package(&package_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    // Read from packages_dir - must exist
    let package_path = std::path::Path::new(&state.packages_dir)
        .join(&release.app_name)
        .join(&release.version.to_string())
        .join(&package_id);

    let body = match tokio::fs::read(&package_path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("Failed to read package {}: {}", package_id, e);
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let len = body.len();

    // Handle Range header for resume support
    let (status, body_bytes) = if let Some(range_hdr) = headers.get(header::RANGE) {
        let range_str = range_hdr.to_str().unwrap_or("");
        if let Some((start, end)) = parse_range(range_str, len) {
            let end = end.unwrap_or(len - 1).min(len - 1);
            if start >= len || start > end {
                return Err(StatusCode::RANGE_NOT_SATISFIABLE);
            }
            let partial = body[start..=end].to_vec();
            (StatusCode::PARTIAL_CONTENT, partial)
        } else {
            (StatusCode::OK, body)
        }
    } else {
        (StatusCode::OK, body)
    };

    let duration = start.elapsed().as_secs_f64();
    state
        .metrics
        .record_download(body_bytes.len() as f64, duration);

    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", package_id),
        )
        .header(header::ACCEPT_RANGES, "bytes")
        .body(axum::body::Body::from(body_bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

/// Parse HTTP Range header
fn parse_range(range: &str, _total_len: usize) -> Option<(usize, Option<usize>)> {
    let range = range.strip_prefix("bytes=")?;
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    let start: usize = parts[0].parse().ok()?;
    let end: Option<usize> = if parts[1].is_empty() {
        None
    } else {
        parts[1].parse().ok()
    };

    Some((start, end))
}

/// Report update status from client
pub async fn report_update(
    State(state): State<AppState>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<Json<UpdateReportResponse>, StatusCode> {
    let metric = UpdateMetricRecord {
        id: uuid::Uuid::new_v4().to_string(),
        app_name: req.app_name.clone(),
        device_id: req.device_id.clone(),
        current_version: req.current_version.clone(),
        target_version: req.target_version.clone(),
        status: req.status,
        duration_secs: req.duration_secs,
        error: req.error.clone(),
        reported_at: chrono::Utc::now(),
    };

    // Update metrics based on status
    match req.status {
        UpdateStatus::Completed => {
            state
                .metrics
                .record_success(&req.app_name, req.duration_secs as f64);
        }
        UpdateStatus::Failed => {
            let error_type = req.error.as_deref().unwrap_or("unknown");
            state.metrics.record_failure(&req.app_name, error_type);
        }
        UpdateStatus::RolledBack => {
            state.metrics.record_rollback();
        }
        _ => {}
    }

    // Record version gauge
    if let Ok(ver) = semver::Version::parse(&req.current_version) {
        state.metrics.record_version(
            &req.app_name,
            &req.device_id,
            &req.current_version,
            ver.patch as f64,
        );
    }

    state.storage.save_metric(metric).await;

    Ok(Json(UpdateReportResponse {
        success: true,
        message: "Report received".to_string(),
    }))
}

/// Get metrics summary (admin endpoint)
pub async fn metrics_summary(
    State(state): State<AppState>,
) -> Json<crate::storage::MetricsSummary> {
    Json(state.storage.get_metrics_summary().await)
}

/// Prometheus metrics scrape endpoint
pub async fn prometheus_metrics(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = state.metrics.registry().gather();
    let mut buffer = String::new();

    encoder
        .encode_utf8(&metric_families, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, encoder.format_type())
        .body(axum::body::Body::from(buffer))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
}

/// List available releases (admin endpoint)
pub async fn list_releases(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = state.storage.get_metrics().await;
    let releases: Vec<_> = metrics
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "app_name": m.app_name,
                "device_id": m.device_id,
                "status": format!("{:?}", m.status),
                "reported_at": m.reported_at,
            })
        })
        .collect();

    Json(releases)
}
