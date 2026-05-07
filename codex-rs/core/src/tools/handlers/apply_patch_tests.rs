use super::*;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_exec_server::LOCAL_FS;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::protocol::FileChange;
use codex_protocol::protocol::SandboxPolicy;
use core_test_support::PathBufExt;
use core_test_support::PathExt;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use crate::session::tests::make_session_and_context;
use crate::tools::context::ToolInvocation;
use crate::tools::hook_names::HookToolName;
use crate::tools::registry::PostToolUsePayload;
use crate::tools::registry::PreToolUsePayload;
use crate::turn_diff_tracker::TurnDiffTracker;

fn sample_patch() -> &'static str {
    r#"*** Begin Patch
*** Add File: hello.txt
+hello
*** End Patch"#
}

async fn invocation_for_payload(payload: ToolPayload) -> ToolInvocation {
    let (session, turn) = make_session_and_context().await;
    ToolInvocation {
        session: session.into(),
        turn: turn.into(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        tracker: Arc::new(Mutex::new(TurnDiffTracker::new())),
        call_id: "call-apply-patch".to_string(),
        tool_name: codex_tools::ToolName::plain("apply_patch"),
        source: crate::tools::context::ToolCallSource::Direct,
        payload,
    }
}

#[tokio::test]
async fn pre_tool_use_payload_uses_json_patch_input() {
    let patch = sample_patch();
    let payload = ToolPayload::Function {
        arguments: json!({ "input": patch }).to_string(),
    };
    let invocation = invocation_for_payload(payload).await;
    let handler = ApplyPatchHandler;

    assert_eq!(
        handler.pre_tool_use_payload(&invocation),
        Some(PreToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_input: json!({ "command": patch }),
        })
    );
}

#[tokio::test]
async fn pre_tool_use_payload_uses_freeform_patch_input() {
    let patch = sample_patch();
    let payload = ToolPayload::Custom {
        input: patch.to_string(),
    };
    let invocation = invocation_for_payload(payload).await;
    let handler = ApplyPatchHandler;

    assert_eq!(
        handler.pre_tool_use_payload(&invocation),
        Some(PreToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_input: json!({ "command": patch }),
        })
    );
}

#[tokio::test]
async fn post_tool_use_payload_uses_patch_input_and_tool_output() {
    let patch = sample_patch();
    let payload = ToolPayload::Custom {
        input: patch.to_string(),
    };
    let invocation = invocation_for_payload(payload).await;
    let output = ApplyPatchToolOutput::from_text("Success. Updated files.".to_string());
    let handler = ApplyPatchHandler;

    assert_eq!(
        handler.post_tool_use_payload(&invocation, &output),
        Some(PostToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_use_id: "call-apply-patch".to_string(),
            tool_input: json!({ "command": patch }),
            tool_response: json!("Success. Updated files."),
        })
    );
}

#[test]
fn diff_consumer_does_not_stream_json_tool_call_arguments() {
    let mut consumer = ApplyPatchArgumentDiffConsumer::default();
    assert!(
        consumer
            .push_delta("call-1".to_string(), r#"{"input":"*** Begin Patch\n"#)
            .is_none()
    );
    assert!(
        consumer
            .push_delta(
                "call-1".to_string(),
                r#"*** Add File: hello.txt\n+hello\n*** End Patch\n"}"#
            )
            .is_none()
    );
}

#[test]
fn diff_consumer_streams_apply_patch_changes() {
    let mut consumer = ApplyPatchArgumentDiffConsumer::default();
    assert!(
        consumer
            .push_delta("call-1".to_string(), "*** Begin Patch\n")
            .is_none()
    );

    let event = consumer
        .push_delta("call-1".to_string(), "*** Add File: hello.txt\n+hello")
        .expect("progress event");
    assert_eq!(
        (event.call_id, event.changes),
        (
            "call-1".to_string(),
            HashMap::from([(
                PathBuf::from("hello.txt"),
                FileChange::Add {
                    content: String::new(),
                },
            )]),
        )
    );

    assert!(
        consumer
            .push_delta("call-1".to_string(), "\n+world")
            .is_none()
    );
    assert!(
        consumer
            .push_delta("call-1".to_string(), "\n*** End Patch")
            .is_none()
    );

    let event = consumer
        .finish_update_on_complete()
        .expect("finish parser")
        .expect("progress event");
    assert_eq!(
        (event.call_id, event.changes),
        (
            "call-1".to_string(),
            HashMap::from([(
                PathBuf::from("hello.txt"),
                FileChange::Add {
                    content: "hello\nworld\n".to_string(),
                },
            )]),
        )
    );
}

#[test]
fn diff_consumer_sends_next_update_after_buffer_interval() {
    let mut consumer = ApplyPatchArgumentDiffConsumer::default();
    consumer.push_delta("call-1".to_string(), "*** Begin Patch\n");
    let first = consumer
        .push_delta("call-1".to_string(), "*** Add File: hello.txt\n+hello")
        .expect("first progress event");
    assert_eq!(
        first.changes,
        HashMap::from([(
            PathBuf::from("hello.txt"),
            FileChange::Add {
                content: String::new(),
            },
        )])
    );

    consumer.last_sent_at =
        Some(std::time::Instant::now() - APPLY_PATCH_ARGUMENT_DIFF_BUFFER_INTERVAL);
    let second = consumer
        .push_delta("call-1".to_string(), "\n+world")
        .expect("second progress event");
    assert_eq!(
        second.changes,
        HashMap::from([(
            PathBuf::from("hello.txt"),
            FileChange::Add {
                content: "hello\n".to_string(),
            },
        )])
    );
}

#[tokio::test]
async fn approval_keys_include_move_destination() {
    let tmp = TempDir::new().expect("tmp");
    let cwd_path = tmp.path();
    let cwd = cwd_path.abs();
    std::fs::create_dir_all(cwd_path.join("old")).expect("create old dir");
    std::fs::create_dir_all(cwd_path.join("renamed/dir")).expect("create dest dir");
    std::fs::write(cwd_path.join("old/name.txt"), "old content\n").expect("write old file");
    let patch = r#"*** Begin Patch
*** Update File: old/name.txt
*** Move to: renamed/dir/name.txt
@@
-old content
+new content
*** End Patch"#;
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    let action = match codex_apply_patch::maybe_parse_apply_patch_verified(
        &argv,
        &cwd,
        LOCAL_FS.as_ref(),
        /*sandbox*/ None,
    )
    .await
    {
        MaybeApplyPatchVerified::Body(action) => action,
        other => panic!("expected patch body, got: {other:?}"),
    };

    let keys = file_paths_for_action(&action);
    assert_eq!(keys.len(), 2);
}

#[test]
fn write_permissions_for_paths_skip_dirs_already_writable_under_workspace_root() {
    let tmp = TempDir::new().expect("tmp");
    let cwd_path = tmp.path();
    let cwd = cwd_path.abs();
    let nested = cwd_path.join("nested");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    let file_path = AbsolutePathBuf::try_from(nested.join("file.txt"))
        .expect("nested file path should be absolute");
    let sandbox_policy = FileSystemSandboxPolicy::from(&SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: false,
    });

    let permissions = write_permissions_for_paths(&[file_path], &sandbox_policy, &cwd);

    assert_eq!(permissions, None);
}

#[test]
fn write_permissions_for_paths_keep_dirs_outside_workspace_root() {
    let tmp = TempDir::new().expect("tmp");
    let cwd = tmp.path().join("workspace");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&cwd).expect("create cwd");
    std::fs::create_dir_all(&outside).expect("create outside dir");
    let file_path = AbsolutePathBuf::try_from(outside.join("file.txt"))
        .expect("outside file path should be absolute");
    let cwd_abs = cwd.abs();
    let sandbox_policy = FileSystemSandboxPolicy::from(&SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    });

    let permissions = write_permissions_for_paths(&[file_path], &sandbox_policy, &cwd_abs);
    let expected_outside =
        dunce::simplified(&outside.canonicalize().expect("canonicalize outside dir")).abs();

    assert_eq!(
        permissions
            .and_then(|profile| profile.file_system)
            .and_then(|fs| fs.legacy_read_write_roots())
            .and_then(|(_read, write)| write),
        Some(vec![expected_outside])
    );
}

#[test]
fn apply_patch_args_carry_environment_id() {
    let args_json = r#"{"input":"*** Begin Patch\n*** End Patch\n","environment_id":"exe_beta"}"#;
    let args: codex_tools::ApplyPatchToolArgs = serde_json::from_str(args_json).expect("parse");
    assert_eq!(args.environment_id.as_deref(), Some("exe_beta"));
    assert!(args.input.contains("Begin Patch"));
}

#[test]
fn apply_patch_args_environment_id_default_is_none() {
    let args_json = r#"{"input":"*** Begin Patch\n*** End Patch\n"}"#;
    let args: codex_tools::ApplyPatchToolArgs = serde_json::from_str(args_json).expect("parse");
    assert!(args.environment_id.is_none());
}

/// P3.4c regression test: prior to this fix, `intercept_apply_patch` ignored
/// the caller-supplied `environment_id` (hardcoded `primary_environment()` and
/// `ApplyPatchRequest { environment_id: None }`). The shell handler resolves
/// env_id from `params.environment_id` (P3.4b) and forwards it here; if the
/// helper drops it, an `apply_patch` invocation routed through `shell` against
/// `exe_two` would silently target the primary environment instead.
///
/// We prove env_id is consumed by passing an unknown id alongside a valid
/// apply_patch command on a turn that *does* have environments. If env_id
/// were ignored, `intercept_apply_patch` would fall back to the primary env
/// and proceed to parse/execute the patch (or fail for an unrelated reason).
/// With the fix in place, the helper rejects the request with the same
/// descriptive error pattern used by the shell handler.
#[tokio::test]
async fn intercept_apply_patch_routes_by_environment_id() {
    use crate::session::tests::make_session_and_context;
    use crate::session::tests::make_test_turn_context_with_environments;
    use crate::session::turn_context::TurnEnvironment;
    use codex_exec_server::LOCAL_FS;

    let env_a = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let env_b = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd_path = std::env::current_dir().expect("cwd");
    let cwd = cwd_path.as_path().abs();
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
    let turn = make_test_turn_context_with_environments(environments).await;
    let (session, _) = make_session_and_context().await;

    let patch = sample_patch();
    let command = vec!["apply_patch".to_string(), patch.to_string()];

    // Negative: unknown env_id surfaces a descriptive error mentioning the
    // requested id and the available ids — proving env_id reaches
    // `select_environment` instead of being silently swapped for primary.
    let result = intercept_apply_patch(
        &command,
        &cwd,
        LOCAL_FS.as_ref(),
        std::sync::Arc::new(session),
        std::sync::Arc::new(turn),
        None,
        "call-intercept",
        "shell",
        Some("exe_missing"),
    )
    .await;
    let err = match result {
        Ok(_) => panic!("unknown env_id must error, not return Ok"),
        Err(err) => err,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("exe_missing"),
        "error should name the requested env id: {msg}"
    );
    assert!(
        msg.contains("exe_one") && msg.contains("exe_two"),
        "error should list available env ids: {msg}"
    );

    // Positive: with the same multi-env turn, `select_environment("exe_two")`
    // resolves to env_b (this mirrors the resolution intercept_apply_patch now
    // performs). This pins the routing contract: the env id flowed in by
    // shell.rs reaches the env lookup that previously hardcoded primary.
    let (_session2, _) = make_session_and_context().await;
    let env_a2 = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let env_b2 = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let environments2 = vec![
        TurnEnvironment {
            environment_id: "exe_one".into(),
            environment: std::sync::Arc::clone(&env_a2),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_two".into(),
            environment: std::sync::Arc::clone(&env_b2),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
    ];
    let turn2 = make_test_turn_context_with_environments(environments2).await;
    let chosen = turn2.select_environment(Some("exe_two")).expect("found");
    assert_eq!(chosen.environment_id, "exe_two");
    assert!(std::sync::Arc::ptr_eq(&chosen.environment, &env_b2));
}
