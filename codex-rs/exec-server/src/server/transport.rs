use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::ConnectInfo;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::any;
use axum::routing::get;
use tokio::net::TcpListener;

use crate::ExecServerRuntimePaths;
use crate::connection::JsonRpcConnection;
use crate::server::processor::ConnectionProcessor;
use crate::server::status::ExecServerStatusState;

pub const DEFAULT_LISTEN_URL: &str = "ws://127.0.0.1:0";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ExecServerListenUrlParseError {
    UnsupportedListenUrl(String),
    InvalidWebSocketListenUrl(String),
}

impl std::fmt::Display for ExecServerListenUrlParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecServerListenUrlParseError::UnsupportedListenUrl(listen_url) => write!(
                f,
                "unsupported --listen URL `{listen_url}`; expected `ws://IP:PORT`"
            ),
            ExecServerListenUrlParseError::InvalidWebSocketListenUrl(listen_url) => write!(
                f,
                "invalid websocket --listen URL `{listen_url}`; expected `ws://IP:PORT`"
            ),
        }
    }
}

impl std::error::Error for ExecServerListenUrlParseError {}

pub(crate) fn parse_listen_url(
    listen_url: &str,
) -> Result<SocketAddr, ExecServerListenUrlParseError> {
    if let Some(socket_addr) = listen_url.strip_prefix("ws://") {
        return socket_addr.parse::<SocketAddr>().map_err(|_| {
            ExecServerListenUrlParseError::InvalidWebSocketListenUrl(listen_url.to_string())
        });
    }

    Err(ExecServerListenUrlParseError::UnsupportedListenUrl(
        listen_url.to_string(),
    ))
}

pub(crate) async fn run_transport(
    listen_url: &str,
    runtime_paths: ExecServerRuntimePaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind_address = parse_listen_url(listen_url)?;
    run_websocket_listener(bind_address, runtime_paths).await
}

async fn run_websocket_listener(
    bind_address: SocketAddr,
    runtime_paths: ExecServerRuntimePaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(bind_address).await?;
    let local_addr = listener.local_addr()?;
    let status_state = ExecServerStatusState::new(runtime_paths.clone());
    let processor = ConnectionProcessor::new(runtime_paths, status_state);
    tracing::info!("codex-exec-server listening on ws://{local_addr}");
    println!("ws://{local_addr}");
    std::io::stdout().flush()?;
    eprintln!("codex exec-server listening on ws://{local_addr}");
    eprintln!("  readyz: http://{local_addr}/readyz");
    eprintln!("  healthz: http://{local_addr}/healthz");
    eprintln!("  status: http://{local_addr}/status");
    eprintln!("  metrics: http://{local_addr}/metrics");

    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/status", get(status))
        .route("/metrics", get(metrics))
        .fallback(any(websocket_upgrade))
        .with_state(Arc::new(processor));
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

async fn readyz(State(processor): State<Arc<ConnectionProcessor>>) -> impl IntoResponse {
    match processor.readiness().await {
        Ok(()) => (StatusCode::OK, "ready\n"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "not ready\n"),
    }
}

async fn status(State(processor): State<Arc<ConnectionProcessor>>) -> impl IntoResponse {
    Json(processor.status_snapshot().await)
}

async fn metrics(State(processor): State<Arc<ConnectionProcessor>>) -> impl IntoResponse {
    let snapshot = processor.status_snapshot().await;
    let metrics = processor.render_prometheus_metrics(&snapshot);
    ([("content-type", "text/plain; version=0.0.4")], metrics)
}

async fn websocket_upgrade(
    websocket: WebSocketUpgrade,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    State(processor): State<Arc<ConnectionProcessor>>,
) -> Response {
    websocket
        .on_upgrade(move |websocket| async move {
            processor
                .run_connection(JsonRpcConnection::from_axum_websocket(
                    websocket,
                    format!("exec-server websocket {peer_addr}"),
                ))
                .await;
        })
        .into_response()
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;
