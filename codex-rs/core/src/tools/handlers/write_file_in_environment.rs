#![allow(dead_code)]
// All public items in this module are dead code until Pa.7 wires the new
// handler into the registry. The `#![allow(dead_code)]` above keeps the
// noise contained to this file rather than relying on per-item attributes
// that would have to be removed later.

//! `write_file_in_environment` — env-aware one-shot file write
//! counterpart to `read_file_in_environment`, added in spec § Pa.6.
//!
//! `apply_patch_in_environment` (Pa.2) already covers incremental edits
//! and `Add File` hunks, but assembling a patch envelope to drop a fresh
//! file onto a non-default environment is awkward. This handler exposes
//! a simpler write path so the LLM can replace a file's content in one
//! call. `create_dirs` is opt-in (default `false`) so a typo in the
//! parent path surfaces as an error instead of silently materializing a
//! deep directory tree.

use std::path::PathBuf;

use codex_exec_server::CreateDirectoryOptions;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub(crate) const TOOL_NAME: &str = "write_file_in_environment";

pub struct WriteFileInEnvironmentHandler;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WriteFileInEnvironmentArgs {
    pub(crate) environment_id: String,
    pub(crate) path: String,
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) create_dirs: bool,
}

impl ToolHandler for WriteFileInEnvironmentHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Replaces file contents.
        true
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "write_file_in_environment received unsupported payload".to_string(),
                ));
            }
        };

        let args: WriteFileInEnvironmentArgs = parse_arguments(&arguments)?;

        if args.environment_id.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "environment_id is required for write_file_in_environment".to_string(),
            ));
        }

        if args.path.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path is required for write_file_in_environment".to_string(),
            ));
        }

        // `content` may be empty (truncating a file to zero bytes is a
        // legitimate operation); only enforce the field's presence at
        // the serde layer.

        let raw_path = PathBuf::from(&args.path);
        if !raw_path.is_absolute() {
            return Err(FunctionCallError::RespondToModel(
                "path must be an absolute path".to_string(),
            ));
        }

        let abs_path = AbsolutePathBuf::from_absolute_path(&raw_path).map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to normalize path `{}`: {err}",
                raw_path.display()
            ))
        })?;

        let Some(turn_environment) = turn.select_environment(Some(&args.environment_id)) else {
            return Err(FunctionCallError::RespondToModel(unknown_env_message(
                &args.environment_id,
                &turn.environments,
            )));
        };

        // Mirror the sandbox-context plumbing from the read sibling:
        // only build the context for remote environments (local
        // filesystems ignore it).
        let sandbox = turn_environment
            .environment
            .is_remote()
            .then(|| turn.file_system_sandbox_context(/*additional_permissions*/ None));

        let fs = turn_environment.environment.get_filesystem();

        if args.create_dirs
            && let Some(parent) = raw_path.parent()
            && !parent.as_os_str().is_empty()
        {
            let parent_abs =
                AbsolutePathBuf::from_absolute_path(parent).map_err(|err| {
                    FunctionCallError::RespondToModel(format!(
                        "failed to normalize parent path `{}`: {err}",
                        parent.display()
                    ))
                })?;
            fs.create_directory(
                &parent_abs,
                CreateDirectoryOptions { recursive: true },
                sandbox.as_ref(),
            )
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "failed to create parent directory `{}` on environment `{}`: {err}",
                    parent.display(),
                    args.environment_id,
                ))
            })?;
        }

        let content_bytes = args.content.into_bytes();
        let byte_count = content_bytes.len();
        fs.write_file(&abs_path, content_bytes, sandbox.as_ref())
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "failed to write file `{}` on environment `{}`: {err}",
                    raw_path.display(),
                    args.environment_id,
                ))
            })?;

        Ok(FunctionToolOutput::from_text(
            format!(
                "Wrote {byte_count} bytes to {} on {}",
                raw_path.display(),
                args.environment_id,
            ),
            Some(true),
        ))
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
#[path = "write_file_in_environment_tests.rs"]
mod tests;
