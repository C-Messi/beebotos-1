//! BeeWeb Server - Remote Software Update Server for BeeBotOS
//!
//! Provides APIs for:
//! - Version checking: GET /api/v1/updates/check
//! - Package download: GET /api/v1/updates/download/{package_id}
//! - Update reporting: POST /api/v1/updates/report
//! - Metrics: GET /metrics

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{info, warn, Level};

mod db;
mod handlers;
mod metrics;
mod models;
mod signature;
mod storage;

use handlers::AppState;
use metrics::UpdateMetrics;
use storage::Storage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "beeweb=info,tower_http=info".into()),
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("Starting BeeWeb Update Server...");

    // Initialize storage (SQLite if DATABASE_URL is set, otherwise in-memory)
    let storage = if let Ok(db_url) = std::env::var("DATABASE_URL") {
        match db::DbStorage::new(&db_url).await {
            Ok(db) => {
                info!("SQLite storage initialized at {}", db_url);
                let s = Storage::with_db(db);
                s.seed_sample_data().await;
                s
            }
            Err(e) => {
                warn!(
                    "Failed to initialize SQLite storage: {}, falling back to memory",
                    e
                );
                let s = Storage::new();
                s.seed_sample_data().await;
                s
            }
        }
    } else {
        let s = Storage::new();
        s.seed_sample_data().await;
        s
    };

    // Initialize metrics
    let metrics = Arc::new(UpdateMetrics::new()?);
    info!("Metrics initialized");

    // Configuration
    let packages_dir =
        std::env::var("BEEWEB_PACKAGES_DIR").unwrap_or_else(|_| "/data/packages".to_string());
    let port = std::env::var("BEEWEB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080u16);

    // Application state
    let state = AppState {
        storage,
        metrics,
        packages_dir,
    };

    // Build router
    let app = create_router(state);

    let addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("BeeWeb Update Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("BeeWeb Update Server shutdown complete");
    Ok(())
}

/// Create the Axum router with all routes
fn create_router(state: AppState) -> Router {
    Router::new()
        // Public API
        .route("/", get(handlers::index))
        .route("/health", get(handlers::health))
        .route("/api/v1/updates/check", get(handlers::check_update_get))
        .route("/api/v1/updates/check", post(handlers::check_update_post))
        .route(
            "/api/v1/updates/download/:package_id",
            get(handlers::download_package),
        )
        .route("/api/v1/updates/report", post(handlers::report_update))
        // Admin / monitoring
        .route("/metrics", get(handlers::prometheus_metrics))
        .route("/admin/metrics/summary", get(handlers::metrics_summary))
        .route("/admin/reports", get(handlers::list_releases))
        // Middleware
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, shutting down..."),
        _ = terminate => info!("Received SIGTERM, shutting down..."),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    use super::*;

    async fn test_app() -> Router {
        let storage = Storage::new();
        storage.seed_sample_data().await;
        let metrics = Arc::new(UpdateMetrics::new().unwrap());
        let state = AppState {
            storage,
            metrics,
            packages_dir: "/tmp/packages".to_string(),
        };
        create_router(state)
    }

    #[tokio::test]
    async fn test_health_check() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_check_update() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/updates/check?app=gateway&version=1.0.0&channel=stable")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_report_update() {
        let app = test_app().await;
        let body = serde_json::json!({
            "app_name": "gateway",
            "device_id": "dev_test_001",
            "current_version": "1.0.0",
            "target_version": "1.1.0",
            "status": "completed",
            "duration_secs": 120
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/updates/report")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_download_path_traversal_blocked() {
        let app = test_app().await;
        // Test encoded path traversal attempt
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/updates/download/pkg%2f..%2fetc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Axum decodes %2f to /, so the path contains '/' which is rejected
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_download_unknown_package_returns_404() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/updates/download/nonexistent-package")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
