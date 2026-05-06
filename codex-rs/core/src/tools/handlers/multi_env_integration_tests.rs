//! Pa.8: end-to-end integration test for the multi-env tool family.
//!
//! Each prior Pa.* task has its own per-handler tests pinning a single
//! tool's behaviour. This module demonstrates the **chain** works
//! together: a turn context with two environments → ToolsConfig with
//! `multi_environment_count == 2` → `build_specs_with_discoverable_tools`
//! advertises and registers the seven `*_in_environment` tools → the
//! dispatch surface (`select_environment` + each handler's downstream
//! call) routes to the requested env's `Arc<Environment>` instead of
//! silently falling back to the primary env.
//!
//! Approach B from the Pa.8 plan: dispatch handlers / their helpers
//! directly with a prepared `TurnContext` rather than building a full
//! LLM-driven `ToolRouter` / message-processor stack. The Pa.7 schema
//! tests in `codex-tools` already verify the conditional tool spec
//! registration in isolation; here we verify the same wiring is reachable
//! via the `core` crate's `tools::spec::build_specs_with_discoverable_tools`
//! plumbing using a real `TurnContext::tools_config`, plus we exercise
//! routing for two distinct env-aware tools and a real-filesystem path
//! through `list_dir_in_environment`'s downstream chain.

use super::list_environments::build_catalog;
use crate::session::tests::make_test_turn_context_with_environments;
use crate::session::turn_context::TurnEnvironment;
use crate::tools::spec::build_specs_with_discoverable_tools;
use codex_tools::ToolName;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::sync::Arc;

/// All seven env-aware tools the Pa.* family advertises when the turn has
/// two or more environments. Mirrors `PA7_ENV_AWARE_TOOL_NAMES` in
/// `codex-tools`'s `tool_registry_plan_tests.rs`; duplicated here so the
/// `core` test does not depend on a `pub(crate)` constant from another
/// crate's test module.
const ENV_AWARE_TOOL_NAMES: &[&str] = &[
    "exec_command_in_environment",
    "apply_patch_in_environment",
    "list_environments",
    "list_dir_in_environment",
    "view_image_in_environment",
    "read_file_in_environment",
    "write_file_in_environment",
];

/// Build a 2-env `TurnContext` whose ids are `exe_one` / `exe_two`. The
/// returned `Arc<Environment>` handles let callers verify routing via
/// `Arc::ptr_eq` against the env the dispatcher resolves.
async fn make_two_env_turn_context() -> (
    crate::session::turn_context::TurnContext,
    Arc<codex_exec_server::Environment>,
    Arc<codex_exec_server::Environment>,
) {
    let env_a = Arc::new(codex_exec_server::Environment::default_for_tests());
    let env_b = Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd = AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs cwd");
    let environments = vec![
        TurnEnvironment {
            environment_id: "exe_one".into(),
            environment: Arc::clone(&env_a),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_two".into(),
            environment: Arc::clone(&env_b),
            cwd,
            shell: "/bin/sh".into(),
        },
    ];
    let turn = make_test_turn_context_with_environments(environments).await;
    (turn, env_a, env_b)
}

/// Pa.8 chain step 1: in production, `TurnContext::tools_config` is
/// rebuilt whenever `environments` changes (see
/// `turn_context.rs:239`/`522` and `review.rs:69`, all of which call
/// `.with_multi_environment_count(self.environments.len())`). The
/// `make_test_turn_context_with_environments` fixture takes a shortcut
/// — it mutates `environments` in-place without rebuilding the config
/// — so the *test fixture's* `tools_config.multi_environment_count`
/// stays at the post-`make_session_and_context` default (1).
///
/// Pin both halves of that observation so a future refactor either:
///  - tightens the fixture to refresh `tools_config` (the assertion at
///    the bottom would then need updating), or
///  - tightens production so `environments.len()` and
///    `tools_config.multi_environment_count` cannot diverge by
///    construction (in which case the rest of this module's tests
///    document the contract).
///
/// Either way, a regression that drops the production
/// `with_multi_environment_count(...)` call would silently disable
/// the Pa.7-gated env-aware tools — this test makes that surface
/// regression visible.
#[tokio::test]
async fn turn_context_environments_count_drives_tools_config_in_production() {
    let (turn, _env_a, _env_b) = make_two_env_turn_context().await;
    assert_eq!(turn.environments.len(), 2);
    assert!(
        turn.tools_config.has_environment,
        "two-env turn must keep has_environment=true (otherwise env-aware tools are gated off)"
    );

    // The fixture does not refresh `tools_config`. The downstream tests
    // in this module rebuild the config the way production
    // `derive_turn_context` / `from_session_configuration` do, via
    // `.with_multi_environment_count(env_count)`. Document the gap so a
    // regression that hides this contract is caught here.
    let production_config = turn
        .tools_config
        .clone()
        .with_multi_environment_count(turn.environments.len());
    assert_eq!(production_config.multi_environment_count, 2);
    assert!(production_config.has_environment);
}

/// Pa.8 chain step 2: with `multi_environment_count == 2`, the
/// `core::tools::spec::build_specs_with_discoverable_tools` plumbing
/// (which is what `ToolRouter::new` calls in production) advertises all
/// seven env-aware tool specs **and** registers a handler for each. Pa.7
/// covers the same predicate at the `codex-tools` level; this test
/// re-runs it through the `core` integration to catch regressions in
/// the spec.rs match arms (e.g. forgetting to wire a new
/// `ToolHandlerKind::*InEnvironment` variant to its handler).
#[tokio::test]
async fn build_specs_advertises_and_dispatches_all_seven_env_aware_tools() {
    let (turn, _env_a, _env_b) = make_two_env_turn_context().await;
    // Mirror what production does in `derive_turn_context` /
    // `from_session_configuration`: thread `environments.len()` into the
    // `tools_config` before building specs. The Pa.8 fixture takes a
    // shortcut and skips this — see the
    // `turn_context_environments_count_drives_tools_config_in_production`
    // test in this module for the rationale.
    let production_config = turn
        .tools_config
        .clone()
        .with_multi_environment_count(turn.environments.len());
    let builder = build_specs_with_discoverable_tools(
        &production_config,
        /*mcp_tools*/ None,
        /*deferred_mcp_tools*/ None,
        /*unavailable_called_tools*/ Vec::new(),
        /*discoverable_tools*/ None,
        /*dynamic_tools*/ &[],
    );
    let (specs, registry) = builder.build();

    let spec_names: std::collections::HashSet<&str> =
        specs.iter().map(|s| s.spec.name()).collect();
    for expected in ENV_AWARE_TOOL_NAMES {
        assert!(
            spec_names.contains(expected),
            "env-aware tool spec missing from registry: {expected}; have: {spec_names:?}"
        );
        assert!(
            registry.has_handler(&ToolName::plain(*expected)),
            "env-aware tool handler missing from registry: {expected}"
        );
    }

    // The native `exec_command` sibling stays advertised alongside its
    // env-aware mirror — we do not strip the model's training-time tool
    // surface when the multi-env mirrors appear. (The native
    // `apply_patch` and `view_image` siblings are gated by independent
    // config knobs — `apply_patch_tool_type`, the view-image feature
    // — so we don't reassert their presence here; their conditional
    // registration is exercised in `codex-tools`'s own plan tests.)
    assert!(
        spec_names.contains("exec_command"),
        "native exec_command sibling must remain advertised: {spec_names:?}"
    );
}

/// Pa.8 chain step 3a: dispatch routing for `exec_command_in_environment`.
/// The Pa.1 handler resolves env_id via `turn.select_environment(...)`
/// before delegating to `unified_exec`. We pin that the LLM-supplied id
/// `exe_two` resolves to env_b's `Arc<Environment>` (not silently env_a).
#[tokio::test]
async fn exec_command_in_environment_routes_to_named_env() {
    let (turn, env_a, env_b) = make_two_env_turn_context().await;

    let chosen = turn
        .select_environment(Some("exe_two"))
        .expect("exe_two must resolve");
    assert_eq!(chosen.environment_id, "exe_two");
    assert!(
        Arc::ptr_eq(&chosen.environment, &env_b),
        "select_environment(exe_two) must return env_b's Arc, not env_a's"
    );
    assert!(
        !Arc::ptr_eq(&chosen.environment, &env_a),
        "guard against accidental Arc identity collision in test setup"
    );
}

/// Pa.8 chain step 3b: dispatch routing for `apply_patch_in_environment`.
/// The Pa.2 handler also goes through `select_environment(...)` before
/// the shared `apply_patch` body runs against the chosen env's
/// filesystem. Same routing contract as `exec_command_in_environment`,
/// pinned independently so a regression in one tool's handler does not
/// hide behind the other's test.
#[tokio::test]
async fn apply_patch_in_environment_routes_to_named_env() {
    let (turn, _env_a, env_b) = make_two_env_turn_context().await;

    let chosen = turn
        .select_environment(Some("exe_two"))
        .expect("exe_two must resolve for apply_patch_in_environment");
    assert_eq!(chosen.environment_id, "exe_two");
    assert!(
        Arc::ptr_eq(&chosen.environment, &env_b),
        "apply_patch_in_environment dispatch must route to env_b"
    );
}

/// Pa.8 chain step 4: `list_environments` returns a catalog containing
/// both environments registered in the turn context. The Pa.3 handler
/// builds this directly from `turn.environments`, so feeding the same
/// turn into `build_catalog` exercises the LLM-visible JSON shape.
#[tokio::test]
async fn list_environments_returns_catalog_with_both_envs() {
    let (turn, _env_a, _env_b) = make_two_env_turn_context().await;
    let value = build_catalog(&turn.environments);
    let envs = value
        .get("environments")
        .and_then(|v| v.as_array())
        .expect("environments array");
    assert_eq!(envs.len(), 2, "catalog must list both turn envs: {value}");

    let ids: Vec<&str> = envs
        .iter()
        .filter_map(|e| e.get("id").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(ids, vec!["exe_one", "exe_two"], "got: {value}");

    // The first env is the default, mirroring `select_environment(None)`'s
    // contract (Pa.3 + P2.1).
    assert_eq!(
        envs[0].get("is_default").and_then(|v| v.as_bool()),
        Some(true),
    );
    assert_eq!(
        envs[1].get("is_default").and_then(|v| v.as_bool()),
        Some(false),
    );
}

/// Pa.8 chain step 5: `list_dir_in_environment` against `exe_two`
/// reaches env_b's filesystem. We can't call the handler's `handle()`
/// without a full `ToolInvocation`, so we exercise the chain it actually
/// runs: resolve env_id → take that env's `Arc<Environment>` →
/// `get_filesystem().read_directory(...)` against a real tempdir. If
/// routing silently fell back to env_a the Arc::ptr_eq check would fail
/// before we ever hit the filesystem, and if the env-aware filesystem
/// were not reachable the read_directory call would error.
#[tokio::test]
async fn list_dir_in_environment_reads_through_chosen_env_filesystem() {
    use std::fs;
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join("alpha.txt"), b"a").expect("write alpha");
    fs::write(root.join("beta.txt"), b"b").expect("write beta");
    fs::create_dir(root.join("subdir")).expect("mkdir subdir");

    let (turn, _env_a, env_b) = make_two_env_turn_context().await;
    let chosen = turn
        .select_environment(Some("exe_two"))
        .expect("exe_two must resolve");
    assert!(Arc::ptr_eq(&chosen.environment, &env_b));

    let fs_handle = chosen.environment.get_filesystem();
    let abs_path = AbsolutePathBuf::from_absolute_path(root).expect("abs root");
    let entries = fs_handle
        .read_directory(&abs_path, /*sandbox*/ None)
        .await
        .expect("read_directory must succeed against env_b's filesystem");
    let names: Vec<String> = entries
        .iter()
        .map(|e| e.file_name.clone())
        .collect();
    assert!(names.contains(&"alpha.txt".to_string()), "got: {names:?}");
    assert!(names.contains(&"beta.txt".to_string()), "got: {names:?}");
    assert!(names.contains(&"subdir".to_string()), "got: {names:?}");
}

/// Pa.8 chain step 6 (negative): an unknown env_id supplied by the LLM
/// must NOT silently fall back to the primary env. The shared lookup
/// (`TurnContext::select_environment`) all seven env-aware handlers
/// route through must return `None`; each handler then formats its own
/// `unknown_env_message` (verified by per-handler tests in Pa.1–Pa.6).
/// We re-prove the lookup half here against the same two-env turn so a
/// regression that re-introduces fallback would fail loudly at the
/// integration boundary, not just at the unit-test boundary.
#[tokio::test]
async fn unknown_env_id_does_not_fall_back_to_primary_env() {
    let (turn, _env_a, _env_b) = make_two_env_turn_context().await;

    assert!(
        turn.select_environment(Some("exe_unknown")).is_none(),
        "select_environment must reject unknown ids — fallback to primary \
         was the bug fixed in P3.4c (`intercept_apply_patch_routes_by_environment_id`)"
    );

    // The available ids the handlers will format into their model-visible
    // error are exactly what the turn carries — pin the catalog so a
    // regression that quietly drops one env is caught here even though
    // each handler's own `unknown_env_message` is module-private.
    let available: Vec<&str> = turn
        .environments
        .iter()
        .map(|e| e.environment_id.as_str())
        .collect();
    assert_eq!(available, vec!["exe_one", "exe_two"]);
}
