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

    /// Returns the id of the environment that should be the session default.
    /// Default impl returns None (preserves existing `DefaultEnvironmentProvider`
    /// behavior, which lets `EnvironmentManager::from_environments` fall back
    /// to its REMOTE/LOCAL heuristic).
    fn default_environment_id(&self) -> Option<&str> {
        None
    }
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

use std::path::PathBuf;

/// Provider that loads a JSON manifest of multiple remote environments.
///
/// Activated by setting `CODEX_EXEC_SERVERS_JSON=<path>`. See spec § P1 for
/// the manifest schema and selection semantics.
#[derive(Debug, Clone)]
pub struct ManifestEnvironmentProvider {
    manifest: ManifestFile,
    default_environment_id: String,
}

impl ManifestEnvironmentProvider {
    /// Reads + validates a manifest from disk. Returns an error for
    /// malformed JSON, empty `environments[]`, duplicate ids, or a
    /// `default_environment_id` not present in `environments`.
    ///
    /// Note: this validation runs at construction; the per-entry
    /// `auth_token_env` lookup is deferred to `get_environments` because
    /// the env var may be set after the provider is constructed in some
    /// test setups. (Production code reads it eagerly via the env var
    /// CODEX_EXEC_GATEWAY_TOKEN already set by codex-app-gateway before
    /// spawning `codex exec`.)
    pub fn from_path(path: PathBuf) -> Result<Self, ExecServerError> {
        let bytes = std::fs::read(&path).map_err(|err| {
            ExecServerError::Protocol(format!(
                "failed to read manifest at {}: {err}",
                path.display()
            ))
        })?;
        let manifest: ManifestFile = serde_json::from_slice(&bytes).map_err(|err| {
            ExecServerError::Protocol(format!(
                "failed to parse manifest at {}: {err}",
                path.display()
            ))
        })?;

        if manifest.environments.is_empty() {
            return Err(ExecServerError::Protocol(
                "manifest environments list is empty".to_string(),
            ));
        }

        let mut seen = std::collections::HashSet::new();
        for entry in &manifest.environments {
            if entry.id.is_empty() {
                return Err(ExecServerError::Protocol(
                    "manifest entry has empty id".to_string(),
                ));
            }
            if !seen.insert(entry.id.clone()) {
                return Err(ExecServerError::Protocol(format!(
                    "manifest contains duplicate environment id: {}",
                    entry.id
                )));
            }
            if entry.url.is_empty() {
                return Err(ExecServerError::Protocol(format!(
                    "manifest entry {} has empty url",
                    entry.id
                )));
            }
            if entry.auth_token_env.is_empty() {
                return Err(ExecServerError::Protocol(format!(
                    "manifest entry {} has empty auth_token_env",
                    entry.id
                )));
            }
        }

        let default_environment_id = match &manifest.default_environment_id {
            Some(id) => {
                if !seen.contains(id) {
                    return Err(ExecServerError::Protocol(format!(
                        "default_environment_id `{id}` is not in environments[]"
                    )));
                }
                id.clone()
            }
            None => manifest.environments[0].id.clone(),
        };

        Ok(Self {
            manifest,
            default_environment_id,
        })
    }

    /// Convenience: build from `CODEX_EXEC_SERVERS_JSON`. Returns Ok(None)
    /// when the var is unset.
    pub fn from_env() -> Result<Option<Self>, ExecServerError> {
        match std::env::var(CODEX_EXEC_SERVERS_JSON_ENV_VAR) {
            Ok(path) if !path.trim().is_empty() => {
                Self::from_path(PathBuf::from(path)).map(Some)
            }
            _ => Ok(None),
        }
    }

}

#[async_trait]
impl EnvironmentProvider for ManifestEnvironmentProvider {
    fn default_environment_id(&self) -> Option<&str> {
        Some(self.default_environment_id.as_str())
    }

    async fn get_environments(
        &self,
        local_runtime_paths: &ExecServerRuntimePaths,
    ) -> Result<HashMap<String, Environment>, ExecServerError> {
        let mut out = HashMap::with_capacity(self.manifest.environments.len());
        for entry in &self.manifest.environments {
            let token = std::env::var(&entry.auth_token_env).map_err(|_| {
                ExecServerError::Protocol(format!(
                    "manifest entry `{}` references env var `{}`, which is not set",
                    entry.id, entry.auth_token_env
                ))
            })?;
            let mut environment = Environment::remote_with_auth(
                entry.url.clone(),
                Some(token),
                Some(local_runtime_paths.clone()),
            );
            if let Some(description) = &entry.description {
                environment = environment.with_description(description.clone());
            }
            out.insert(entry.id.clone(), environment);
        }
        Ok(out)
    }
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

    use std::io::Write;

    fn write_manifest(json: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("temp file");
        f.write_all(json.as_bytes()).expect("write");
        f.flush().expect("flush");
        f
    }

    #[tokio::test]
    async fn manifest_provider_loads_explicit_default() {
        // SAFETY: setting env vars in tests is OK as this is the only test mutating P1_AUTH_A.
        unsafe { std::env::set_var("P1_AUTH_A", "tok-a"); }
        let f = write_manifest(
            r#"{
                "default_environment_id": "exe_b",
                "environments": [
                    {"id": "exe_a", "url": "ws://h/a", "auth_token_env": "P1_AUTH_A"},
                    {"id": "exe_b", "url": "ws://h/b", "auth_token_env": "P1_AUTH_A"}
                ]
            }"#,
        );
        let provider = super::ManifestEnvironmentProvider::from_path(f.path().to_path_buf())
            .expect("provider");
        let runtime_paths = test_runtime_paths();
        let envs = provider.get_environments(&runtime_paths).await.expect("envs");
        assert!(envs.contains_key("exe_a"));
        assert!(envs.contains_key("exe_b"));
        assert_eq!(provider.default_environment_id(), Some("exe_b"));
    }

    #[tokio::test]
    async fn manifest_provider_falls_back_to_first_when_default_absent() {
        unsafe { std::env::set_var("P1_AUTH_B", "tok-b"); }
        let f = write_manifest(
            r#"{
                "environments": [
                    {"id": "exe_first", "url": "ws://h/1", "auth_token_env": "P1_AUTH_B"},
                    {"id": "exe_second", "url": "ws://h/2", "auth_token_env": "P1_AUTH_B"}
                ]
            }"#,
        );
        let provider = super::ManifestEnvironmentProvider::from_path(f.path().to_path_buf())
            .expect("provider");
        assert_eq!(provider.default_environment_id(), Some("exe_first"));
    }

    #[tokio::test]
    async fn manifest_provider_rejects_empty_environments() {
        let f = write_manifest(r#"{"environments": []}"#);
        let err = super::ManifestEnvironmentProvider::from_path(f.path().to_path_buf())
            .expect_err("should fail");
        assert!(err.to_string().contains("environments"));
    }

    #[tokio::test]
    async fn manifest_provider_rejects_unset_auth_env() {
        unsafe { std::env::remove_var("P1_AUTH_MISSING"); }
        let f = write_manifest(
            r#"{
                "environments": [
                    {"id": "x", "url": "ws://h/x", "auth_token_env": "P1_AUTH_MISSING"}
                ]
            }"#,
        );
        let runtime_paths = test_runtime_paths();
        let provider = super::ManifestEnvironmentProvider::from_path(f.path().to_path_buf())
            .expect("provider parses");
        let err = provider.get_environments(&runtime_paths).await
            .expect_err("missing env should fail");
        assert!(err.to_string().contains("P1_AUTH_MISSING"));
    }

    #[tokio::test]
    async fn manifest_provider_rejects_default_id_not_in_list() {
        unsafe { std::env::set_var("P1_AUTH_C", "tok-c"); }
        let f = write_manifest(
            r#"{
                "default_environment_id": "exe_does_not_exist",
                "environments": [
                    {"id": "exe_real", "url": "ws://h/r", "auth_token_env": "P1_AUTH_C"}
                ]
            }"#,
        );
        let err = super::ManifestEnvironmentProvider::from_path(f.path().to_path_buf())
            .expect_err("should fail");
        assert!(err.to_string().contains("default_environment_id"));
    }

    #[tokio::test]
    async fn manifest_provider_rejects_duplicate_ids() {
        unsafe { std::env::set_var("P1_AUTH_D", "tok-d"); }
        let f = write_manifest(
            r#"{
                "environments": [
                    {"id": "dup", "url": "ws://h/1", "auth_token_env": "P1_AUTH_D"},
                    {"id": "dup", "url": "ws://h/2", "auth_token_env": "P1_AUTH_D"}
                ]
            }"#,
        );
        let err = super::ManifestEnvironmentProvider::from_path(f.path().to_path_buf())
            .expect_err("duplicate id should fail");
        assert!(err.to_string().contains("dup"));
    }
}
