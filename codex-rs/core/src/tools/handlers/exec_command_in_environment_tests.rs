use super::*;
use crate::tools::handlers::parse_arguments;
use pretty_assertions::assert_eq;

#[test]
fn rejects_missing_environment_id() {
    let json = r#"{"cmd": "echo hi"}"#;
    let err = parse_arguments::<ExecCommandInEnvironmentArgs>(json)
        .expect_err("missing environment_id should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("environment_id"),
        "expected error to mention environment_id, got: {msg}"
    );
}

#[test]
fn rejects_missing_cmd() {
    let json = r#"{"environment_id": "exe_two"}"#;
    let err = parse_arguments::<ExecCommandInEnvironmentArgs>(json)
        .expect_err("missing cmd should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("cmd"),
        "expected error to mention cmd, got: {msg}"
    );
}

#[test]
fn parses_minimum_required_fields() {
    let json = r#"{"environment_id": "exe_two", "cmd": "echo hi"}"#;
    let args: ExecCommandInEnvironmentArgs =
        parse_arguments(json).expect("happy-path parse should succeed");
    assert_eq!(args.environment_id, "exe_two");
    assert_eq!(args.cmd, "echo hi");
    assert_eq!(args.workdir, None);
    assert_eq!(args.shell, None);
    assert_eq!(args.login, None);
    assert!(!args.tty);
    assert_eq!(args.yield_time_ms, 10_000);
    assert_eq!(args.max_output_tokens, None);
}

#[test]
fn parses_full_field_set() {
    let json = r#"{
        "environment_id": "exe_beta",
        "cmd": "ls -la",
        "workdir": "/tmp",
        "shell": "/bin/bash",
        "login": true,
        "tty": true,
        "yield_time_ms": 500,
        "max_output_tokens": 1024,
        "justification": "needed",
        "prefix_rule": ["ls"]
    }"#;
    let args: ExecCommandInEnvironmentArgs =
        parse_arguments(json).expect("full parse should succeed");
    assert_eq!(args.environment_id, "exe_beta");
    assert_eq!(args.cmd, "ls -la");
    assert_eq!(args.workdir.as_deref(), Some("/tmp"));
    assert_eq!(args.shell.as_deref(), Some("/bin/bash"));
    assert_eq!(args.login, Some(true));
    assert!(args.tty);
    assert_eq!(args.yield_time_ms, 500);
    assert_eq!(args.max_output_tokens, Some(1024));
    assert_eq!(args.justification.as_deref(), Some("needed"));
    assert_eq!(
        args.prefix_rule.as_ref().map(|v| v.as_slice()),
        Some(["ls".to_string()].as_slice())
    );
}

#[test]
fn into_exec_command_args_round_trips_field_values() {
    let json = r#"{
        "environment_id": "exe_x",
        "cmd": "echo hi",
        "workdir": "/srv",
        "shell": "/bin/sh",
        "tty": true,
        "yield_time_ms": 250,
        "max_output_tokens": 99,
        "login": false
    }"#;
    let env_args: ExecCommandInEnvironmentArgs =
        parse_arguments(json).expect("parse");
    let (env_id, exec_args) = env_args.into_exec_command_args();
    assert_eq!(env_id, "exe_x");
    assert_eq!(exec_args.cmd, "echo hi");
    assert_eq!(exec_args.workdir.as_deref(), Some("/srv"));
    assert_eq!(exec_args.shell.as_deref(), Some("/bin/sh"));
    assert!(exec_args.tty);
    assert_eq!(exec_args.yield_time_ms, 250);
    assert_eq!(exec_args.max_output_tokens, Some(99));
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

    // Mirrors the runtime contract verified by P2.4
    // (`unified_exec_routes_to_second_environment_when_environment_id_set`)
    // and the P3.4b shell-handler analogue. The new
    // `exec_command_in_environment` handler uses the same
    // `select_environment` helper to look up the env-aware filesystem
    // before delegating to the shared exec body, so this pin establishes
    // the contract end-to-end at the handler layer.
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
