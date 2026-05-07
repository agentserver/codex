use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

/// Builds the env-aware mirror of a file read primitive. Unlike
/// `list_dir_in_environment` (which mirrors a native upstream tool of the
/// same name), there is no native `read_file` tool to mirror — the LLM
/// historically reads files via `shell` / `cat`. This tool exposes a
/// dedicated read path so the LLM can fetch a file's content from a
/// non-default environment without spawning a process. See spec § Pa.6.
///
/// The schema exposes `environment_id` + `path` (both required) plus an
/// optional `byte_range` slice. Non-text files surface a clear error; the
/// LLM should use `view_image_in_environment` for images and
/// `exec_command_in_environment` for binary tooling.
pub fn create_read_file_in_environment_tool() -> ToolSpec {
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

    ToolSpec::Function(ResponsesApiTool {
        name: "read_file_in_environment".to_string(),
        description: "Reads a file from the named environment's filesystem. Returns text content (UTF-8 decoded). For non-text files use `view_image_in_environment` (images) or `exec_command_in_environment` (binary tooling). Optional `byte_range` reads a slice; omitted = full file.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["environment_id".to_string(), "path".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

#[cfg(test)]
#[path = "read_file_in_environment_tool_tests.rs"]
mod tests;
