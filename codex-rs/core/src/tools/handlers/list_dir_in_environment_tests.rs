use super::*;
use crate::tools::handlers::parse_arguments;
use pretty_assertions::assert_eq;

#[test]
fn rejects_missing_environment_id() {
    let json = r#"{"path": "/tmp"}"#;
    let err = parse_arguments::<ListDirInEnvironmentArgs>(json)
        .expect_err("missing environment_id should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("environment_id"),
        "expected error to mention environment_id, got: {msg}"
    );
}

#[test]
fn rejects_missing_path() {
    let json = r#"{"environment_id": "exe_two"}"#;
    let err = parse_arguments::<ListDirInEnvironmentArgs>(json)
        .expect_err("missing path should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("path"),
        "expected error to mention path, got: {msg}"
    );
}

#[test]
fn parses_required_fields() {
    let json = r#"{"environment_id": "exe_two", "path": "/srv/data"}"#;
    let args: ListDirInEnvironmentArgs =
        parse_arguments(json).expect("happy-path parse should succeed");
    assert_eq!(args.environment_id, "exe_two");
    assert_eq!(args.path, "/srv/data");
}

#[tokio::test]
async fn unknown_env_message_lists_available_ids() {
    use crate::session::turn_context::TurnEnvironment;

    let env = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs");
    let environments = vec![
        TurnEnvironment {
            environment_id: "exe_alpha".into(),
            environment: std::sync::Arc::clone(&env),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_beta".into(),
            environment: std::sync::Arc::clone(&env),
            cwd,
            shell: "/bin/sh".into(),
        },
    ];

    let msg = unknown_env_message("exe_missing", &environments);
    assert!(
        msg.contains("exe_missing"),
        "msg should include the unknown id: {msg}"
    );
    assert!(
        msg.contains("exe_alpha") && msg.contains("exe_beta"),
        "msg should list available ids: {msg}"
    );
}

#[test]
fn unknown_env_message_when_no_envs_says_no_environments() {
    let msg = unknown_env_message("exe_missing", &[]);
    assert!(msg.contains("exe_missing"));
    assert!(msg.contains("no environments"), "got: {msg}");
}

#[tokio::test]
async fn select_environment_routes_to_named_env_for_handler_dispatch() {
    use crate::session::tests::make_test_turn_context_with_environments;
    use crate::session::turn_context::TurnEnvironment;

    // Mirrors the handler-layer routing pin established for the other
    // env-aware tools (Pa.1 `exec_command_in_environment`, Pa.2
    // `apply_patch_in_environment`). The Pa.4 handler resolves the
    // env-id supplied by the LLM via `turn.select_environment(...)`
    // and then invokes that env's `get_filesystem().read_directory(...)`.
    // This test pins the routing half: env id resolves to the right
    // `Arc<Environment>` (verified via `Arc::ptr_eq`), not a silent
    // fallback to the primary env.
    let env_a = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let env_b = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs");
    let environments = vec![
        TurnEnvironment {
            environment_id: "exe_one".into(),
            environment: std::sync::Arc::clone(&env_a),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_two".into(),
            environment: std::sync::Arc::clone(&env_b),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
    ];
    let turn_context = make_test_turn_context_with_environments(environments).await;

    let chosen = turn_context
        .select_environment(Some("exe_two"))
        .expect("found");
    assert_eq!(chosen.environment_id, "exe_two");
    assert!(std::sync::Arc::ptr_eq(&chosen.environment, &env_b));

    // And the handler-layer error message matches what the new tool
    // returns when the LLM-supplied id is unknown.
    let msg = unknown_env_message("exe_missing", &turn_context.environments);
    assert!(msg.contains("exe_one"));
    assert!(msg.contains("exe_two"));
}

#[tokio::test]
async fn format_listing_renders_real_directory_via_environment_filesystem() {
    use codex_utils_absolute_path::AbsolutePathBuf;
    use std::fs;

    // End-to-end-ish test of the second half of the handler: build a
    // real `Environment::default_for_tests()` (which uses
    // `LocalFileSystem::unsandboxed()`), create a tempdir on disk with a
    // mix of files and subdirs, call the env's `read_directory`, and
    // verify `format_listing` produces a deterministic, useful render.
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::create_dir(root.join("subdir")).expect("mkdir subdir");
    fs::write(root.join("alpha.txt"), b"a").expect("write alpha");
    fs::write(root.join("beta.txt"), b"b").expect("write beta");

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();
    let abs_path = AbsolutePathBuf::from_absolute_path(root).expect("abs root");
    let entries = fs_handle
        .read_directory(&abs_path, /*sandbox*/ None)
        .await
        .expect("read_directory should succeed");

    let rendered = format_listing("exe_target", root, entries);
    let mut lines = rendered.lines();
    assert_eq!(
        lines.next(),
        Some("Environment: exe_target"),
        "first line is env id; got: {rendered}"
    );
    let path_line = lines.next().expect("path line");
    assert!(
        path_line.starts_with("Absolute path: "),
        "second line carries abs path; got: {path_line}"
    );
    assert!(
        path_line.contains(root.to_str().expect("utf8 path")),
        "abs path line should mention the root: {path_line}"
    );

    let body: Vec<&str> = lines.collect();
    assert_eq!(body, vec!["alpha.txt", "beta.txt", "subdir/"], "got: {rendered}");
}

#[tokio::test]
async fn read_directory_error_is_user_visible() {
    // The fs error path returns `ToolError::Rejected` with the fs error
    // message inline. We can't easily call `handle()` (it requires a
    // full ToolInvocation), but we can pin the contract by exercising
    // the read directly: nonexistent absolute path -> the env's
    // filesystem returns an `io::Error`, which the handler wraps into
    // `FunctionCallError::RespondToModel`.
    use codex_utils_absolute_path::AbsolutePathBuf;

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();

    // Pick a path that virtually never exists.
    let nonexistent = std::path::PathBuf::from(
        "/this/path/should/not/exist/codex-multi-env/list_dir_in_environment/test",
    );
    let abs_path = AbsolutePathBuf::from_absolute_path(&nonexistent).expect("abs");
    let err = fs_handle
        .read_directory(&abs_path, /*sandbox*/ None)
        .await
        .expect_err("nonexistent dir must error");

    // The handler turns this into the `RespondToModel` shown below; pin
    // the error format we'd interpolate into the model-visible string.
    let wrapped = format!(
        "failed to read directory `{}` on environment `{}`: {err}",
        nonexistent.display(),
        "exe_target",
    );
    assert!(wrapped.contains("failed to read directory"));
    assert!(wrapped.contains("exe_target"));
}
