use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use crate::local_tool::CommandToolOptions;
use crate::local_tool::create_approval_parameters;
use crate::local_tool::unified_exec_output_schema;
use std::collections::BTreeMap;

/// Builds the env-aware mirror of `exec_command`. The native `exec_command`
/// tool stays byte-identical to upstream so the model sees its training-time
/// schema; this parallel tool prepends a required `environment_id` field that
/// routes the call to a non-default execution environment.
///
/// See spec § Pa.1.
pub fn create_exec_command_in_environment_tool(options: CommandToolOptions) -> ToolSpec {
    let mut properties = BTreeMap::from([
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Required. Identifier of the execution environment to run this command in. \
                 See <environments> in the system prompt for available ids. Use \
                 `list_environments` to refresh the catalog at runtime."
                    .to_string(),
            )),
        ),
        (
            "cmd".to_string(),
            JsonSchema::string(Some("Shell command to execute.".to_string())),
        ),
        (
            "workdir".to_string(),
            JsonSchema::string(Some(
                "Optional working directory to run the command in; defaults to the environment's cwd."
                    .to_string(),
            )),
        ),
        (
            "shell".to_string(),
            JsonSchema::string(Some(
                "Shell binary to launch. Defaults to the user's default shell.".to_string(),
            )),
        ),
        (
            "tty".to_string(),
            JsonSchema::boolean(Some(
                "Whether to allocate a TTY for the command. Defaults to false (plain pipes); set to true to open a PTY and access TTY process."
                    .to_string(),
            )),
        ),
        (
            "yield_time_ms".to_string(),
            JsonSchema::number(Some(
                "How long to wait (in milliseconds) for output before yielding.".to_string(),
            )),
        ),
        (
            "max_output_tokens".to_string(),
            JsonSchema::number(Some(
                "Maximum number of tokens to return. Excess output will be truncated.".to_string(),
            )),
        ),
    ]);
    if options.allow_login_shell {
        properties.insert(
            "login".to_string(),
            JsonSchema::boolean(Some(
                "Whether to run the shell with -l/-i semantics. Defaults to true.".to_string(),
            )),
        );
    }
    properties.extend(create_approval_parameters(
        options.exec_permission_approvals_enabled,
    ));

    ToolSpec::Function(ResponsesApiTool {
        name: "exec_command_in_environment".to_string(),
        description: "Runs a command in a PTY on the named execution environment. Mirrors `exec_command` but routes to a non-default environment via `environment_id`. Returns output or a session ID for ongoing interaction (use `write_stdin` to continue a session — `write_stdin` inherits the environment from the session it targets).".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["environment_id".to_string(), "cmd".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(unified_exec_output_schema()),
    })
}

#[cfg(test)]
#[path = "exec_command_in_environment_tool_tests.rs"]
mod tests;
