use super::*;
use crate::tools::handlers::parse_arguments;
use pretty_assertions::assert_eq;

#[test]
fn rejects_missing_environment_id() {
    let json = r#"{"input": "*** Begin Patch\n*** End Patch\n"}"#;
    let err = parse_arguments::<ApplyPatchInEnvironmentArgs>(json)
        .expect_err("missing environment_id should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("environment_id"),
        "expected error to mention environment_id, got: {msg}"
    );
}

#[test]
fn rejects_missing_input() {
    let json = r#"{"environment_id": "exe_two"}"#;
    let err = parse_arguments::<ApplyPatchInEnvironmentArgs>(json)
        .expect_err("missing input should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("input"),
        "expected error to mention input, got: {msg}"
    );
}

#[test]
fn parses_required_fields() {
    let json = r#"{"environment_id": "exe_two", "input": "*** Begin Patch\n*** End Patch\n"}"#;
    let args: ApplyPatchInEnvironmentArgs =
        parse_arguments(json).expect("happy-path parse should succeed");
    assert_eq!(args.environment_id, "exe_two");
    assert!(args.input.contains("Begin Patch"));
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

    // Mirrors the Pa.1 contract for `exec_command_in_environment` and the
    // P3.4c regression test for `intercept_apply_patch`. Pins the routing
    // contract end-to-end at the handler layer for the new env-aware
    // `apply_patch_in_environment` tool: the env id supplied by the LLM
    // resolves to the chosen env's `Arc<Environment>` (verified via
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

    // And the handler-layer error message matches what the new tool
    // returns when the LLM-supplied id is unknown.
    let msg = unknown_env_message("exe_missing", &turn_context.environments);
    assert!(msg.contains("exe_one"));
    assert!(msg.contains("exe_two"));
}
