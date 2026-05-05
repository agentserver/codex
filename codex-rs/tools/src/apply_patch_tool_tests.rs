use super::*;
use crate::JsonSchema;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn create_apply_patch_freeform_tool_matches_expected_spec() {
    assert_eq!(
        create_apply_patch_freeform_tool(),
        ToolSpec::Freeform(FreeformTool {
            name: "apply_patch".to_string(),
            description:
                "Use the `apply_patch` tool to edit files. This is a FREEFORM tool, so do not wrap the patch in JSON."
                    .to_string(),
            format: FreeformToolFormat {
                r#type: "grammar".to_string(),
                syntax: "lark".to_string(),
                definition: APPLY_PATCH_LARK_GRAMMAR.to_string(),
            },
        })
    );
}

#[test]
fn create_apply_patch_json_tool_matches_expected_spec() {
    let spec = create_apply_patch_json_tool();
    assert_eq!(
        spec,
        ToolSpec::Function(ResponsesApiTool {
            name: "apply_patch".to_string(),
            description: APPLY_PATCH_JSON_TOOL_DESCRIPTION.to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                BTreeMap::from([
                    (
                        "input".to_string(),
                        JsonSchema::string(Some(
                            "The entire contents of the apply_patch command".to_string(),
                        ),),
                    ),
                    (
                        "environment_id".to_string(),
                        JsonSchema::string(Some(
                            "Optional. Identifier of the execution environment to apply this patch in. \
                             Defaults to the primary environment for the turn. See <environments> in the \
                             system prompt for available ids."
                                .to_string(),
                        )),
                    ),
                ]),
                Some(vec!["input".to_string()]),
                Some(false.into())
            ),
            output_schema: None,
        })
    );

    let parameters = match &spec {
        ToolSpec::Function(ResponsesApiTool { parameters, .. }) => parameters,
        other => panic!("expected function tool, got {other:?}"),
    };
    let serialized = serde_json::to_value(parameters).expect("schema");
    let properties = serialized
        .get("properties")
        .expect("properties")
        .as_object()
        .expect("object");
    assert!(
        properties.contains_key("environment_id"),
        "apply_patch tool schema missing environment_id property"
    );
    assert_eq!(properties["environment_id"]["type"], "string");
    let required = serialized
        .get("required")
        .expect("required")
        .as_array()
        .expect("array");
    assert!(!required.iter().any(|r| r == "environment_id"));
}
