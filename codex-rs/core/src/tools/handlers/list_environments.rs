#![allow(dead_code)]
// All public items in this module are dead code until Pa.7 wires the new
// handler into the registry. The `#![allow(dead_code)]` above keeps the
// noise contained to this file rather than relying on per-item attributes
// that would have to be removed later.

//! `list_environments` — read-only catalog tool, added in spec § Pa.3.
//!
//! Returns the list of execution environments visible to the current turn so
//! the model can refresh the static `<environments>` block injected into the
//! system prompt at turn start (envs may go online/offline mid-turn).
//!
//! The handler is intentionally stateless: it reads `turn.environments`,
//! produces a JSON catalog, and returns. No env routing, no process spawn,
//! no filesystem access.
//!
//! # Pa.3 limitation: `include_status`
//!
//! The schema accepts an `include_status` boolean. Pa.3 parses it but does
//! NOT implement bridge-pinging — the response always omits the `online`
//! field. The bridge-ping infrastructure does not exist in codex core today;
//! adding it is deferred to Pa.7+ if a concrete need arises. This tool
//! responds successfully when `include_status=true`; it just produces the
//! same shape it would for `include_status=false`.

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use serde::Deserialize;
use serde_json::json;

pub(crate) const TOOL_NAME: &str = "list_environments";

pub struct ListEnvironmentsHandler;

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ListEnvironmentsArgs {
    /// Pa.3: parsed but ignored — see module docs.
    #[serde(default)]
    pub(crate) include_status: bool,
}

impl ToolHandler for ListEnvironmentsHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        // Pure read of the in-memory turn catalog.
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "list_environments received unsupported payload".to_string(),
                ));
            }
        };

        // Accept missing/empty arguments as "{}" so a no-arg call works.
        // The Responses API tends to send "" for tools with no required
        // fields when the model didn't include any parameters.
        let trimmed = arguments.trim();
        let _args: ListEnvironmentsArgs = if trimmed.is_empty() {
            ListEnvironmentsArgs::default()
        } else {
            parse_arguments(&arguments)?
        };

        let body = build_catalog(&turn.environments);
        Ok(FunctionToolOutput::from_text(body.to_string(), Some(true)))
    }
}

/// Build the JSON catalog payload from a slice of `TurnEnvironment`. Pure
/// (no I/O), so it can be unit-tested without spinning up a session.
///
/// Empty input -> `{"environments": []}` (not an error). The first element
/// is treated as the primary/default for the turn; this matches
/// `TurnContext::select_environment(None)`, which returns the first element.
pub(crate) fn build_catalog(
    environments: &[crate::session::turn_context::TurnEnvironment],
) -> serde_json::Value {
    let primary_id = environments.first().map(|env| env.environment_id.as_str());

    let entries: Vec<serde_json::Value> = environments
        .iter()
        .map(|env| {
            let is_default = primary_id == Some(env.environment_id.as_str());
            let mut entry = serde_json::Map::new();
            entry.insert(
                "id".to_string(),
                serde_json::Value::String(env.environment_id.clone()),
            );
            if let Some(description) = env.environment.description() {
                entry.insert(
                    "description".to_string(),
                    serde_json::Value::String(description.to_string()),
                );
            }
            entry.insert(
                "is_default".to_string(),
                serde_json::Value::Bool(is_default),
            );
            // Pa.3: `online` intentionally omitted — bridge-ping is not
            // implemented (see module docs).
            serde_json::Value::Object(entry)
        })
        .collect();

    json!({ "environments": entries })
}

#[cfg(test)]
#[path = "list_environments_tests.rs"]
mod tests;
