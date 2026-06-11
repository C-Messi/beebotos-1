//! BeeHub - Skill Marketplace for BeeBotOS

use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;

mod handlers;
mod models;
mod storage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(handlers::index))
        .route(
            "/api/skills",
            get(handlers::list_skills).post(handlers::publish_skill),
        )
        .route("/api/skills/:id", get(handlers::get_skill))
        .route("/api/skills/:id/download", get(handlers::download_skill));

    let host = std::env::var("BEEHUB_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("BEEHUB_PORT").unwrap_or_else(|_| "3000".to_string());
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("BeeHub listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
