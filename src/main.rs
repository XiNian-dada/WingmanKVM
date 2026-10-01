use anyhow::Context;
use axum::serve::ListenerExt;

mod auth;
mod config;
mod devices;
mod proxy_protocol;
mod web;
mod web_ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wingmankvm=info,tower_http=info".into()),
        )
        .init();

    let state = web::AppState::load().context("failed to load WingmanKVM state")?;
    let address = state.server_address().await?;
    let proxy_mode = state.proxy_protocol_mode().await;
    let app = web::router(state);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to listen on {address}"))?;
    let listener = proxy_protocol::ProxyProtocolListener::new(listener, proxy_mode);

    tracing::info!(%address, ?proxy_mode, "WingmanKVM is listening");
    axum::serve(
        listener.tap_io(|_| {}),
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .context("HTTP server failed")
}

