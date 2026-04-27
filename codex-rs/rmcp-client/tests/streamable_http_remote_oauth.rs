//! Regression coverage for remote-environment Streamable HTTP OAuth.
//!
//! The OAuth issuer in this test uses an unresolvable hostname. If any
//! discovery, registration, or token-exchange request bypasses the injected
//! `HttpClient`, the test fails before the callback can complete.

mod streamable_http_test_support;

use std::ffi::OsString;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;
use std::sync::PoisonError;

use codex_config::types::OAuthCredentialsStoreMode;
use codex_exec_server::ExecServerError;
use codex_exec_server::HttpClient;
use codex_exec_server::HttpHeader;
use codex_exec_server::HttpRequestParams;
use codex_exec_server::HttpRequestResponse;
use codex_exec_server::HttpResponseBodyStream;
use codex_rmcp_client::StoredOAuthTokens;
use codex_rmcp_client::WrappedOAuthTokenResponse;
use codex_rmcp_client::perform_oauth_login_return_url_with_client;
use codex_rmcp_client::save_oauth_tokens;
use futures::FutureExt as _;
use futures::future::BoxFuture;
use oauth2::AccessToken;
use oauth2::EmptyExtraTokenFields;
use oauth2::RefreshToken;
use oauth2::basic::BasicTokenType;
use pretty_assertions::assert_eq;
use rmcp::transport::auth::OAuthTokenResponse;
use serde_json::Value;
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;

use streamable_http_test_support::call_echo_tool;
use streamable_http_test_support::create_remote_oauth_client;
use streamable_http_test_support::expected_echo_result;
use streamable_http_test_support::spawn_exec_server;
use streamable_http_test_support::spawn_streamable_http_server_with_oauth_bearer;

#[derive(Clone, Default)]
struct RecordingHttpClient {
    requests: Arc<Mutex<Vec<HttpRequestParams>>>,
}

impl RecordingHttpClient {
    fn recorded_requests(&self) -> Vec<HttpRequestParams> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl HttpClient for RecordingHttpClient {
    fn http_request(
        &self,
        params: HttpRequestParams,
    ) -> BoxFuture<'_, Result<HttpRequestResponse, ExecServerError>> {
        let requests = Arc::clone(&self.requests);
        async move {
            requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(params.clone());

            let response = match (params.method.as_str(), params.url.as_str()) {
                ("GET", "http://oauth.test/.well-known/oauth-authorization-server/mcp") => {
                    json_response(json!({
                        "authorization_endpoint": "http://oauth.test/authorize",
                        "token_endpoint": "http://oauth.test/token",
                    "registration_endpoint": "http://oauth.test/register",
                    "scopes_supported": ["tools.read", "tools.write"],
                    "response_types_supported": ["code"],
                    }))?
                }
                ("POST", "http://oauth.test/register") => json_response(json!({
                    "client_id": "registered-client",
                }))?,
                ("POST", "http://oauth.test/token") => json_response(json!({
                    "access_token": "remote-access-token",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "refresh_token": "remote-refresh-token",
                }))?,
                _ => {
                    return Err(ExecServerError::HttpRequest(format!(
                        "unexpected HTTP request: {} {}",
                        params.method, params.url
                    )));
                }
            };

            Ok(response)
        }
        .boxed()
    }

    fn http_request_stream(
        &self,
        params: HttpRequestParams,
    ) -> BoxFuture<'_, Result<(HttpRequestResponse, HttpResponseBodyStream), ExecServerError>> {
        async move {
            Err(ExecServerError::HttpRequest(format!(
                "unexpected streaming HTTP request: {} {}",
                params.method, params.url
            )))
        }
        .boxed()
    }
}

fn json_response(body: Value) -> Result<HttpRequestResponse, ExecServerError> {
    let body = serde_json::to_vec(&body)
        .map_err(|err| ExecServerError::HttpRequest(format!("serialize JSON response: {err}")))?;
    Ok(HttpRequestResponse {
        status: 200,
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "application/json".to_string(),
        }],
        body: body.into(),
    })
}

struct TempCodexHome {
    _guard: MutexGuard<'static, ()>,
    previous: Option<OsString>,
    _dir: TempDir,
}

impl TempCodexHome {
    fn new() -> anyhow::Result<Self> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let guard = LOCK
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let previous = std::env::var_os("CODEX_HOME");
        let dir = TempDir::new()?;
        unsafe {
            std::env::set_var("CODEX_HOME", dir.path());
        }
        Ok(Self {
            _guard: guard,
            previous,
            _dir: dir,
        })
    }
}

impl Drop for TempCodexHome {
    fn drop(&mut self) {
        unsafe {
            match self.previous.as_ref() {
                Some(value) => std::env::set_var("CODEX_HOME", value),
                None => std::env::remove_var("CODEX_HOME"),
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn browser_callback_flow_uses_injected_http_client_for_oauth_requests() -> anyhow::Result<()>
{
    let _codex_home = TempCodexHome::new()?;
    let http_client = RecordingHttpClient::default();

    let handle = perform_oauth_login_return_url_with_client(
        "remote-oauth-test",
        "http://oauth.test/mcp",
        OAuthCredentialsStoreMode::File,
        /*http_headers*/ None,
        /*env_http_headers*/ None,
        &["tools.read".to_string()],
        /*oauth_resource*/ None,
        Some(10),
        /*callback_port*/ None,
        /*callback_url*/ None,
        Arc::new(http_client.clone()),
    )
    .await?;

    let authorization_url = reqwest::Url::parse(handle.authorization_url())?;
    let mut state = None;
    let mut redirect_uri = None;
    for (name, value) in authorization_url.query_pairs() {
        match name.as_ref() {
            "state" => state = Some(value.into_owned()),
            "redirect_uri" => redirect_uri = Some(value.into_owned()),
            _ => {}
        }
    }

    let state = state.expect("authorization URL includes state");
    let redirect_uri = redirect_uri.expect("authorization URL includes redirect_uri");
    let callback_url = format!("{redirect_uri}?code=provider-code&state={state}");
    let callback_response = reqwest::get(callback_url).await?;
    assert_eq!(callback_response.status(), reqwest::StatusCode::OK);

    handle.wait().await?;

    let requests = http_client.recorded_requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| (request.method.as_str(), request.url.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                "GET",
                "http://oauth.test/.well-known/oauth-authorization-server/mcp"
            ),
            ("POST", "http://oauth.test/register"),
            ("POST", "http://oauth.test/token"),
        ]
    );

    let registration_body: Value = serde_json::from_slice(
        &requests[1]
            .body
            .as_ref()
            .expect("registration request has a body")
            .0,
    )?;
    assert_eq!(
        registration_body,
        json!({
            "client_name": "Codex",
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "token_endpoint_auth_method": "none",
            "response_types": ["code"],
        })
    );

    let token_body = String::from_utf8(
        requests[2]
            .body
            .as_ref()
            .expect("token request has a body")
            .0
            .clone(),
    )?;
    assert!(token_body.contains("grant_type=authorization_code"));
    assert!(token_body.contains("code=provider-code"));
    assert!(token_body.contains("client_id=registered-client"));
    assert!(token_body.contains("code_verifier="));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn stored_oauth_refreshes_and_authenticates_remote_streamable_http() -> anyhow::Result<()> {
    let _codex_home = TempCodexHome::new()?;
    let refresh_token = "remote-refresh-token";
    let refreshed_access_token = "refreshed-remote-access-token";
    let (_server, base_url) =
        spawn_streamable_http_server_with_oauth_bearer(refreshed_access_token, refresh_token)
            .await?;
    let exec_server = spawn_exec_server().await?;

    save_oauth_tokens(
        "test-streamable-http-remote-oauth",
        &expired_oauth_tokens(
            "test-streamable-http-remote-oauth",
            &format!("{base_url}/mcp"),
            "expired-access-token",
            refresh_token,
        ),
        OAuthCredentialsStoreMode::File,
    )?;

    let client = create_remote_oauth_client(&base_url, exec_server.client.clone()).await?;
    let result = call_echo_tool(&client, "remote-oauth").await?;

    assert_eq!(result, expected_echo_result("remote-oauth"));

    Ok(())
}

fn expired_oauth_tokens(
    server_name: &str,
    url: &str,
    access_token: &str,
    refresh_token: &str,
) -> StoredOAuthTokens {
    let mut token_response = OAuthTokenResponse::new(
        AccessToken::new(access_token.to_string()),
        BasicTokenType::Bearer,
        EmptyExtraTokenFields {},
    );
    token_response.set_refresh_token(Some(RefreshToken::new(refresh_token.to_string())));

    StoredOAuthTokens {
        server_name: server_name.to_string(),
        url: url.to_string(),
        client_id: "stored-client".to_string(),
        token_response: WrappedOAuthTokenResponse(token_response),
        expires_at: Some(0),
    }
}
