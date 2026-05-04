use std::io::Write as _;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tracing::warn;

use crate::ExecServerRuntimePaths;
use crate::connection::JsonRpcConnection;
use crate::server::processor::ConnectionProcessor;

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

/// Runs the exec-server in outbound-connect mode: dial `connect_url` as a
/// WebSocket client, send `Authorization: Bearer <auth_token>` if provided,
/// then run the standard `ConnectionProcessor` against the resulting
/// connection. The protocol handler is identical to `--listen` mode — only
/// the connection establishment direction differs.
///
/// The function returns once the connection terminates (peer disconnect,
/// IO error, or processor finish). It does NOT loop / reconnect; callers
/// who want reconnection should wrap this in their own retry policy.
pub async fn run_connect_mode(
    connect_url: &str,
    auth_token: Option<&str>,
    runtime_paths: ExecServerRuntimePaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut request = connect_url.into_client_request().map_err(|err| {
        format!("invalid --connect URL `{connect_url}`: {err}")
    })?;
    if let Some(token) = auth_token {
        let header_value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|err| format!("invalid auth token for Authorization header: {err}"))?;
        request.headers_mut().insert(AUTHORIZATION, header_value);
    }

    tracing::info!("codex-exec-server connecting to {connect_url}");
    let (websocket, _response) = connect_async(request).await.map_err(|err| {
        format!("failed to connect to {connect_url}: {err}")
    })?;
    tracing::info!("codex-exec-server connected to {connect_url}");
    println!("connected: {connect_url}");
    std::io::stdout().flush()?;

    let processor = ConnectionProcessor::new(runtime_paths);
    processor
        .run_connection(JsonRpcConnection::from_websocket(
            websocket,
            format!("exec-server connect {connect_url}"),
        ))
        .await;
    Ok(())
}

async fn run_websocket_listener(
    bind_address: SocketAddr,
    runtime_paths: ExecServerRuntimePaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(bind_address).await?;
    let local_addr = listener.local_addr()?;
    let processor = ConnectionProcessor::new(runtime_paths);
    tracing::info!("codex-exec-server listening on ws://{local_addr}");
    println!("ws://{local_addr}");
    std::io::stdout().flush()?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let processor = processor.clone();
        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(websocket) => {
                    processor
                        .run_connection(JsonRpcConnection::from_websocket(
                            websocket,
                            format!("exec-server websocket {peer_addr}"),
                        ))
                        .await;
                }
                Err(err) => {
                    warn!(
                        "failed to accept exec-server websocket connection from {peer_addr}: {err}"
                    );
                }
            }
        });
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;
