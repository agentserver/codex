use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

/// Builds the env-aware mirror of `list_dir`. The native `list_dir` tool
/// stays byte-identical to upstream so the model sees its training-time
/// schema; this parallel tool prepends a required `environment_id` field
/// that routes the directory listing to a non-default execution
/// environment's filesystem.
///
/// Unlike `list_dir`, the env-aware variant intentionally exposes only
/// `environment_id` + `path`. The optional pagination/depth knobs from
/// `list_dir` rely on `tokio::fs` semantics that aren't part of the
/// `ExecutorFileSystem::read_directory` contract today; the listing is
/// shallow (no recursion) and unbounded. Adding pagination/depth at this
/// layer would require either widening the `ExecutorFileSystem` trait or
/// shoehorning post-filtering on top of the remote response, which is
/// deferred until a concrete need arises (see spec § Pa.4).
///
/// See spec § Pa.4.
pub fn create_list_dir_in_environment_tool() -> ToolSpec {
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

    ToolSpec::Function(ResponsesApiTool {
        name: "list_dir_in_environment".to_string(),
        description: "Lists the contents of a directory on the named execution environment's filesystem. Mirrors `list_dir` but routes the read to a non-default environment via `environment_id`. Returns a shallow listing with one entry per line, suffixed with `/` for directories.".to_string(),
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
#[path = "list_dir_in_environment_tool_tests.rs"]
mod tests;
