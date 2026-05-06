#![allow(dead_code)]
// All public items in this module are dead code until Pa.7 wires the new
// handler into the registry. The `#![allow(dead_code)]` above keeps the
// noise contained to this file rather than relying on per-item attributes
// that would have to be removed later.

//! `view_image_in_environment` — env-aware mirror of the native
//! `view_image` tool, added in spec § Pa.5.
//!
//! The native `view_image` tool stays byte-identical to upstream codex
//! (it loads from the local filesystem). This handler exposes a parallel
//! surface that routes the image read through a chosen environment's
//! `ExecutorFileSystem::read_file`, so the LLM can attach an image from
//! a remote env's filesystem without copying bytes through `cat | base64`.
//!
//! # Pa.5 limitation: no `detail = "original"` override
//!
//! `view_image` exposes `detail = "original"` gated on
//! `can_request_original_image_detail(model_info)`. Re-plumbing that
//! capability check into the env-aware tool would risk silent divergence
//! as the upstream gating evolves. The Pa.5 surface intentionally
//! returns the resized representation only; a future scenario can add
//! the field with the same gating contract.

use std::path::PathBuf;

use codex_protocol::items::ImageViewItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::DEFAULT_IMAGE_DETAIL;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ImageDetail;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::openai_models::InputModality;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_image::PromptImageMode;
use codex_utils_image::load_for_prompt_bytes;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub(crate) const TOOL_NAME: &str = "view_image_in_environment";

const VIEW_IMAGE_UNSUPPORTED_MESSAGE: &str =
    "view_image_in_environment is not allowed because you do not support image inputs";

pub struct ViewImageInEnvironmentHandler;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ViewImageInEnvironmentArgs {
    pub(crate) environment_id: String,
    pub(crate) path: String,
}

impl ToolHandler for ViewImageInEnvironmentHandler {
    type Output = ViewImageInEnvironmentOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Read-only image load.
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        // Mirror the native `view_image` modality gate so the env-aware
        // tool surface is also disallowed for text-only models.
        if !invocation
            .turn
            .model_info
            .input_modalities
            .contains(&InputModality::Image)
        {
            return Err(FunctionCallError::RespondToModel(
                VIEW_IMAGE_UNSUPPORTED_MESSAGE.to_string(),
            ));
        }

        let ToolInvocation {
            session,
            turn,
            payload,
            call_id,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "view_image_in_environment received unsupported payload".to_string(),
                ));
            }
        };

        let args: ViewImageInEnvironmentArgs = parse_arguments(&arguments)?;

        if args.environment_id.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "environment_id is required for view_image_in_environment".to_string(),
            ));
        }

        if args.path.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path is required for view_image_in_environment".to_string(),
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

        // Mirror the sandbox-context plumbing from the native `view_image`
        // handler: only build the context for remote environments (local
        // filesystems ignore it).
        let sandbox = turn_environment
            .environment
            .is_remote()
            .then(|| turn.file_system_sandbox_context(/*additional_permissions*/ None));

        let fs = turn_environment.environment.get_filesystem();

        let metadata = fs
            .get_metadata(&abs_path, sandbox.as_ref())
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "unable to locate image at `{}` on environment `{}`: {err}",
                    raw_path.display(),
                    args.environment_id,
                ))
            })?;
        if !metadata.is_file {
            return Err(FunctionCallError::RespondToModel(format!(
                "image path `{}` on environment `{}` is not a file",
                raw_path.display(),
                args.environment_id,
            )));
        }

        let file_bytes = fs.read_file(&abs_path, sandbox.as_ref()).await.map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "unable to read image at `{}` on environment `{}`: {err}",
                raw_path.display(),
                args.environment_id,
            ))
        })?;

        // Pa.5 always uses the resized representation. See module docs
        // for why we do not re-plumb the `detail = "original"` knob.
        let image = load_for_prompt_bytes(abs_path.as_path(), file_bytes, PromptImageMode::ResizeToFit)
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "unable to process image at `{}` on environment `{}`: {err}",
                    raw_path.display(),
                    args.environment_id,
                ))
            })?;
        let image_url = image.into_data_url();
        let image_detail = Some(DEFAULT_IMAGE_DETAIL);

        let event_path = abs_path.clone();
        let item = TurnItem::ImageView(ImageViewItem {
            id: call_id,
            path: event_path,
        });
        session.emit_turn_item_started(turn.as_ref(), &item).await;
        session.emit_turn_item_completed(turn.as_ref(), item).await;

        Ok(ViewImageInEnvironmentOutput {
            image_url,
            image_detail,
        })
    }
}

pub struct ViewImageInEnvironmentOutput {
    pub(crate) image_url: String,
    pub(crate) image_detail: Option<ImageDetail>,
}

impl ToolOutput for ViewImageInEnvironmentOutput {
    fn log_preview(&self) -> String {
        self.image_url.clone()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        let body =
            FunctionCallOutputBody::ContentItems(vec![FunctionCallOutputContentItem::InputImage {
                image_url: self.image_url.clone(),
                detail: self.image_detail,
            }]);
        let output = FunctionCallOutputPayload {
            body,
            success: Some(true),
        };

        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> serde_json::Value {
        serde_json::json!({
            "image_url": self.image_url,
            "detail": self.image_detail
        })
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
#[path = "view_image_in_environment_tests.rs"]
mod tests;
