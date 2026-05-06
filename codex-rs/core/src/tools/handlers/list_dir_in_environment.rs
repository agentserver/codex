#![allow(dead_code)]
// All public items in this module are dead code until Pa.7 wires the new
// handler into the registry. The `#![allow(dead_code)]` above keeps the
// noise contained to this file rather than relying on per-item attributes
// that would have to be removed later.

//! `list_dir_in_environment` — env-aware mirror of the native `list_dir`
//! tool, added in spec § Pa.4.
//!
//! The native `list_dir` tool stays byte-identical to upstream codex (it
//! reads the local filesystem via `tokio::fs`). This handler exposes a
//! parallel surface that routes the directory read through a chosen
//! environment's `ExecutorFileSystem::read_directory`, so the LLM can list
//! a remote env's filesystem without shelling out to `ls`.
//!
//! # Pa.4 limitation: shallow listing, no pagination
//!
//! The schema intentionally exposes only `environment_id` + `path`. The
//! upstream `list_dir` tool's `offset` / `limit` / `depth` knobs rely on
//! `tokio::fs` semantics that are not part of the
//! `ExecutorFileSystem::read_directory` contract today (it returns a flat
//! `Vec<ReadDirectoryEntry>` with no recursion). Replicating those knobs
//! would require either widening the trait or post-filtering remote
//! responses; both are deferred until a concrete need arises.

use std::path::PathBuf;

use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub(crate) const TOOL_NAME: &str = "list_dir_in_environment";

pub struct ListDirInEnvironmentHandler;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ListDirInEnvironmentArgs {
    pub(crate) environment_id: String,
    pub(crate) path: String,
}

impl ToolHandler for ListDirInEnvironmentHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Read-only directory listing.
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "list_dir_in_environment received unsupported payload".to_string(),
                ));
            }
        };

        let args: ListDirInEnvironmentArgs = parse_arguments(&arguments)?;

        if args.environment_id.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "environment_id is required for list_dir_in_environment".to_string(),
            ));
        }

        if args.path.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path is required for list_dir_in_environment".to_string(),
            ));
        }

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

        let fs = turn_environment.environment.get_filesystem();
        let entries = fs
            .read_directory(&abs_path, /*sandbox*/ None)
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "failed to read directory `{}` on environment `{}`: {err}",
                    raw_path.display(),
                    args.environment_id
                ))
            })?;

        Ok(FunctionToolOutput::from_text(
            format_listing(&args.environment_id, &raw_path, entries),
            Some(true),
        ))
    }
}

/// Format the listing into one-entry-per-line plain text. Mirrors the
/// upstream `list_dir` "header + entries" shape: the first line records
/// the resolved absolute path (and env id), and each subsequent line is a
/// single entry suffixed with `/` for directories. Symlinks are reported
/// without a suffix because `ReadDirectoryEntry` does not expose a
/// symlink flag (the trait only returns `is_directory` / `is_file`).
fn format_listing(
    environment_id: &str,
    path: &std::path::Path,
    mut entries: Vec<codex_exec_server::ReadDirectoryEntry>,
) -> String {
    entries.sort_unstable_by(|a, b| a.file_name.cmp(&b.file_name));
    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(format!(
        "Environment: {environment_id}\nAbsolute path: {}",
        path.display()
    ));
    for entry in entries {
        let suffix = if entry.is_directory { "/" } else { "" };
        lines.push(format!("{}{}", entry.file_name, suffix));
    }
    lines.join("\n")
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
#[path = "list_dir_in_environment_tests.rs"]
mod tests;
