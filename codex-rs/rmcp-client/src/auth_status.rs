use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use codex_exec_server::ExecServerError;
use codex_exec_server::HttpClient;
use codex_exec_server::HttpHeader;
use codex_exec_server::HttpRequestParams;
use codex_exec_server::HttpRequestResponse;
use codex_exec_server::HttpResponseBodyStream;
use codex_protocol::protocol::McpAuthStatus;
use futures::FutureExt;
use futures::future::BoxFuture;
use reqwest::Method;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;
use tracing::debug;

use crate::mcp_oauth_http::OAuthHttpClient;
use crate::mcp_oauth_http::StreamableHttpOAuthDiscovery;
use crate::oauth::has_oauth_tokens;
use crate::utils::build_default_headers;
use codex_config::types::OAuthCredentialsStoreMode;

/// Determine the authentication status for a streamable HTTP MCP server.
pub async fn determine_streamable_http_auth_status(
    server_name: &str,
    url: &str,
    bearer_token_env_var: Option<&str>,
    http_headers: Option<HashMap<String, String>>,
    env_http_headers: Option<HashMap<String, String>>,
    store_mode: OAuthCredentialsStoreMode,
) -> Result<McpAuthStatus> {
    determine_streamable_http_auth_status_with_client(
        server_name,
        url,
        bearer_token_env_var,
        http_headers,
        env_http_headers,
        store_mode,
        no_proxy_http_client(),
    )
    .await
}

/// Determine the authentication status for a streamable HTTP MCP server using
/// the caller-selected runtime HTTP client.
#[allow(clippy::too_many_arguments)]
pub async fn determine_streamable_http_auth_status_with_client(
    server_name: &str,
    url: &str,
    bearer_token_env_var: Option<&str>,
    http_headers: Option<HashMap<String, String>>,
    env_http_headers: Option<HashMap<String, String>>,
    store_mode: OAuthCredentialsStoreMode,
    http_client: Arc<dyn HttpClient>,
) -> Result<McpAuthStatus> {
    if bearer_token_env_var.is_some() {
        return Ok(McpAuthStatus::BearerToken);
    }

    let default_headers = build_default_headers(http_headers.clone(), env_http_headers.clone())?;
    if default_headers.contains_key(AUTHORIZATION) {
        return Ok(McpAuthStatus::BearerToken);
    }

    if has_oauth_tokens(server_name, url, store_mode)? {
        return Ok(McpAuthStatus::OAuth);
    }

    let oauth_http = OAuthHttpClient::from_default_headers(http_client, default_headers);
    match oauth_http.discover(url).await {
        Ok(Some(_)) => Ok(McpAuthStatus::NotLoggedIn),
        Ok(None) => Ok(McpAuthStatus::Unsupported),
        Err(error) => {
            debug!(
                "failed to detect OAuth support for MCP server `{server_name}` at {url}: {error:?}"
            );
            Ok(McpAuthStatus::Unsupported)
        }
    }
}

/// Attempt to determine whether a streamable HTTP MCP server advertises OAuth login.
pub async fn supports_oauth_login(url: &str) -> Result<bool> {
    Ok(discover_streamable_http_oauth(
        url, /*http_headers*/ None, /*env_http_headers*/ None,
    )
    .await?
    .is_some())
}

pub async fn discover_streamable_http_oauth(
    url: &str,
    http_headers: Option<HashMap<String, String>>,
    env_http_headers: Option<HashMap<String, String>>,
) -> Result<Option<StreamableHttpOAuthDiscovery>> {
    discover_streamable_http_oauth_with_client(
        url,
        http_headers,
        env_http_headers,
        no_proxy_http_client(),
    )
    .await
}

pub(crate) fn no_proxy_http_client() -> Arc<dyn HttpClient> {
    Arc::new(NoProxyReqwestHttpClient)
}

#[derive(Debug, Clone, Default)]
struct NoProxyReqwestHttpClient;

impl HttpClient for NoProxyReqwestHttpClient {
    fn http_request(
        &self,
        params: HttpRequestParams,
    ) -> BoxFuture<'_, std::result::Result<HttpRequestResponse, ExecServerError>> {
        async move { no_proxy_http_request(params).await }.boxed()
    }

    fn http_request_stream(
        &self,
        _params: HttpRequestParams,
    ) -> BoxFuture<
        '_,
        std::result::Result<(HttpRequestResponse, HttpResponseBodyStream), ExecServerError>,
    > {
        async move {
            Err(ExecServerError::Protocol(
                "streaming is not supported for OAuth discovery".to_string(),
            ))
        }
        .boxed()
    }
}

async fn no_proxy_http_request(
    params: HttpRequestParams,
) -> std::result::Result<HttpRequestResponse, ExecServerError> {
    let method = Method::from_bytes(params.method.as_bytes())
        .map_err(|error| ExecServerError::HttpRequest(error.to_string()))?;
    let mut builder = reqwest::Client::builder().no_proxy();
    if let Some(timeout_ms) = params.timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }
    let client = builder
        .build()
        .map_err(|error| ExecServerError::HttpRequest(error.to_string()))?;

    let mut request = client.request(method, &params.url);
    for header in params.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|error| ExecServerError::HttpRequest(error.to_string()))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|error| ExecServerError::HttpRequest(error.to_string()))?;
        request = request.header(name, value);
    }
    if let Some(body) = params.body {
        request = request.body(body.into_inner());
    }

    let response = request
        .send()
        .await
        .map_err(|error| ExecServerError::HttpRequest(error.to_string()))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|value| HttpHeader {
                name: name.to_string(),
                value: value.to_string(),
            })
        })
        .collect();
    let body = response
        .bytes()
        .await
        .map_err(|error| ExecServerError::HttpRequest(error.to_string()))?
        .to_vec();

    Ok(HttpRequestResponse {
        status,
        headers,
        body: body.into(),
    })
}

pub async fn discover_streamable_http_oauth_with_client(
    url: &str,
    http_headers: Option<HashMap<String, String>>,
    env_http_headers: Option<HashMap<String, String>>,
    http_client: Arc<dyn HttpClient>,
) -> Result<Option<StreamableHttpOAuthDiscovery>> {
    let oauth_http = OAuthHttpClient::new(http_client, http_headers, env_http_headers)?;
    oauth_http.discover(url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::Router;
    use axum::routing::get;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use tokio::task::JoinHandle;

    struct TestServer {
        url: String,
        handle: JoinHandle<()>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    async fn spawn_oauth_discovery_server(metadata: serde_json::Value) -> TestServer {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("listener should have address");
        let app = Router::new().route(
            "/.well-known/oauth-authorization-server/mcp",
            get({
                let metadata = metadata.clone();
                move || {
                    let metadata = metadata.clone();
                    async move { Json(metadata) }
                }
            }),
        );
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        TestServer {
            url: format!("http://{address}/mcp"),
            handle,
        }
    }

    struct EnvVarGuard {
        key: String,
        original: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &str, value: &str) -> Self {
            let original = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key: key.to_string(),
                original,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                unsafe {
                    std::env::set_var(&self.key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(&self.key);
                }
            }
        }
    }

    #[tokio::test]
    async fn determine_auth_status_uses_bearer_token_when_authorization_header_present() {
        let status = determine_streamable_http_auth_status(
            "server",
            "not-a-url",
            /*bearer_token_env_var*/ None,
            Some(HashMap::from([(
                "Authorization".to_string(),
                "Bearer token".to_string(),
            )])),
            /*env_http_headers*/ None,
            OAuthCredentialsStoreMode::Keyring,
        )
        .await
        .expect("status should compute");

        assert_eq!(status, McpAuthStatus::BearerToken);
    }

    #[tokio::test]
    #[serial(auth_status_env)]
    async fn determine_auth_status_uses_bearer_token_when_env_authorization_header_present() {
        let _guard = EnvVarGuard::set("CODEX_RMCP_CLIENT_AUTH_STATUS_TEST_TOKEN", "Bearer token");
        let status = determine_streamable_http_auth_status(
            "server",
            "not-a-url",
            /*bearer_token_env_var*/ None,
            /*http_headers*/ None,
            Some(HashMap::from([(
                "Authorization".to_string(),
                "CODEX_RMCP_CLIENT_AUTH_STATUS_TEST_TOKEN".to_string(),
            )])),
            OAuthCredentialsStoreMode::Keyring,
        )
        .await
        .expect("status should compute");

        assert_eq!(status, McpAuthStatus::BearerToken);
    }

    #[tokio::test]
    async fn discover_streamable_http_oauth_returns_normalized_scopes() {
        let server = spawn_oauth_discovery_server(serde_json::json!({
            "authorization_endpoint": "https://example.com/authorize",
            "token_endpoint": "https://example.com/token",
            "scopes_supported": ["profile", " email ", "profile", "", "   "],
        }))
        .await;

        let discovery = discover_streamable_http_oauth(
            &server.url,
            /*http_headers*/ None,
            /*env_http_headers*/ None,
        )
        .await
        .expect("discovery should succeed")
        .expect("oauth support should be detected");

        assert_eq!(
            discovery.scopes_supported,
            Some(vec!["profile".to_string(), "email".to_string()])
        );
    }

    #[tokio::test]
    async fn discover_streamable_http_oauth_ignores_empty_scopes() {
        let server = spawn_oauth_discovery_server(serde_json::json!({
            "authorization_endpoint": "https://example.com/authorize",
            "token_endpoint": "https://example.com/token",
            "scopes_supported": ["", "   "],
        }))
        .await;

        let discovery = discover_streamable_http_oauth(
            &server.url,
            /*http_headers*/ None,
            /*env_http_headers*/ None,
        )
        .await
        .expect("discovery should succeed")
        .expect("oauth support should be detected");

        assert_eq!(discovery.scopes_supported, None);
    }

    #[tokio::test]
    async fn supports_oauth_login_does_not_require_scopes_supported() {
        let server = spawn_oauth_discovery_server(serde_json::json!({
            "authorization_endpoint": "https://example.com/authorize",
            "token_endpoint": "https://example.com/token",
        }))
        .await;

        let supported = supports_oauth_login(&server.url)
            .await
            .expect("support check should succeed");

        assert!(supported);
    }
}
