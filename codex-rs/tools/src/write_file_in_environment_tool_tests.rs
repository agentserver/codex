use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn write_file_in_environment_tool_matches_expected_spec() {
    let tool = create_write_file_in_environment_tool();

    let description = "Writes text content to a file on the named environment's filesystem. Replaces the entire file. For incremental edits use `apply_patch_in_environment`. Use this for one-shot file creation or full rewrites.".to_string();

    let properties = BTreeMap::from([
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Required. Identifier of the execution environment whose filesystem to write to. \
                 See <environments> in the system prompt for available ids."
                    .to_string(),
            )),
        ),
        (
            "path".to_string(),
            JsonSchema::string(Some(
                "Required. Absolute path of the file to write on the named environment. \
                 Existing file is replaced; use `apply_patch_in_environment` for incremental edits."
                    .to_string(),
            )),
        ),
        (
            "content".to_string(),
            JsonSchema::string(Some(
                "Required. UTF-8 text content to write to the file. Replaces existing content if any."
                    .to_string(),
            )),
        ),
        (
            "create_dirs".to_string(),
            JsonSchema::boolean(Some(
                "Optional. If true, create missing parent directories. Defaults to false."
                    .to_string(),
            )),
        ),
    ]);

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "write_file_in_environment".to_string(),
            description,
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec![
                    "environment_id".to_string(),
                    "path".to_string(),
                    "content".to_string(),
                ]),
                Some(false.into()),
            ),
            output_schema: None,
        })
    );
}

#[test]
fn write_file_in_environment_tool_requires_env_path_and_content() {
    // `create_dirs` is intentionally optional and defaults to false on
    // the handler side. Pin the required list so a future edit doesn't
    // accidentally promote it to required.
    let tool = create_write_file_in_environment_tool();
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = tool else {
        panic!("expected function tool");
    };

    let required = parameters.required.as_ref().expect("required list");
    let mut required_sorted: Vec<&str> = required.iter().map(String::as_str).collect();
    required_sorted.sort();
    assert_eq!(
        required_sorted,
        vec!["content", "environment_id", "path"]
    );

    let properties = parameters
        .properties
        .as_ref()
        .expect("object properties present");
    let mut keys: Vec<&str> = properties.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["content", "create_dirs", "environment_id", "path"]
    );
}
