use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn read_file_in_environment_tool_matches_expected_spec() {
    let tool = create_read_file_in_environment_tool();

    let description = "Reads a file from the named environment's filesystem. Returns text content (UTF-8 decoded). For non-text files use `view_image_in_environment` (images) or `exec_command_in_environment` (binary tooling). Optional `byte_range` reads a slice; omitted = full file.".to_string();

    let properties = BTreeMap::from([
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Required. Identifier of the execution environment whose filesystem to read from. \
                 See <environments> in the system prompt for available ids."
                    .to_string(),
            )),
        ),
        (
            "path".to_string(),
            JsonSchema::string(Some(
                "Required. Absolute path of the file to read on the named environment."
                    .to_string(),
            )),
        ),
        (
            "byte_range".to_string(),
            JsonSchema::object(
                BTreeMap::from([
                    (
                        "start".to_string(),
                        JsonSchema::number(Some("Inclusive byte offset".to_string())),
                    ),
                    (
                        "end".to_string(),
                        JsonSchema::number(Some("Exclusive byte offset".to_string())),
                    ),
                ]),
                Some(vec!["start".to_string(), "end".to_string()]),
                Some(false.into()),
            ),
        ),
    ]);

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "read_file_in_environment".to_string(),
            description,
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec![
                    "environment_id".to_string(),
                    "path".to_string(),
                ]),
                Some(false.into()),
            ),
            output_schema: None,
        })
    );
}

#[test]
fn read_file_in_environment_tool_requires_environment_id_and_path_only() {
    // `byte_range` is intentionally optional: the LLM may omit it to read
    // the full file. Pin the required list to guard against accidental
    // inclusion.
    let tool = create_read_file_in_environment_tool();
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
    assert_eq!(keys, vec!["byte_range", "environment_id", "path"]);
}
