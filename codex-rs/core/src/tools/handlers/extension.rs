//! Dispatches model tool calls to host-provided session runtime extensions.

use crate::function_tool::FunctionCallError;
use crate::session_extension::SessionRuntimeHandle;
use crate::session_extension::SessionToolError;
use crate::session_extension::SessionToolInvocation;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use std::sync::Arc;

pub struct ExtensionToolHandler;

impl ToolHandler for ExtensionToolHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::Custom { .. }
        )
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            tool_name,
            call_id,
            ..
        } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            ToolPayload::Custom { input } => input,
            ToolPayload::ToolSearch { .. }
            | ToolPayload::LocalShell { .. }
            | ToolPayload::Mcp { .. } => {
                return Err(FunctionCallError::RespondToModel(
                    "extension tool handler received unsupported payload".to_string(),
                ));
            }
        };
        let Some(extension) = session.runtime_extension() else {
            return Err(FunctionCallError::Fatal(format!(
                "no runtime extension installed for tool {}",
                tool_name.display()
            )));
        };
        let output = extension
            .handle_tool_call(
                SessionRuntimeHandle::new(Arc::clone(&session)),
                SessionToolInvocation {
                    tool_name,
                    call_id,
                    turn_id: turn.sub_id.clone(),
                    mode: turn.collaboration_mode.mode,
                    arguments,
                },
            )
            .await
            .map_err(|err| match err {
                SessionToolError::RespondToModel(message) => {
                    FunctionCallError::RespondToModel(message)
                }
                SessionToolError::Fatal(message) => FunctionCallError::Fatal(message),
            })?;
        Ok(FunctionToolOutput::from_content(
            output.body,
            output.success,
        ))
    }
}
