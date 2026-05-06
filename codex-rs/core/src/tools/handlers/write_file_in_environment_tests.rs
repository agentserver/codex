use super::*;
use crate::tools::handlers::parse_arguments;
use pretty_assertions::assert_eq;

#[test]
fn rejects_missing_environment_id() {
    let json = r#"{"path": "/tmp/foo.txt", "content": "hi"}"#;
    let err = parse_arguments::<WriteFileInEnvironmentArgs>(json)
        .expect_err("missing environment_id should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("environment_id"),
        "expected error to mention environment_id, got: {msg}"
    );
}

#[test]
fn rejects_missing_path() {
    let json = r#"{"environment_id": "exe_two", "content": "hi"}"#;
    let err = parse_arguments::<WriteFileInEnvironmentArgs>(json)
        .expect_err("missing path should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("path"),
        "expected error to mention path, got: {msg}"
    );
}

#[test]
fn rejects_missing_content() {
    let json = r#"{"environment_id": "exe_two", "path": "/tmp/x"}"#;
    let err = parse_arguments::<WriteFileInEnvironmentArgs>(json)
        .expect_err("missing content should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("content"),
        "expected error to mention content, got: {msg}"
    );
}

#[test]
fn parses_required_fields_with_default_create_dirs_false() {
    let json = r#"{"environment_id": "exe_two", "path": "/srv/data/log.txt", "content": "hello"}"#;
    let args: WriteFileInEnvironmentArgs =
        parse_arguments(json).expect("happy-path parse should succeed");
    assert_eq!(args.environment_id, "exe_two");
    assert_eq!(args.path, "/srv/data/log.txt");
    assert_eq!(args.content, "hello");
    assert!(!args.create_dirs);

    let json = r#"{"environment_id": "exe_two", "path": "/srv/data/log.txt", "content": "", "create_dirs": true}"#;
    let args: WriteFileInEnvironmentArgs =
        parse_arguments(json).expect("create_dirs=true parse should succeed");
    assert!(args.create_dirs);
    assert_eq!(args.content, "");
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
    assert!(msg.contains("exe_missing"), "msg should include the unknown id: {msg}");
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

    // Mirrors the routing pin established for the other env-aware tools.
    // The Pa.6 write handler resolves the env-id supplied by the LLM via
    // `turn.select_environment(...)` and then invokes that env's
    // `get_filesystem().write_file(...)`. Pin the routing half: env id
    // resolves to the right `Arc<Environment>` (verified via
    // `Arc::ptr_eq`), not a silent fallback to the primary env.
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
}

#[tokio::test]
async fn write_file_via_environment_filesystem_round_trip() {
    use codex_utils_absolute_path::AbsolutePathBuf;
    use std::fs;

    // End-to-end-ish test of the second half of the handler: build a
    // real `Environment::default_for_tests()` (which uses
    // `LocalFileSystem::unsandboxed()`), write content via the env's
    // `write_file`, and read it back to confirm the contract.
    let tmp = tempfile::tempdir().expect("tempdir");
    let file_path = tmp.path().join("hello.txt");
    let body = "round trip\n";

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();
    let abs_path = AbsolutePathBuf::from_absolute_path(&file_path).expect("abs path");

    fs_handle
        .write_file(&abs_path, body.as_bytes().to_vec(), /*sandbox*/ None)
        .await
        .expect("write_file should succeed");

    let on_disk = fs::read_to_string(&file_path).expect("read back");
    assert_eq!(on_disk, body);
}

#[tokio::test]
async fn write_file_with_create_dirs_creates_parent_path() {
    use codex_exec_server::CreateDirectoryOptions;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use std::fs;

    // Pin the create_dirs=true behavior: the handler calls
    // `create_directory(parent, recursive=true)` before `write_file`.
    // Build the call sequence on a real env's fs and confirm the file
    // ends up where requested.
    let tmp = tempfile::tempdir().expect("tempdir");
    let nested_dir = tmp.path().join("a").join("b").join("c");
    let file_path = nested_dir.join("nested.txt");
    let body = "deep";

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();
    let parent_abs = AbsolutePathBuf::from_absolute_path(&nested_dir).expect("abs parent");
    let abs_path = AbsolutePathBuf::from_absolute_path(&file_path).expect("abs file");

    fs_handle
        .create_directory(
            &parent_abs,
            CreateDirectoryOptions { recursive: true },
            /*sandbox*/ None,
        )
        .await
        .expect("create_directory should succeed");

    fs_handle
        .write_file(&abs_path, body.as_bytes().to_vec(), /*sandbox*/ None)
        .await
        .expect("write_file should succeed after create_directory");

    let on_disk = fs::read_to_string(&file_path).expect("read back");
    assert_eq!(on_disk, body);
}

#[tokio::test]
async fn write_file_without_create_dirs_errors_on_missing_parent() {
    use codex_utils_absolute_path::AbsolutePathBuf;

    // Pin the create_dirs=false default: writing into a non-existent
    // parent must fail at the `write_file` layer (with no implicit
    // mkdir). The handler wraps this into a `RespondToModel` error;
    // here we exercise the underlying fs op to confirm it errors.
    let tmp = tempfile::tempdir().expect("tempdir");
    let nonexistent_dir = tmp.path().join("nope").join("nope");
    let file_path = nonexistent_dir.join("orphan.txt");

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();
    let abs_path = AbsolutePathBuf::from_absolute_path(&file_path).expect("abs file");

    let err = fs_handle
        .write_file(&abs_path, b"x".to_vec(), /*sandbox*/ None)
        .await
        .expect_err("write into missing parent must fail");

    let wrapped = format!(
        "failed to write file `{}` on environment `{}`: {err}",
        file_path.display(),
        "exe_one",
    );
    assert!(wrapped.contains("failed to write file"));
    assert!(wrapped.contains("exe_one"));
}
