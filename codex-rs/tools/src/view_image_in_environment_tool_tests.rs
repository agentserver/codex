use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn view_image_in_environment_tool_matches_expected_spec() {
    let tool = create_view_image_in_environment_tool();

    let description = "Loads an image from the named execution environment's filesystem and attaches it to the conversation as an image input. Mirrors `view_image` but routes the read to a non-default environment via `environment_id`. Always returns the default resized representation; the `detail = original` override exposed by `view_image` is not available here.".to_string();

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

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "view_image_in_environment".to_string(),
            description,
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["environment_id".to_string(), "path".to_string()]),
                Some(false.into()),
            ),
            output_schema: Some(view_image_in_environment_output_schema()),
        })
    );
}

#[test]
fn view_image_in_environment_tool_requires_environment_id_and_path() {
    let tool = create_view_image_in_environment_tool();
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = tool else {
        panic!("expected function tool");
    };

    let required = parameters.required.as_ref().expect("required list");
    let mut required_sorted: Vec<&str> = required.iter().map(String::as_str).collect();
    required_sorted.sort();
    assert_eq!(required_sorted, vec!["environment_id", "path"]);

    let properties = parameters
        .properties
        .as_ref()
        .expect("object properties present");
    let mut keys: Vec<&str> = properties.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["environment_id", "path"]);
}

#[test]
fn output_schema_only_advertises_resized_detail() {
    // Pa.5 contract: the env-aware variant intentionally never returns
    // `detail = "original"` because the schema does not expose that knob.
    // Pin the description so a future schema rev that adds the override
    // can't silently leave this docstring stale.
    let schema = view_image_in_environment_output_schema();
    let detail_desc = schema["properties"]["detail"]["description"]
        .as_str()
        .expect("detail description present");
    assert!(
        detail_desc.contains("never `original`"),
        "detail description should pin the no-original contract; got: {detail_desc}"
    );
}

#[test]
fn view_image_in_environment_tool_name_is_stable() {
    let tool = create_view_image_in_environment_tool();
    let ToolSpec::Function(ResponsesApiTool { name, .. }) = tool else {
        panic!("expected function tool");
    };
    assert_eq!(name, "view_image_in_environment");
}
