//! `apply_patch_in_environment` — env-aware mirror of the native
//! `apply_patch` JSON tool, added in spec § Pa.2.
//!
//! The native `apply_patch` tool stays byte-identical to upstream codex
//! (Pa.0 reverted the schema modifications) so the model sees its
//! training-time signature. This handler exposes the env routing as a
//! parallel tool whose JSON schema prepends a required `environment_id`
//! field.
//!
//! All patch verification, approval, and apply-patch runtime delegation
//! is shared with the legacy `apply_patch` path via
//! `handle_apply_patch_request` in the `apply_patch` handler module.

use crate::function_tool::FunctionCallError;
use crate::tools::context::ApplyPatchToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::apply_patch::handle_apply_patch_request;
use crate::tools::handlers::parse_arguments;
use crate::tools::hook_names::HookToolName;
use crate::tools::registry::PostToolUsePayload;
use crate::tools::registry::PreToolUsePayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use serde::Deserialize;

pub(crate) const TOOL_NAME: &str = "apply_patch_in_environment";

pub struct ApplyPatchInEnvironmentHandler;

/// Args for `apply_patch_in_environment`. Mirrors the JSON variant of
/// `apply_patch` (`ApplyPatchToolArgs`) but the `environment_id` field is
/// **required** rather than optional.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApplyPatchInEnvironmentArgs {
    pub(crate) environment_id: String,
    pub(crate) input: String,
}

impl ToolHandler for ApplyPatchInEnvironmentHandler {
    type Output = ApplyPatchToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        // env-aware variant is JSON-only; no freeform/Lark surface (Lark
        // grammar can't express env_id; see spec § Pa.2).
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Patches always modify files.
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

        parse_arguments::<ApplyPatchInEnvironmentArgs>(arguments)
            .ok()
            .map(|args| PreToolUsePayload {
                // Patches are file operations, not bash commands — report
                // as the apply_patch hook tool name (matches the native
                // apply_patch handler).
                tool_name: HookToolName::apply_patch(),
                tool_input: serde_json::json!({ "command": args.input }),
            })
    }

    fn post_tool_use_payload(
        &self,
        invocation: &ToolInvocation,
        result: &Self::Output,
    ) -> Option<PostToolUsePayload> {
        let ToolPayload::Function { arguments } = &invocation.payload else {
            return None;
        };
        let args = parse_arguments::<ApplyPatchInEnvironmentArgs>(arguments).ok()?;
        let tool_response =
            result.post_tool_use_response(&invocation.call_id, &invocation.payload)?;
        Some(PostToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_use_id: invocation.call_id.clone(),
            tool_input: serde_json::json!({ "command": args.input }),
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
                    "apply_patch_in_environment received unsupported payload".to_string(),
                ));
            }
        };

        let args: ApplyPatchInEnvironmentArgs = parse_arguments(&arguments)?;

        if args.environment_id.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "environment_id is required for apply_patch_in_environment".to_string(),
            ));
        }

        // Pre-validate env_id → readable error before we touch the patch
        // verification pipeline (mirrors Pa.1's
        // `exec_command_in_environment` handler).
        if turn.select_environment(Some(&args.environment_id)).is_none() {
            return Err(FunctionCallError::RespondToModel(unknown_env_message(
                &args.environment_id,
                &turn.environments,
            )));
        }

        handle_apply_patch_request(
            args.input,
            Some(args.environment_id),
            session,
            turn,
            tracker,
            call_id,
            tool_name.display(),
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
#[path = "apply_patch_in_environment_tests.rs"]
mod tests;
