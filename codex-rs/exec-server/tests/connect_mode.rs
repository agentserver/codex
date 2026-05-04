#![cfg(unix)]

mod common;

use std::process::Stdio;
use std::time::Duration;

use anyhow::anyhow;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_exec_server::FsReadFileParams;
use codex_exec_server::FsReadFileResponse;
use codex_exec_server::FsWriteFileParams;
use codex_exec_server::FsWriteFileResponse;
use codex_exec_server::InitializeParams;
use codex_exec_server::InitializeResponse;
use codex_utils_absolute_path::AbsolutePathBuf;
use common::exec_server::test_codex_helper_paths;
use futures::SinkExt;
use futures::StreamExt;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;
use uuid::Uuid;

const SPAWN_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

/// End-to-end test for the `--connect` mode added to `codex exec-server`.
/// Verifies:
///   1. The binary dials the configured URL outbound.
///   2. The bearer token from `--auth-token-env <ENV>` is sent as
///      `Authorization: Bearer <token>` on the WebSocket upgrade request.
///   3. After connection, the standard exec-server JSON-RPC protocol works:
///      initialize → fs/writeFile → fs/readFile, identical to `--listen`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_server_connect_mode_round_trip() -> anyhow::Result<()> {
    let helper_paths = test_codex_helper_paths()?;
    let codex_home = TempDir::new()?;

    // Bind a TCP listener; we'll act as the "remote harness" that the
    // exec-server connects to.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let listen_addr = listener.local_addr()?;
    let connect_url = format!("ws://{listen_addr}");
    let bearer_token = "test-token-abc123";

    // Spawn the binary in --connect mode with a bearer token env var.
    let mut child = Command::new(&helper_paths.codex_exe);
    child.args([
        "exec-server",
        "--connect",
        &connect_url,
        "--auth-token-env",
        "AGENTSERVER_TOKEN",
    ]);
    child.env("AGENTSERVER_TOKEN", bearer_token);
    child.env("CODEX_HOME", codex_home.path());
    child.stdin(Stdio::null());
    child.stdout(Stdio::piped());
    child.stderr(Stdio::inherit());
    child.kill_on_drop(true);
    let mut child = child.spawn()?;

    // Accept the inbound TCP connection from the spawned binary, do the
    // WS upgrade, and capture the request headers so we can assert on
    // the Authorization header.
    let (tcp_stream, _peer) = timeout(SPAWN_TIMEOUT, listener.accept())
        .await
        .map_err(|_| anyhow!("timed out waiting for exec-server to connect back"))??;

    let mut captured_authorization: Option<String> = None;
    let websocket = accept_hdr_async(
        tcp_stream,
        |req: &WebSocketRequest, response: WebSocketResponse| {
            if let Some(value) = req.headers().get("authorization") {
                captured_authorization = value.to_str().map(|s| s.to_string()).ok();
            }
            Ok(response)
        },
    )
    .await?;

    // The binary signals "connected" on its stdout once the WS upgrade
    // completes. Drain that line to make sure ordering is what we expect.
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("missing child stdout"))?;
    let mut reader = BufReader::new(stdout).lines();
    let connected_line = timeout(SPAWN_TIMEOUT, reader.next_line())
        .await?
        .map_err(|err| anyhow!("read connected line: {err}"))?
        .ok_or_else(|| anyhow!("child closed stdout before connected line"))?;
    assert!(
        connected_line.starts_with("connected:"),
        "unexpected stdout line: {connected_line}"
    );

    // Authorization header should carry the bearer token.
    assert_eq!(
        captured_authorization.as_deref(),
        Some(format!("Bearer {bearer_token}").as_str()),
        "exec-server did not send the bearer token on the upgrade request"
    );

    // Standard exec-server protocol: initialize → fs/writeFile → fs/readFile.
    let mut harness = HarnessClient::new(websocket);

    // 1. initialize
    let initialize_id = harness
        .send_request(
            "initialize",
            serde_json::to_value(InitializeParams {
                client_name: "connect-mode-test".to_string(),
                resume_session_id: None,
            })?,
        )
        .await?;
    let initialize_resp = match harness.next_event().await? {
        JSONRPCMessage::Response(JSONRPCResponse { id, result }) if id == initialize_id => result,
        other => panic!("expected initialize response, got {other:?}"),
    };
    let initialize: InitializeResponse = serde_json::from_value(initialize_resp)?;
    Uuid::parse_str(&initialize.session_id)?;

    // The exec-server requires an `initialized` notification before any
    // filesystem methods can be used.
    harness
        .send_notification("initialized", serde_json::json!({}))
        .await?;

    // 2. fs/writeFile against a temp file
    let work_dir = TempDir::new()?;
    let target_path = work_dir.path().join("hello.txt");
    assert!(target_path.is_absolute());
    let absolute_path = AbsolutePathBuf::try_from(target_path.clone())
        .map_err(|err| anyhow!("path should be absolute: {err}"))?;
    let payload = b"hello from connect mode";
    let write_id = harness
        .send_request(
            "fs/writeFile",
            serde_json::to_value(FsWriteFileParams {
                path: absolute_path.clone(),
                data_base64: BASE64_STANDARD.encode(payload),
                sandbox: None,
            })?,
        )
        .await?;
    let write_resp = match harness.next_event().await? {
        JSONRPCMessage::Response(JSONRPCResponse { id, result }) if id == write_id => result,
        other => panic!("expected writeFile response, got {other:?}"),
    };
    let _: FsWriteFileResponse = serde_json::from_value(write_resp)?;

    // 3. fs/readFile and verify content
    let read_id = harness
        .send_request(
            "fs/readFile",
            serde_json::to_value(FsReadFileParams {
                path: absolute_path,
                sandbox: None,
            })?,
        )
        .await?;
    let read_resp = match harness.next_event().await? {
        JSONRPCMessage::Response(JSONRPCResponse { id, result }) if id == read_id => result,
        other => panic!("expected readFile response, got {other:?}"),
    };
    let read: FsReadFileResponse = serde_json::from_value(read_resp)?;
    let decoded = BASE64_STANDARD.decode(&read.data_base64)?;
    assert_eq!(&decoded, payload);

    // Tear down: closing the harness's WS triggers the spawned binary to exit.
    drop(harness);
    let _ = timeout(SPAWN_TIMEOUT, child.wait()).await;
    Ok(())
}

/// Minimal client wrapper around the WebSocket — we can't reuse the existing
/// `ExecServerHarness` because that one drives a *spawned* server, and here
/// the spawned process is the *client*, so we need to be the server.
struct HarnessClient {
    websocket: WebSocketStream<tokio::net::TcpStream>,
    next_request_id: i64,
}

impl HarnessClient {
    fn new(websocket: WebSocketStream<tokio::net::TcpStream>) -> Self {
        Self {
            websocket,
            next_request_id: 1,
        }
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<RequestId> {
        let id = RequestId::Integer(self.next_request_id);
        self.next_request_id += 1;
        let msg = JSONRPCMessage::Request(JSONRPCRequest {
            id: id.clone(),
            method: method.to_string(),
            params: Some(params),
            trace: None,
        });
        let text = serde_json::to_string(&msg)?;
        self.websocket.send(Message::Text(text.into())).await?;
        Ok(id)
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        let msg = JSONRPCMessage::Notification(JSONRPCNotification {
            method: method.to_string(),
            params: Some(params),
        });
        let text = serde_json::to_string(&msg)?;
        self.websocket.send(Message::Text(text.into())).await?;
        Ok(())
    }

    async fn next_event(&mut self) -> anyhow::Result<JSONRPCMessage> {
        loop {
            let message = timeout(EVENT_TIMEOUT, self.websocket.next())
                .await?
                .ok_or_else(|| anyhow!("websocket closed before event"))??;
            match message {
                Message::Text(text) => {
                    return Ok(serde_json::from_str::<JSONRPCMessage>(&text)?);
                }
                Message::Binary(bytes) => {
                    return Ok(serde_json::from_slice::<JSONRPCMessage>(&bytes)?);
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => {
                    return Err(anyhow!("websocket closed by peer"));
                }
                Message::Frame(_) => continue,
            }
        }
    }
}
