use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListDirToolOptions {
    pub has_multiple_environments: bool,
}

pub fn create_list_dir_tool() -> ToolSpec {
    create_list_dir_tool_with_options(ListDirToolOptions::default())
}

pub fn create_list_dir_tool_with_options(options: ListDirToolOptions) -> ToolSpec {
    let dir_path_description = if options.has_multiple_environments {
        "Path to the directory to list. Plain relative paths resolve against the selected environment's current working directory, and env-qualified paths use oai_env://<environment_id>/<absolute-path>."
    } else {
        "Absolute path to the directory to list."
    };
    let mut properties = BTreeMap::from([
        (
            "dir_path".to_string(),
            JsonSchema::string(Some(dir_path_description.to_string())),
        ),
        (
            "offset".to_string(),
            JsonSchema::number(Some(
                "The entry number to start listing from. Must be 1 or greater.".to_string(),
            )),
        ),
        (
            "limit".to_string(),
            JsonSchema::number(Some("The maximum number of entries to return.".to_string())),
        ),
        (
            "depth".to_string(),
            JsonSchema::number(Some(
                "The maximum directory depth to traverse. Must be 1 or greater.".to_string(),
            )),
        ),
    ]);
    if options.has_multiple_environments {
        properties.insert(
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Optional selected environment id. Omit to use the primary environment. If dir_path uses oai_env://<environment_id>/<absolute-path>, this value must match.".to_string(),
            )),
        );
    }

    ToolSpec::Function(ResponsesApiTool {
        name: "list_dir".to_string(),
        description:
            "Lists entries in a local directory with 1-indexed entry numbers and simple type labels."
                .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, Some(vec!["dir_path".to_string()]), Some(false.into())),
        output_schema: None,
    })
}

pub fn create_test_sync_tool() -> ToolSpec {
    let barrier_properties = BTreeMap::from([
        (
            "id".to_string(),
            JsonSchema::string(Some(
                "Identifier shared by concurrent calls that should rendezvous".to_string(),
            )),
        ),
        (
            "participants".to_string(),
            JsonSchema::number(Some(
                "Number of tool calls that must arrive before the barrier opens".to_string(),
            )),
        ),
        (
            "timeout_ms".to_string(),
            JsonSchema::number(Some(
                "Maximum time in milliseconds to wait at the barrier".to_string(),
            )),
        ),
    ]);

    let properties = BTreeMap::from([
        (
            "sleep_before_ms".to_string(),
            JsonSchema::number(Some(
                "Optional delay in milliseconds before any other action".to_string(),
            )),
        ),
        (
            "sleep_after_ms".to_string(),
            JsonSchema::number(Some(
                "Optional delay in milliseconds after completing the barrier".to_string(),
            )),
        ),
        (
            "barrier".to_string(),
            JsonSchema::object(
                barrier_properties,
                Some(vec!["id".to_string(), "participants".to_string()]),
                Some(false.into()),
            ),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "test_sync_tool".to_string(),
        description: "Internal synchronization helper used by Codex integration tests.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, /*required*/ None, Some(false.into())),
        output_schema: None,
    })
}

#[cfg(test)]
#[path = "utility_tool_tests.rs"]
mod tests;
