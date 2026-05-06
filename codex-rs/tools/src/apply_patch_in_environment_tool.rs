use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

/// Builds the env-aware mirror of `apply_patch` (JSON variant). The native
/// `apply_patch` tool stays byte-identical to upstream so the model sees its
/// training-time schema; this parallel tool prepends a required
/// `environment_id` field that routes the patch to a non-default execution
/// environment's filesystem.
///
/// Note: we intentionally do NOT add a freeform/Lark variant. The Lark
/// grammar cannot express an `environment_id` field, so env routing is
/// JSON-only. See spec § Pa.2.
pub fn create_apply_patch_in_environment_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Required. Identifier of the execution environment whose filesystem to apply this patch to. \
                 See <environments> in the system prompt for available ids. Use `list_environments` to refresh \
                 the catalog at runtime."
                    .to_string(),
            )),
        ),
        (
            "input".to_string(),
            JsonSchema::string(Some(
                "The entire contents of the apply_patch command".to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "apply_patch_in_environment".to_string(),
        description: "Applies a patch to the named execution environment's filesystem. Mirrors the JSON variant of `apply_patch` but routes to a non-default environment via `environment_id`. Use `apply_patch` directly when targeting the default environment.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["environment_id".to_string(), "input".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

#[cfg(test)]
#[path = "apply_patch_in_environment_tool_tests.rs"]
mod tests;
