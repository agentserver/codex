use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn apply_patch_in_environment_tool_matches_expected_spec() {
    let tool = create_apply_patch_in_environment_tool();

    let description = "Applies a patch to the named execution environment's filesystem. Mirrors the JSON variant of `apply_patch` but routes to a non-default environment via `environment_id`. Use `apply_patch` directly when targeting the default environment.".to_string();

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

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "apply_patch_in_environment".to_string(),
            description,
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["environment_id".to_string(), "input".to_string()]),
                Some(false.into()),
            ),
            output_schema: None,
        })
    );
}

#[test]
fn apply_patch_in_environment_tool_requires_environment_id_and_input() {
    let tool = create_apply_patch_in_environment_tool();
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = tool else {
        panic!("expected function tool");
    };

    let required = parameters.required.as_ref().expect("required list");
    assert_eq!(
        required,
        &vec!["environment_id".to_string(), "input".to_string()]
    );

    let properties = parameters
        .properties
        .as_ref()
        .expect("object properties present");
    // Only env_id and input — no other fields.
    let keys: Vec<&str> = properties.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["environment_id", "input"]);
}
