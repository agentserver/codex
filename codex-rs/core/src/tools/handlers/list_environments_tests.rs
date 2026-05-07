use super::*;
use crate::tools::handlers::parse_arguments;
use pretty_assertions::assert_eq;

#[test]
fn args_default_when_missing() {
    // Empty object -> include_status defaults to false.
    let args: ListEnvironmentsArgs = parse_arguments("{}").expect("parse empty object");
    assert!(!args.include_status);
}

#[test]
fn args_parses_include_status_true() {
    let args: ListEnvironmentsArgs =
        parse_arguments(r#"{"include_status": true}"#).expect("parse include_status");
    assert!(args.include_status);
}

#[test]
fn args_parses_include_status_false_explicit() {
    let args: ListEnvironmentsArgs =
        parse_arguments(r#"{"include_status": false}"#).expect("parse include_status=false");
    assert!(!args.include_status);
}

#[tokio::test]
async fn build_catalog_empty_envs_yields_empty_array() {
    let value = build_catalog(&[]);
    let envs = value
        .get("environments")
        .and_then(|v| v.as_array())
        .expect("environments array");
    assert!(envs.is_empty(), "got: {value}");
}

#[tokio::test]
async fn build_catalog_marks_first_env_as_default_and_carries_description() {
    use crate::session::turn_context::TurnEnvironment;

    let env_a = std::sync::Arc::new(
        codex_exec_server::Environment::default_for_tests()
            .with_description("Alpha host".to_string()),
    );
    // env_b intentionally has no description -> the JSON entry should omit
    // the `description` key entirely.
    let env_b = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());

    let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs");
    let environments = vec![
        TurnEnvironment {
            environment_id: "exe_alpha".into(),
            environment: std::sync::Arc::clone(&env_a),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_beta".into(),
            environment: std::sync::Arc::clone(&env_b),
            cwd,
            shell: "/bin/sh".into(),
        },
    ];

    let value = build_catalog(&environments);
    let envs = value
        .get("environments")
        .and_then(|v| v.as_array())
        .expect("environments array");
    assert_eq!(envs.len(), 2, "got: {value}");

    let first = envs[0].as_object().expect("first entry object");
    assert_eq!(first.get("id").and_then(|v| v.as_str()), Some("exe_alpha"));
    assert_eq!(first.get("is_default").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        first.get("description").and_then(|v| v.as_str()),
        Some("Alpha host"),
        "description should be threaded from Environment::description()"
    );
    // Pa.3: `online` field intentionally absent regardless of include_status.
    assert!(!first.contains_key("online"));

    let second = envs[1].as_object().expect("second entry object");
    assert_eq!(second.get("id").and_then(|v| v.as_str()), Some("exe_beta"));
    assert_eq!(
        second.get("is_default").and_then(|v| v.as_bool()),
        Some(false)
    );
    // No description set -> field omitted.
    assert!(
        !second.contains_key("description"),
        "missing description should be omitted, not null: {second:?}"
    );
    assert!(!second.contains_key("online"));
}

#[tokio::test]
async fn build_catalog_single_env_is_default() {
    use crate::session::turn_context::TurnEnvironment;

    let env = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs");
    let environments = vec![TurnEnvironment {
        environment_id: "solo".into(),
        environment: env,
        cwd,
        shell: "/bin/sh".into(),
    }];

    let value = build_catalog(&environments);
    let envs = value
        .get("environments")
        .and_then(|v| v.as_array())
        .expect("environments array");
    assert_eq!(envs.len(), 1);
    assert_eq!(
        envs[0].get("is_default").and_then(|v| v.as_bool()),
        Some(true)
    );
}
