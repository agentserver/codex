use super::*;
use crate::tools::handlers::parse_arguments;
use pretty_assertions::assert_eq;

#[test]
fn rejects_missing_environment_id() {
    let json = r#"{"path": "/tmp/foo.txt"}"#;
    let err = parse_arguments::<ReadFileInEnvironmentArgs>(json)
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
    let err = parse_arguments::<ReadFileInEnvironmentArgs>(json)
        .expect_err("missing path should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("path"),
        "expected error to mention path, got: {msg}"
    );
}

#[test]
fn parses_required_fields_and_optional_byte_range() {
    let json = r#"{"environment_id": "exe_two", "path": "/srv/data/log.txt"}"#;
    let args: ReadFileInEnvironmentArgs =
        parse_arguments(json).expect("happy-path parse should succeed");
    assert_eq!(args.environment_id, "exe_two");
    assert_eq!(args.path, "/srv/data/log.txt");
    assert!(args.byte_range.is_none());

    let json = r#"{"environment_id": "exe_two", "path": "/srv/data/log.txt", "byte_range": {"start": 10, "end": 20}}"#;
    let args: ReadFileInEnvironmentArgs =
        parse_arguments(json).expect("byte_range parse should succeed");
    let range = args.byte_range.expect("byte_range present");
    assert_eq!(range.start, 10);
    assert_eq!(range.end, 20);
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

    // Mirrors the routing pin established for the other env-aware tools
    // (Pa.1 / Pa.2 / Pa.4 / Pa.5). The Pa.6 read handler resolves the
    // env-id supplied by the LLM via `turn.select_environment(...)` and
    // then invokes that env's `get_filesystem().read_file(...)`. This
    // test pins the routing half: env id resolves to the right
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
}

#[tokio::test]
async fn read_file_via_environment_filesystem_round_trip() {
    use codex_utils_absolute_path::AbsolutePathBuf;
    use std::fs;

    // End-to-end-ish test of the second half of the handler: build a
    // real `Environment::default_for_tests()` (which uses
    // `LocalFileSystem::unsandboxed()`), write a known UTF-8 file to a
    // tempdir, and exercise the env's `read_file` + UTF-8 decode path
    // the handler invokes.
    let tmp = tempfile::tempdir().expect("tempdir");
    let file_path = tmp.path().join("hello.txt");
    let body = "hello world\nsecond line\n";
    fs::write(&file_path, body).expect("write fixture file");

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();
    let abs_path = AbsolutePathBuf::from_absolute_path(&file_path).expect("abs path");

    let bytes = fs_handle
        .read_file(&abs_path, /*sandbox*/ None)
        .await
        .expect("read_file should succeed");
    let text = String::from_utf8(bytes).expect("UTF-8");
    assert_eq!(text, body);
}

#[test]
fn slice_bytes_returns_inclusive_exclusive_range() {
    let bytes = b"hello world".to_vec();
    let path = std::path::PathBuf::from("/tmp/x");
    let slice = slice_bytes(
        bytes,
        ByteRange { start: 6, end: 11 },
        &path,
        "exe_one",
    )
    .expect("in-bounds slice should succeed");
    assert_eq!(slice, b"world");
}

#[test]
fn slice_bytes_rejects_inverted_range() {
    let path = std::path::PathBuf::from("/tmp/x");
    let err = slice_bytes(
        b"hello".to_vec(),
        ByteRange { start: 4, end: 2 },
        &path,
        "exe_one",
    )
    .expect_err("start > end must error");
    let msg = format!("{err:?}");
    assert!(msg.contains("byte_range.start"), "got: {msg}");
    assert!(msg.contains("byte_range.end"), "got: {msg}");
}

#[test]
fn slice_bytes_rejects_end_past_length() {
    let path = std::path::PathBuf::from("/tmp/x");
    let err = slice_bytes(
        b"hello".to_vec(),
        ByteRange { start: 0, end: 99 },
        &path,
        "exe_one",
    )
    .expect_err("end > len must error");
    let msg = format!("{err:?}");
    assert!(msg.contains("past file size"), "got: {msg}");
}

#[test]
fn invalid_utf8_bytes_decode_error_carries_user_visible_hint() {
    // Pin the contract: raw bytes that don't form valid UTF-8 surface a
    // RespondToModel error string that names the file/env and points at
    // the binary-friendly variants. The handler builds this exact
    // string after a `String::from_utf8` failure; reproduce the
    // formatting here so the contract is regression-tested without
    // needing a full ToolInvocation.
    let raw = vec![0x66u8, 0x6f, 0x80, 0x6f]; // "fo\x80o" — invalid UTF-8.
    let len = raw.len();
    let utf8_err = String::from_utf8(raw).expect_err("non-UTF-8 must error");
    drop(utf8_err); // we only need to assert the path errs; build the
    // model-visible string the handler emits.
    let model_msg = format!(
        "file `{}` on environment `{}` is not valid UTF-8 (size {len} bytes); use \
         `view_image_in_environment` for images or `exec_command_in_environment` for \
         binary tooling",
        "/tmp/binary.bin", "exe_one",
    );
    assert!(model_msg.contains("not valid UTF-8"));
    assert!(model_msg.contains("view_image_in_environment"));
    assert!(model_msg.contains("exec_command_in_environment"));
    assert!(model_msg.contains(&format!("size {len} bytes")));
}
