//! Internal tool activity models used by chat cells and interrupt queueing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::diff_model::FileChange;
use codex_app_server_protocol::CommandExecOutputStream;
use codex_app_server_protocol::CommandExecutionSource;
use codex_app_server_protocol::CommandExecutionStatus;
use codex_app_server_protocol::HookRunSummary;
use codex_app_server_protocol::PatchApplyStatus;
use codex_protocol::mcp::CallToolResult;
use codex_protocol::parse_command::ParsedCommand;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct McpInvocation {
    pub(crate) server: String,
    pub(crate) tool: String,
    pub(crate) arguments: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct McpToolCallBeginEvent {
    pub(crate) call_id: String,
    pub(crate) invocation: McpInvocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_app_resource_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct McpToolCallEndEvent {
    pub(crate) call_id: String,
    pub(crate) invocation: McpInvocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_app_resource_uri: Option<String>,
    pub(crate) duration: Duration,
    pub(crate) result: Result<CallToolResult, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WebSearchBeginEvent {
    pub(crate) call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WebSearchEndEvent {
    pub(crate) call_id: String,
    pub(crate) query: String,
    pub(crate) action: codex_protocol::models::WebSearchAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImageGenerationBeginEvent {
    pub(crate) call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImageGenerationEndEvent {
    pub(crate) call_id: String,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) revised_prompt: Option<String>,
    pub(crate) result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) saved_path: Option<AbsolutePathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecCommandBeginEvent {
    pub(crate) call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_id: Option<String>,
    pub(crate) turn_id: String,
    pub(crate) command: Vec<String>,
    pub(crate) cwd: AbsolutePathBuf,
    pub(crate) parsed_cmd: Vec<ParsedCommand>,
    #[serde(default)]
    pub(crate) source: CommandExecutionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) interaction_input: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecCommandEndEvent {
    pub(crate) call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_id: Option<String>,
    pub(crate) turn_id: String,
    pub(crate) command: Vec<String>,
    pub(crate) cwd: AbsolutePathBuf,
    pub(crate) parsed_cmd: Vec<ParsedCommand>,
    #[serde(default)]
    pub(crate) source: CommandExecutionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) interaction_input: Option<String>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    #[serde(default)]
    pub(crate) aggregated_output: String,
    pub(crate) exit_code: i32,
    pub(crate) duration: Duration,
    pub(crate) formatted_output: String,
    pub(crate) status: CommandExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ViewImageToolCallEvent {
    pub(crate) call_id: String,
    pub(crate) path: AbsolutePathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ExecCommandOutputDeltaEvent {
    pub(crate) call_id: String,
    pub(crate) stream: CommandExecOutputStream,
    pub(crate) chunk: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TerminalInteractionEvent {
    pub(crate) call_id: String,
    pub(crate) process_id: String,
    pub(crate) stdin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PatchApplyBeginEvent {
    pub(crate) call_id: String,
    #[serde(default)]
    pub(crate) turn_id: String,
    pub(crate) auto_approved: bool,
    pub(crate) changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PatchApplyEndEvent {
    pub(crate) call_id: String,
    #[serde(default)]
    pub(crate) turn_id: String,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) success: bool,
    #[serde(default)]
    pub(crate) changes: HashMap<PathBuf, FileChange>,
    pub(crate) status: PatchApplyStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HookStartedEvent {
    pub(crate) turn_id: Option<String>,
    pub(crate) run: HookRunSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HookCompletedEvent {
    pub(crate) turn_id: Option<String>,
    pub(crate) run: HookRunSummary,
}
