#![allow(dead_code)]
// All public items in this module are dead code until Pa.7 wires the new
// handler into the registry. The `#![allow(dead_code)]` above keeps the
// noise contained to this file rather than relying on per-item attributes
// that would have to be removed later.

//! `exec_command_in_environment` — env-aware mirror of the native
//! `exec_command` tool, added in spec § Pa.1.
//!
//! The native `exec_command` tool stays byte-identical to upstream codex
//! (Pa.0 reverted the schema modifications) so the model sees its
//! training-time signature. This handler exposes the env routing as a
//! parallel tool whose schema prepends a required `environment_id` field.
//!
//! All approval / sandbox / apply-patch interception logic is shared with
//! the legacy `exec_command` path via `handle_exec_command_request` in the
//! `unified_exec` handler module.

use crate::function_tool::FunctionCallError;
use crate::sandboxing::SandboxPermissions;
use crate::tools::context::ExecCommandToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::handlers::parse_arguments_with_base_path;
use crate::tools::handlers::resolve_workdir_base_path;
use crate::tools::handlers::unified_exec::ExecCommandArgs;
use crate::tools::handlers::unified_exec::handle_exec_command_request;
use crate::tools::hook_names::HookToolName;
use crate::tools::registry::PostToolUsePayload;
use crate::tools::registry::PreToolUsePayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::unified_exec::UnifiedExecContext;
use crate::unified_exec::UnifiedExecProcessManager;
use codex_protocol::models::AdditionalPermissionProfile;
use serde::Deserialize;

pub(crate) const TOOL_NAME: &str = "exec_command_in_environment";

pub struct ExecCommandInEnvironmentHandler;

/// Args for `exec_command_in_environment`. Mirrors `ExecCommandArgs` from
/// the legacy `exec_command` handler but prepends a **required**
/// `environment_id` field. Defaults match `ExecCommandArgs` so the
/// behaviour outside env routing is identical.
#[derive(Debug, Deserialize)]
pub(crate) struct ExecCommandInEnvironmentArgs {
    pub(crate) environment_id: String,
    pub(crate) cmd: String,
    #[serde(default)]
    pub(crate) workdir: Option<String>,
    #[serde(default)]
    pub(crate) shell: Option<String>,
    #[serde(default)]
    pub(crate) login: Option<bool>,
    #[serde(default = "default_tty")]
    pub(crate) tty: bool,
    #[serde(default = "default_exec_yield_time_ms")]
    pub(crate) yield_time_ms: u64,
    #[serde(default)]
    pub(crate) max_output_tokens: Option<usize>,
    #[serde(default)]
    pub(crate) sandbox_permissions: SandboxPermissions,
    #[serde(default)]
    pub(crate) additional_permissions: Option<AdditionalPermissionProfile>,
    #[serde(default)]
    pub(crate) justification: Option<String>,
    #[serde(default)]
    pub(crate) prefix_rule: Option<Vec<String>>,
}

fn default_exec_yield_time_ms() -> u64 {
    10_000
}

fn default_tty() -> bool {
    false
}

impl ExecCommandInEnvironmentArgs {
    /// Project the env-aware args onto the legacy `ExecCommandArgs` shape so
    /// the shared `handle_exec_command_request` body can consume them.
    pub(crate) fn into_exec_command_args(self) -> (String, ExecCommandArgs) {
        let env_id = self.environment_id;
        // Re-serialize to JSON, then parse as ExecCommandArgs. This avoids
        // duplicating the field list and stays robust if upstream adds
        // new fields to ExecCommandArgs (they'll flow through). The JSON
        // round-trip is cheap; this is on the per-tool-call hot path but
        // already alongside process spawning.
        let json = serde_json::json!({
            "cmd": self.cmd,
            "workdir": self.workdir,
            "shell": self.shell,
            "login": self.login,
            "tty": self.tty,
            "yield_time_ms": self.yield_time_ms,
            "max_output_tokens": self.max_output_tokens,
            "sandbox_permissions": self.sandbox_permissions,
            "additional_permissions": self.additional_permissions,
            "justification": self.justification,
            "prefix_rule": self.prefix_rule,
        });
        let args: ExecCommandArgs =
            serde_json::from_value(json).expect("ExecCommandArgs round-trip");
        (env_id, args)
    }
}

impl ToolHandler for ExecCommandInEnvironmentHandler {
    type Output = ExecCommandToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Conservative: command in another env may have side effects.
        // The shared body re-derives the safe-command exemption when
        // appropriate; mirroring the legacy handler's conservative answer
        // here keeps the registry-level guarding consistent.
        true
    }

    fn pre_tool_use_payload(&self, invocation: &ToolInvocation) -> Option<PreToolUsePayload> {
        if invocation.tool_name.namespace.is_some()
            || invocation.tool_name.name.as_str() != TOOL_NAME
        {
            return None;
        }

        let ToolPayload::Function { arguments } = &invocation.payload else {
            return None;
        };

        parse_arguments::<ExecCommandInEnvironmentArgs>(arguments)
            .ok()
            .map(|args| PreToolUsePayload {
                tool_name: HookToolName::bash(),
                tool_input: serde_json::json!({ "command": args.cmd }),
            })
    }

    fn post_tool_use_payload(
        &self,
        invocation: &ToolInvocation,
        result: &Self::Output,
    ) -> Option<PostToolUsePayload> {
        let ToolPayload::Function { .. } = &invocation.payload else {
            return None;
        };

        let command = result.hook_command.clone()?;
        let tool_use_id = if result.event_call_id.is_empty() {
            invocation.call_id.clone()
        } else {
            result.event_call_id.clone()
        };
        let tool_response = result.post_tool_use_response(&tool_use_id, &invocation.payload)?;
        Some(PostToolUsePayload {
            tool_name: HookToolName::bash(),
            tool_use_id,
            tool_input: serde_json::json!({ "command": command }),
            tool_response,
        })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            tool_name,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "exec_command_in_environment received unsupported payload".to_string(),
                ));
            }
        };

        // Pre-validate env_id → readable error before we allocate a process
        // id or touch the manager. Mirrors the descriptive-error contract
        // established by P3.4b/P3.4c.
        let cwd = resolve_workdir_base_path(&arguments, &turn.cwd)?;
        let env_args: ExecCommandInEnvironmentArgs =
            parse_arguments_with_base_path(&arguments, &cwd)?;

        if env_args.environment_id.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "environment_id is required for exec_command_in_environment".to_string(),
            ));
        }

        let Some(turn_environment) = turn.select_environment(Some(&env_args.environment_id)) else {
            return Err(FunctionCallError::RespondToModel(unknown_env_message(
                &env_args.environment_id,
                &turn.environments,
            )));
        };

        // Use the chosen env's filesystem for apply_patch interception so
        // the tool actually patches files in the requested env (P3.4c
        // pattern: env_id flows all the way to the fs lookup, not just
        // to the runtime).
        let fs = turn_environment.environment.get_filesystem();

        let manager: &UnifiedExecProcessManager = &session.services.unified_exec_manager;
        let context = UnifiedExecContext::new(session.clone(), turn.clone(), call_id.clone());

        let (env_id, args) = env_args.into_exec_command_args();
        handle_exec_command_request(
            args,
            Some(env_id),
            cwd,
            session.clone(),
            turn.clone(),
            tracker.clone(),
            &context,
            tool_name.name.as_str(),
            fs.as_ref(),
            manager,
        )
        .await
    }
}

fn unknown_env_message(
    requested: &str,
    environments: &[crate::session::turn_context::TurnEnvironment],
) -> String {
    if environments.is_empty() {
        format!("environment_id `{requested}` is not available: this turn has no environments")
    } else {
        let available: Vec<&str> = environments
            .iter()
            .map(|e| e.environment_id.as_str())
            .collect();
        format!(
            "environment_id `{requested}` not found; available: [{}]",
            available.join(", ")
        )
    }
}

#[cfg(test)]
#[path = "exec_command_in_environment_tests.rs"]
mod tests;
