use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

/// Builds the env-aware mirror of `view_image`. The native `view_image`
/// tool stays byte-identical to upstream so the model sees its
/// training-time schema; this parallel tool prepends a required
/// `environment_id` field that routes the image read to a non-default
/// execution environment's filesystem.
///
/// Unlike `view_image`, the env-aware variant intentionally drops the
/// optional `detail` knob. `detail = "original"` is gated on
/// `can_request_original_image_detail(model_info)` for the local tool;
/// rather than re-plumbing that capability check into the env-aware
/// surface (and risking divergence as the upstream gating evolves), the
/// Pa.5 surface always returns the resized representation. If a future
/// scenario needs original-resolution image reads from a remote env, add
/// the field at that point with the same gating contract.
///
/// See spec § Pa.5.
pub fn create_view_image_in_environment_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Required. Identifier of the execution environment whose filesystem to read \
                 the image from. See <environments> in the system prompt for available ids. \
                 Use `list_environments` to refresh the catalog at runtime."
                    .to_string(),
            )),
        ),
        (
            "path".to_string(),
            JsonSchema::string(Some(
                "Required. Absolute path of the image file to load on the named environment's \
                 filesystem."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "view_image_in_environment".to_string(),
        description: "Loads an image from the named execution environment's filesystem and attaches it to the conversation as an image input. Mirrors `view_image` but routes the read to a non-default environment via `environment_id`. Always returns the default resized representation; the `detail = original` override exposed by `view_image` is not available here.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["environment_id".to_string(), "path".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(view_image_in_environment_output_schema()),
    })
}

fn view_image_in_environment_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "image_url": {
                "type": "string",
                "description": "Data URL for the loaded image."
            },
            "detail": {
                "type": ["string", "null"],
                "description": "Image detail hint. Always the default for view_image_in_environment; never `original`."
            }
        },
        "required": ["image_url", "detail"],
        "additionalProperties": false
    })
}

#[cfg(test)]
#[path = "view_image_in_environment_tool_tests.rs"]
mod tests;
