use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

/// Builds the read-only `list_environments` tool. Returns the runtime catalog
/// of execution environments visible to this turn so the model can refresh
/// the static `<environments>` block injected at turn start (the system
/// prompt's snapshot may go stale mid-turn).
///
/// See spec § Pa.3.
pub fn create_list_environments_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "include_status".to_string(),
        JsonSchema::boolean(Some(
            "Optional. When true, include online/offline status for each environment by \
             pinging its bridge endpoint. Adds latency proportional to environment count. \
             Defaults to false (returns the static catalog only)."
                .to_string(),
        )),
    )]);

    ToolSpec::Function(ResponsesApiTool {
        name: "list_environments".to_string(),
        description: "Returns the catalog of available execution environments for this turn. Each entry includes id, description, and whether it is the default. Use this to discover env ids for the `*_in_environment` tool family. The system prompt's <environments> block contains the same information at turn start; this tool refreshes the catalog (e.g., to check status changes mid-turn).".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec![]),
            Some(false.into()),
        ),
        output_schema: Some(list_environments_output_schema()),
    })
}

/// JSON Schema for the `list_environments` response. The `online` field is
/// declared in the schema for forward compatibility but is currently always
/// omitted from responses (Pa.3 does not implement bridge-pinging; see the
/// handler module for details).
pub(crate) fn list_environments_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "environments": {
                "type": "array",
                "description": "List of available environments.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "description": { "type": "string" },
                        "is_default": { "type": "boolean" },
                        "online": {
                            "type": "boolean",
                            "description": "Only present when include_status was true."
                        }
                    },
                    "required": ["id", "is_default"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["environments"],
        "additionalProperties": false
    })
}

#[cfg(test)]
#[path = "list_environments_tool_tests.rs"]
mod tests;
