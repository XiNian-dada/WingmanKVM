use std::net::SocketAddr;

use anyhow::Context;
use axum::{Router, routing::get};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wingmankvm=info,tower_http=info".into()),
        )
        .init();

    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let address = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to listen on {address}"))?;

    tracing::info!(%address, "WingmanKVM is listening");
    axum::serve(listener, app)
        .await
        .context("HTTP server failed")
}
