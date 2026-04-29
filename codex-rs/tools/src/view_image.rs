use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use codex_protocol::models::VIEW_IMAGE_TOOL_NAME;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewImageToolOptions {
    pub can_request_original_image_detail: bool,
    pub has_multiple_environments: bool,
}

pub fn create_view_image_tool(options: ViewImageToolOptions) -> ToolSpec {
    let path_description = if options.has_multiple_environments {
        "Path to an image file. Plain relative paths resolve against the selected environment's current working directory, and env-qualified paths use oai_env://<environment_id>/<absolute-path>."
    } else {
        "Local filesystem path to an image file"
    };
    let mut properties = BTreeMap::from([(
        "path".to_string(),
        JsonSchema::string(Some(path_description.to_string())),
    )]);
    if options.has_multiple_environments {
        properties.insert(
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Optional selected environment id. Omit to use the primary environment. If path uses oai_env://<environment_id>/<absolute-path>, this value must match.".to_string(),
            )),
        );
    }
    if options.can_request_original_image_detail {
        properties.insert(
            "detail".to_string(),
            JsonSchema::string(Some(
                "Optional detail override. The only supported value is `original`; omit this field for default resized behavior. Use `original` to preserve the file's original resolution instead of resizing to fit. This is important when high-fidelity image perception or precise localization is needed, especially for CUA agents.".to_string(),
            )),
        );
    }

    ToolSpec::Function(ResponsesApiTool {
        name: VIEW_IMAGE_TOOL_NAME.to_string(),
        description: "View a local image from the filesystem (only use if given a full filepath by the user, and the image isn't already attached to the thread context within <image ...> tags)."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, Some(vec!["path".to_string()]), Some(false.into())),
        output_schema: Some(view_image_output_schema()),
    })
}

fn view_image_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "image_url": {
                "type": "string",
                "description": "Data URL for the loaded image."
            },
            "detail": {
                "type": ["string", "null"],
                "description": "Image detail hint returned by view_image. Returns `original` when original resolution is preserved, otherwise `null`."
            }
        },
        "required": ["image_url", "detail"],
        "additionalProperties": false
    })
}
