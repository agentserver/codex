//! Integration tests for the multi-environment manifest path.
//!
//! Spec reference: `2026-05-05-codex-app-gateway-and-exec-gateway-design.md`
//! § Subsystem 1, P1.

use std::io::Write;

use codex_exec_server::EnvironmentManager;
use codex_exec_server::EnvironmentManagerArgs;
use codex_exec_server::ExecServerRuntimePaths;
use codex_exec_server::ManifestEnvironmentProvider;

fn runtime_paths() -> ExecServerRuntimePaths {
    ExecServerRuntimePaths::new(
        std::env::current_exe().expect("current exe"),
        /*codex_linux_sandbox_exe*/ None,
    )
    .expect("runtime paths")
}

#[tokio::test]
async fn end_to_end_manifest_loads_two_remote_environments() {
    unsafe { std::env::set_var("P1_E2E_TOK", "tok-e2e"); }

    let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
    tmp.write_all(
        br#"{
            "default_environment_id": "exe_alpha",
            "environments": [
                {
                    "id": "exe_alpha",
                    "url": "ws://gw:6060/bridge/exe_alpha",
                    "auth_token_env": "P1_E2E_TOK",
                    "description": "Daisy MBP"
                },
                {
                    "id": "exe_beta",
                    "url": "ws://gw:6060/bridge/exe_beta",
                    "auth_token_env": "P1_E2E_TOK",
                    "description": "EC2"
                }
            ]
        }"#,
    )
    .expect("write");

    let provider = ManifestEnvironmentProvider::from_path(tmp.path().to_path_buf())
        .expect("manifest parses");
    let manager = EnvironmentManager::from_provider(&provider, runtime_paths())
        .await
        .expect("manager builds");

    assert_eq!(manager.default_environment_id(), Some("exe_alpha"));
    let alpha = manager.get_environment("exe_alpha").expect("alpha");
    let beta = manager.get_environment("exe_beta").expect("beta");
    assert!(alpha.is_remote());
    assert!(beta.is_remote());
    assert_eq!(alpha.exec_server_url(), Some("ws://gw:6060/bridge/exe_alpha"));
    assert_eq!(beta.exec_server_url(), Some("ws://gw:6060/bridge/exe_beta"));
}

#[tokio::test]
async fn legacy_single_url_still_works_when_manifest_unset() {
    // Defensive: ensure manifest var is not set in this test.
    unsafe { std::env::remove_var("CODEX_EXEC_SERVERS_JSON"); }
    unsafe { std::env::set_var("CODEX_EXEC_SERVER_URL", "ws://127.0.0.1:8765"); }

    let manager =
        EnvironmentManager::new(EnvironmentManagerArgs::new(runtime_paths())).await;
    assert_eq!(manager.default_environment_id(), Some("remote"));
    assert!(
        manager
            .default_environment()
            .expect("default")
            .is_remote()
    );

    unsafe { std::env::remove_var("CODEX_EXEC_SERVER_URL"); }
}

#[tokio::test]
async fn manifest_descriptions_propagate_to_environment() {
    unsafe { std::env::set_var("P4_DESC_TOK", "tok-d"); }
    let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
    std::io::Write::write_all(
        &mut tmp,
        br#"{
            "environments": [
                {"id":"a","url":"ws://h/a","auth_token_env":"P4_DESC_TOK","description":"Alpha host"},
                {"id":"b","url":"ws://h/b","auth_token_env":"P4_DESC_TOK"}
            ]
        }"#,
    )
    .expect("write");

    let provider = ManifestEnvironmentProvider::from_path(tmp.path().to_path_buf()).expect("p");
    let manager = EnvironmentManager::from_provider(&provider, runtime_paths())
        .await
        .expect("m");
    assert_eq!(
        manager.get_environment("a").expect("a").description(),
        Some("Alpha host")
    );
    assert!(manager.get_environment("b").expect("b").description().is_none());
}
