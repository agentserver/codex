use super::*;
use crate::local_tool::create_approval_parameters;
use crate::local_tool::unified_exec_output_schema;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn exec_command_in_environment_tool_matches_expected_spec() {
    let tool = create_exec_command_in_environment_tool(CommandToolOptions {
        allow_login_shell: true,
        exec_permission_approvals_enabled: false,
    });

    let description = "Runs a command in a PTY on the named execution environment. Mirrors `exec_command` but routes to a non-default environment via `environment_id`. Returns output or a session ID for ongoing interaction (use `write_stdin` to continue a session — `write_stdin` inherits the environment from the session it targets).".to_string();

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
        (
            "login".to_string(),
            JsonSchema::boolean(Some(
                "Whether to run the shell with -l/-i semantics. Defaults to true.".to_string(),
            )),
        ),
    ]);
    properties.extend(create_approval_parameters(
        /*exec_permission_approvals_enabled*/ false,
    ));

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "exec_command_in_environment".to_string(),
            description,
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["environment_id".to_string(), "cmd".to_string()]),
                Some(false.into()),
            ),
            output_schema: Some(unified_exec_output_schema()),
        })
    );
}

#[test]
fn exec_command_in_environment_tool_omits_login_when_disallowed() {
    let tool = create_exec_command_in_environment_tool(CommandToolOptions {
        allow_login_shell: false,
        exec_permission_approvals_enabled: false,
    });

    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = tool else {
        panic!("expected function tool");
    };

    let properties = parameters
        .properties
        .as_ref()
        .expect("object properties present");
    assert!(!properties.contains_key("login"));
    let required = parameters.required.as_ref().expect("required list");
    assert_eq!(
        required,
        &vec!["environment_id".to_string(), "cmd".to_string()]
    );
}

#[test]
fn exec_command_in_environment_tool_includes_additional_permissions_when_enabled() {
    let tool = create_exec_command_in_environment_tool(CommandToolOptions {
        allow_login_shell: true,
        exec_permission_approvals_enabled: true,
    });

    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = tool else {
        panic!("expected function tool");
    };

    let properties = parameters
        .properties
        .as_ref()
        .expect("object properties present");
    assert!(properties.contains_key("additional_permissions"));
    assert!(properties.contains_key("environment_id"));
}
