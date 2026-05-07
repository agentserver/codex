//! `read_file_in_environment` — env-aware file read primitive added in
//! spec § Pa.6.
//!
//! Unlike Pa.4 / Pa.5 which mirror native upstream tools, there is no
//! native `read_file` tool to mirror — historically the LLM reads files
//! via `shell` / `cat`. This handler exposes a dedicated read path so
//! the LLM can fetch a file's content from a non-default environment
//! without spawning a process. The body is decoded as UTF-8; non-text
//! files surface a clear error pointing at `view_image_in_environment`
//! for images and `exec_command_in_environment` for binary tooling.
//!
//! # Pa.6 limitation: full-file UTF-8 only (modulo `byte_range`)
//!
//! The optional `byte_range` reads a slice of the file's raw bytes, but
//! the slice is still decoded as UTF-8 and rejected if the slice cuts a
//! multi-byte character or the file is not UTF-8. Streaming or paginated
//! reads of arbitrary binary files are out of scope; the LLM should use
//! the binary-friendly variants for those cases.

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

/// Public tool name. Mirrors the schema name in
/// `codex_tools::create_read_file_in_environment_tool`. Currently only
/// referenced by tests; runtime registration uses the string literal in
/// `codex_tools::tool_registry_plan::build_tool_registry_plan`.
#[allow(dead_code)]
pub(crate) const TOOL_NAME: &str = "read_file_in_environment";

pub struct ReadFileInEnvironmentHandler;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReadFileInEnvironmentArgs {
    pub(crate) environment_id: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) byte_range: Option<ByteRange>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct ByteRange {
    pub(crate) start: u64,
    pub(crate) end: u64,
}

impl ToolHandler for ReadFileInEnvironmentHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Read-only file load.
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "read_file_in_environment received unsupported payload".to_string(),
                ));
            }
        };

        let args: ReadFileInEnvironmentArgs = parse_arguments(&arguments)?;

        if args.environment_id.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "environment_id is required for read_file_in_environment".to_string(),
            ));
        }

        if args.path.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path is required for read_file_in_environment".to_string(),
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

        // Mirror the sandbox-context plumbing from `view_image_in_environment`:
        // only build the context for remote environments (local
        // filesystems ignore it).
        let sandbox = turn_environment
            .environment
            .is_remote()
            .then(|| turn.file_system_sandbox_context(/*additional_permissions*/ None));

        let fs = turn_environment.environment.get_filesystem();

        let bytes = fs
            .read_file(&abs_path, sandbox.as_ref())
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "failed to read file `{}` on environment `{}`: {err}",
                    raw_path.display(),
                    args.environment_id,
                ))
            })?;

        let slice = match args.byte_range {
            None => bytes,
            Some(range) => slice_bytes(bytes, range, &raw_path, &args.environment_id)?,
        };

        let len = slice.len();
        let text = String::from_utf8(slice).map_err(|_| {
            FunctionCallError::RespondToModel(format!(
                "file `{}` on environment `{}` is not valid UTF-8 (size {len} bytes); use \
                 `view_image_in_environment` for images or `exec_command_in_environment` for \
                 binary tooling",
                raw_path.display(),
                args.environment_id,
            ))
        })?;

        Ok(FunctionToolOutput::from_text(text, Some(true)))
    }
}

/// Slice the raw byte buffer to the requested `[start, end)` range,
/// validating that `start <= end <= len`. Returns a descriptive
/// `RespondToModel` error if the bounds are out of order or out of
/// range.
fn slice_bytes(
    bytes: Vec<u8>,
    range: ByteRange,
    path: &std::path::Path,
    environment_id: &str,
) -> Result<Vec<u8>, FunctionCallError> {
    let len = bytes.len() as u64;
    if range.start > range.end {
        return Err(FunctionCallError::RespondToModel(format!(
            "byte_range.start ({}) must be <= byte_range.end ({}) for file `{}` on environment \
             `{}`",
            range.start,
            range.end,
            path.display(),
            environment_id,
        )));
    }
    if range.end > len {
        return Err(FunctionCallError::RespondToModel(format!(
            "byte_range.end ({}) is past file size ({len}) for file `{}` on environment `{}`",
            range.end,
            path.display(),
            environment_id,
        )));
    }
    let start = range.start as usize;
    let end = range.end as usize;
    Ok(bytes[start..end].to_vec())
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
#[path = "read_file_in_environment_tests.rs"]
mod tests;
