use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn list_dir_in_environment_tool_matches_expected_spec() {
    let tool = create_list_dir_in_environment_tool();

    let description = "Lists the contents of a directory on the named execution environment's filesystem. Mirrors `list_dir` but routes the read to a non-default environment via `environment_id`. Returns a shallow listing with one entry per line, suffixed with `/` for directories.".to_string();

    let properties = BTreeMap::from([
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Required. Identifier of the execution environment whose filesystem to list. \
                 See <environments> in the system prompt for available ids. Use \
                 `list_environments` to refresh the catalog at runtime."
                    .to_string(),
            )),
        ),
        (
            "path".to_string(),
            JsonSchema::string(Some(
                "Required. Absolute path of the directory to list on the named environment's \
                 filesystem."
                    .to_string(),
            )),
        ),
    ]);

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "list_dir_in_environment".to_string(),
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
fn list_dir_in_environment_tool_requires_environment_id_and_path() {
    let tool = create_list_dir_in_environment_tool();
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
