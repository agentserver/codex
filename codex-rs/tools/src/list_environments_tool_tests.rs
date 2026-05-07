use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn list_environments_tool_matches_expected_spec() {
    let tool = create_list_environments_tool();

    let description = "Returns the catalog of available execution environments for this turn. Each entry includes id, description, and whether it is the default. Use this to discover env ids for the `*_in_environment` tool family. The system prompt's <environments> block contains the same information at turn start; this tool refreshes the catalog (e.g., to check status changes mid-turn).".to_string();

    let properties = BTreeMap::from([(
        "include_status".to_string(),
        JsonSchema::boolean(Some(
            "Optional. When true, include online/offline status for each environment by \
             pinging its bridge endpoint. Adds latency proportional to environment count. \
             Defaults to false (returns the static catalog only)."
                .to_string(),
        )),
    )]);

    assert_eq!(
        tool,
        ToolSpec::Function(ResponsesApiTool {
            name: "list_environments".to_string(),
            description,
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec![]),
                Some(false.into()),
            ),
            output_schema: Some(list_environments_output_schema()),
        })
    );
}

#[test]
fn list_environments_tool_has_no_required_fields() {
    let tool = create_list_environments_tool();
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = tool else {
        panic!("expected function tool");
    };

    let required = parameters.required.as_ref().expect("required list");
    assert!(
        required.is_empty(),
        "list_environments takes no required fields: got {required:?}"
    );

    let properties = parameters
        .properties
        .as_ref()
        .expect("object properties present");
    let keys: Vec<&str> = properties.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["include_status"]);
}

#[test]
fn list_environments_tool_provides_output_schema_with_required_id_and_is_default() {
    let tool = create_list_environments_tool();
    let ToolSpec::Function(ResponsesApiTool { output_schema, .. }) = tool else {
        panic!("expected function tool");
    };

    let output_schema = output_schema.expect("output_schema present");
    let envs_schema = output_schema
        .get("properties")
        .and_then(|properties| properties.get("environments"))
        .expect("environments property");
    let item_schema = envs_schema.get("items").expect("items present");
    let required = item_schema
        .get("required")
        .and_then(|r| r.as_array())
        .expect("required list on item");
    let required: Vec<&str> = required
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(required, vec!["id", "is_default"]);

    let item_props = item_schema
        .get("properties")
        .and_then(|p| p.as_object())
        .expect("item properties object");
    let mut keys: Vec<&str> = item_props.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["description", "id", "is_default", "online"]);
}
