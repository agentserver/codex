use std::io::Write as _;
use std::net::SocketAddr;
use std::time::Duration;

use codex_utils_rustls_provider::ensure_rustls_crypto_provider;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tracing::warn;
use url::Url;

use crate::ExecServerRuntimePaths;
use crate::connection::JsonRpcConnection;
use crate::server::processor::ConnectionProcessor;

pub const DEFAULT_LISTEN_URL: &str = "ws://127.0.0.1:0";

/// Maximum time to wait for the WebSocket upgrade handshake when running in
/// `--connect` mode. Mirrors `app-server-client::remote::CONNECT_TIMEOUT`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ExecServerConnectUrlParseError {
    InvalidUrl { url: String, reason: String },
    UnsupportedScheme(String),
    AuthTokenOverCleartext(String),
}

impl std::fmt::Display for ExecServerConnectUrlParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecServerConnectUrlParseError::InvalidUrl { url, reason } => {
                write!(f, "invalid --connect URL `{url}`: {reason}")
            }
            ExecServerConnectUrlParseError::UnsupportedScheme(url) => write!(
                f,
                "unsupported --connect URL `{url}`; expected `ws://` or `wss://`"
            ),
            ExecServerConnectUrlParseError::AuthTokenOverCleartext(url) => write!(
                f,
                "auth tokens require `wss://` or loopback `ws://` URLs; got `{url}`"
            ),
        }
    }
}

impl std::error::Error for ExecServerConnectUrlParseError {}

/// Parses and validates a `--connect` URL. Accepts `ws://` and `wss://`. When
/// `with_auth_token` is true, additionally rejects plaintext `ws://` URLs to
/// non-loopback hosts to avoid leaking the bearer credential. Mirrors the
/// guard in `app-server-client::remote::websocket_url_supports_auth_token`.
pub(crate) fn parse_connect_url(
    connect_url: &str,
    with_auth_token: bool,
) -> Result<Url, ExecServerConnectUrlParseError> {
    let url = Url::parse(connect_url).map_err(|err| {
        ExecServerConnectUrlParseError::InvalidUrl {
            url: connect_url.to_string(),
            reason: err.to_string(),
        }
    })?;
    match url.scheme() {
        "ws" | "wss" => {}
        _ => {
            return Err(ExecServerConnectUrlParseError::UnsupportedScheme(
                connect_url.to_string(),
            ));
        }
    }
    if with_auth_token && !url_supports_auth_token(&url) {
        return Err(ExecServerConnectUrlParseError::AuthTokenOverCleartext(
            connect_url.to_string(),
        ));
    }
    Ok(url)
}

/// Returns true when sending an `Authorization: Bearer ...` header on a WS
/// upgrade to `url` is safe (TLS, or loopback).
fn url_supports_auth_token(url: &Url) -> bool {
    match (url.scheme(), url.host()) {
        ("wss", Some(_)) => true,
        ("ws", Some(url::Host::Domain(domain))) => domain.eq_ignore_ascii_case("localhost"),
        ("ws", Some(url::Host::Ipv4(addr))) => addr.is_loopback(),
        ("ws", Some(url::Host::Ipv6(addr))) => addr.is_loopback(),
        _ => false,
    }
}

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
/// The function does NOT loop / reconnect. On peer disconnect, transport
/// failure, or any other connection termination, it returns `Err` so the
/// caller (typically the `codex exec-server` binary) can exit with a
/// non-zero status. Callers wanting reconnection should run this in a
/// retry loop.
pub async fn run_connect_mode(
    connect_url: &str,
    auth_token: Option<&str>,
    runtime_paths: ExecServerRuntimePaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = parse_connect_url(connect_url, auth_token.is_some())?;

    let mut request = url.as_str().into_client_request().map_err(|err| {
        format!("invalid --connect URL `{connect_url}`: {err}")
    })?;
    if let Some(token) = auth_token {
        let header_value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|err| format!("invalid auth token for Authorization header: {err}"))?;
        request.headers_mut().insert(AUTHORIZATION, header_value);
    }

    ensure_rustls_crypto_provider();

    tracing::info!("codex-exec-server connecting to {connect_url}");
    let (websocket, response) = timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| {
            format!(
                "timed out after {:?} connecting to `{connect_url}`",
                CONNECT_TIMEOUT
            )
        })?
        .map_err(|err| format!("failed to connect to `{connect_url}`: {err}"))?;
    tracing::info!(
        "codex-exec-server connected to {connect_url}: status={}",
        response.status()
    );
    println!("connected: {connect_url}");
    std::io::stdout().flush()?;

    let processor = ConnectionProcessor::new(runtime_paths);
    processor
        .run_connection(JsonRpcConnection::from_websocket(
            websocket,
            format!("exec-server connect {connect_url}"),
        ))
        .await;

    // The processor returns when the WebSocket is closed (peer Close frame,
    // transport error, or stream end). Surface that as a non-zero exit so
    // a supervising launcher can distinguish disconnect from clean
    // shutdown. Mirrors the `AppServerEvent::Disconnected` →
    // `FatalExitRequest` pattern used by `RemoteAppServerClient` consumers.
    let msg = format!("exec-server connection to `{connect_url}` ended");
    tracing::warn!("{msg}");
    Err(msg.into())
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
