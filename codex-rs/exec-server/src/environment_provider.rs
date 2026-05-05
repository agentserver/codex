use std::collections::HashMap;

use async_trait::async_trait;

use crate::Environment;
use crate::ExecServerError;
use crate::ExecServerRuntimePaths;
use crate::environment::CODEX_EXEC_SERVER_URL_ENV_VAR;
use crate::environment::LOCAL_ENVIRONMENT_ID;
use crate::environment::REMOTE_ENVIRONMENT_ID;

/// Lists the concrete environments available to Codex.
///
/// Implementations should return the provider-owned startup snapshot that
/// `EnvironmentManager` will cache. Providers that want the local environment to
/// be addressable by id should include it explicitly in the returned map.
#[async_trait]
pub trait EnvironmentProvider: Send + Sync {
    /// Returns the environments available for a new manager.
    async fn get_environments(
        &self,
        local_runtime_paths: &ExecServerRuntimePaths,
    ) -> Result<HashMap<String, Environment>, ExecServerError>;
}

/// Default provider backed by `CODEX_EXEC_SERVER_URL`.
#[derive(Clone, Debug)]
pub struct DefaultEnvironmentProvider {
    exec_server_url: Option<String>,
}

impl DefaultEnvironmentProvider {
    /// Builds a provider from an already-read raw `CODEX_EXEC_SERVER_URL` value.
    pub fn new(exec_server_url: Option<String>) -> Self {
        Self { exec_server_url }
    }

    /// Builds a provider by reading `CODEX_EXEC_SERVER_URL`.
    pub fn from_env() -> Self {
        Self::new(std::env::var(CODEX_EXEC_SERVER_URL_ENV_VAR).ok())
    }

    pub(crate) fn environments(
        &self,
        local_runtime_paths: &ExecServerRuntimePaths,
    ) -> HashMap<String, Environment> {
        let mut environments = HashMap::from([(
            LOCAL_ENVIRONMENT_ID.to_string(),
            Environment::local(local_runtime_paths.clone()),
        )]);
        let exec_server_url = normalize_exec_server_url(self.exec_server_url.clone()).0;

        if let Some(exec_server_url) = exec_server_url {
            environments.insert(
                REMOTE_ENVIRONMENT_ID.to_string(),
                Environment::remote_inner(exec_server_url, Some(local_runtime_paths.clone())),
            );
        }

        environments
    }
}

#[async_trait]
impl EnvironmentProvider for DefaultEnvironmentProvider {
    async fn get_environments(
        &self,
        local_runtime_paths: &ExecServerRuntimePaths,
    ) -> Result<HashMap<String, Environment>, ExecServerError> {
        Ok(self.environments(local_runtime_paths))
    }
}

pub(crate) fn normalize_exec_server_url(exec_server_url: Option<String>) -> (Option<String>, bool) {
    match exec_server_url.as_deref().map(str::trim) {
        None | Some("") => (None, false),
        Some(url) if url.eq_ignore_ascii_case("none") => (None, true),
        Some(url) => (Some(url.to_string()), false),
    }
}

/// Environment variable that, when set, points to a JSON manifest of multiple
/// remote environments. See spec § P1 for schema details. When set, this
/// supersedes `CODEX_EXEC_SERVER_URL` (and a warning is logged).
pub const CODEX_EXEC_SERVERS_JSON_ENV_VAR: &str = "CODEX_EXEC_SERVERS_JSON";

/// Top-level structure of the manifest file referenced by
/// `CODEX_EXEC_SERVERS_JSON`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestFile {
    /// Optional. If set, must match the `id` of an entry in `environments`.
    /// If unset, the first entry in `environments` is treated as default.
    #[serde(default)]
    pub default_environment_id: Option<String>,
    pub environments: Vec<ManifestEntry>,
}

/// One execution environment in the manifest.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestEntry {
    /// Stable id used by the LLM to select this environment via tool calls.
    pub id: String,
    /// Websocket URL the codex process dials to reach this environment.
    pub url: String,
    /// Name of the environment variable that holds the bearer token used to
    /// authenticate the websocket dial. Per spec § Capability token, this is
    /// typically `CODEX_EXEC_GATEWAY_TOKEN`.
    pub auth_token_env: String,
    /// Free-form description rendered into the `<environments>` block in P4.
    #[serde(default)]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ExecServerRuntimePaths;

    fn test_runtime_paths() -> ExecServerRuntimePaths {
        ExecServerRuntimePaths::new(
            std::env::current_exe().expect("current exe"),
            /*codex_linux_sandbox_exe*/ None,
        )
        .expect("runtime paths")
    }

    #[tokio::test]
    async fn default_provider_returns_local_environment_when_url_is_missing() {
        let provider = DefaultEnvironmentProvider::new(/*exec_server_url*/ None);
        let runtime_paths = test_runtime_paths();
        let environments = provider
            .get_environments(&runtime_paths)
            .await
            .expect("environments");

        assert!(!environments[LOCAL_ENVIRONMENT_ID].is_remote());
        assert_eq!(
            environments[LOCAL_ENVIRONMENT_ID].local_runtime_paths(),
            Some(&runtime_paths)
        );
        assert!(!environments.contains_key(REMOTE_ENVIRONMENT_ID));
    }

    #[tokio::test]
    async fn default_provider_returns_local_environment_when_url_is_empty() {
        let provider = DefaultEnvironmentProvider::new(Some(String::new()));
        let runtime_paths = test_runtime_paths();
        let environments = provider
            .get_environments(&runtime_paths)
            .await
            .expect("environments");

        assert!(!environments[LOCAL_ENVIRONMENT_ID].is_remote());
        assert!(!environments.contains_key(REMOTE_ENVIRONMENT_ID));
    }

    #[tokio::test]
    async fn default_provider_returns_local_environment_for_none_value() {
        let provider = DefaultEnvironmentProvider::new(Some("none".to_string()));
        let runtime_paths = test_runtime_paths();
        let environments = provider
            .get_environments(&runtime_paths)
            .await
            .expect("environments");

        assert!(!environments[LOCAL_ENVIRONMENT_ID].is_remote());
        assert!(!environments.contains_key(REMOTE_ENVIRONMENT_ID));
    }

    #[tokio::test]
    async fn default_provider_adds_remote_environment_for_websocket_url() {
        let provider = DefaultEnvironmentProvider::new(Some("ws://127.0.0.1:8765".to_string()));
        let runtime_paths = test_runtime_paths();
        let environments = provider
            .get_environments(&runtime_paths)
            .await
            .expect("environments");

        assert!(!environments[LOCAL_ENVIRONMENT_ID].is_remote());
        let remote_environment = &environments[REMOTE_ENVIRONMENT_ID];
        assert!(remote_environment.is_remote());
        assert_eq!(
            remote_environment.exec_server_url(),
            Some("ws://127.0.0.1:8765")
        );
    }

    #[tokio::test]
    async fn default_provider_normalizes_exec_server_url() {
        let provider = DefaultEnvironmentProvider::new(Some(" ws://127.0.0.1:8765 ".to_string()));
        let runtime_paths = test_runtime_paths();
        let environments = provider
            .get_environments(&runtime_paths)
            .await
            .expect("environments");

        assert_eq!(
            environments[REMOTE_ENVIRONMENT_ID].exec_server_url(),
            Some("ws://127.0.0.1:8765")
        );
    }

    #[test]
    fn manifest_file_parses_minimal_valid_payload() {
        let json = r#"{
            "default_environment_id": "exe_alpha",
            "environments": [
                {
                    "id": "exe_alpha",
                    "url": "ws://gw:6060/bridge/exe_alpha",
                    "auth_token_env": "CODEX_EXEC_GATEWAY_TOKEN",
                    "description": "Daisy MBP"
                }
            ]
        }"#;
        let parsed: super::ManifestFile = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.default_environment_id.as_deref(), Some("exe_alpha"));
        assert_eq!(parsed.environments.len(), 1);
        let entry = &parsed.environments[0];
        assert_eq!(entry.id, "exe_alpha");
        assert_eq!(entry.url, "ws://gw:6060/bridge/exe_alpha");
        assert_eq!(entry.auth_token_env, "CODEX_EXEC_GATEWAY_TOKEN");
        assert_eq!(entry.description.as_deref(), Some("Daisy MBP"));
    }

    #[test]
    fn manifest_entry_description_is_optional() {
        let json = r#"{"id":"e","url":"ws://x","auth_token_env":"T"}"#;
        let entry: super::ManifestEntry = serde_json::from_str(json).expect("parse");
        assert!(entry.description.is_none());
    }

    #[test]
    fn manifest_env_var_constant_value() {
        assert_eq!(super::CODEX_EXEC_SERVERS_JSON_ENV_VAR, "CODEX_EXEC_SERVERS_JSON");
    }
}
