use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

/// Builds the env-aware write counterpart to `read_file_in_environment`.
/// Like its read sibling, this tool has no native upstream mirror; codex's
/// historical "write a whole file" path is `apply_patch` with an `Add
/// File` hunk. This tool exposes a simpler one-shot write so the LLM
/// doesn't have to assemble a patch envelope to drop a fresh file onto a
/// non-default environment. See spec § Pa.6.
///
/// `create_dirs` is opt-in (default `false`) so a typo in the parent path
/// surfaces as an error instead of silently materializing a deep
/// directory tree.
pub fn create_write_file_in_environment_tool() -> ToolSpec {
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

    ToolSpec::Function(ResponsesApiTool {
        name: "write_file_in_environment".to_string(),
        description: "Writes text content to a file on the named environment's filesystem. Replaces the entire file. For incremental edits use `apply_patch_in_environment`. Use this for one-shot file creation or full rewrites.".to_string(),
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
}

#[cfg(test)]
#[path = "write_file_in_environment_tool_tests.rs"]
mod tests;
