use std::collections::HashMap;
use std::collections::HashSet;

use codex_features::Feature;
use codex_mcp::CODEX_APPS_MCP_SERVER_NAME;
use codex_mcp::ToolInfo as McpToolInfo;
use codex_mcp::filter_non_codex_apps_mcp_tools_only;
use codex_tools::ToolsConfig;

use crate::config::Config;
use crate::connectors;
use crate::tools::mcp_tool_input::McpToolInput;

pub(crate) const DIRECT_MCP_TOOL_EXPOSURE_THRESHOLD: usize = 100;

pub(crate) struct McpToolExposure {
    pub(crate) tools: HashMap<String, McpToolInput>,
}

pub(crate) fn build_mcp_tool_exposure(
    all_mcp_tools: &HashMap<String, McpToolInfo>,
    connectors: Option<&[connectors::AppInfo]>,
    explicitly_enabled_connectors: &[connectors::AppInfo],
    config: &Config,
    tools_config: &ToolsConfig,
) -> McpToolExposure {
    let mut candidate_tools = filter_non_codex_apps_mcp_tools_only(all_mcp_tools);
    if let Some(connectors) = connectors {
        candidate_tools.extend(filter_codex_apps_mcp_tools(
            all_mcp_tools,
            connectors,
            config,
        ));
    }

    let should_defer = tools_config.search_tool
        && (config
            .features
            .enabled(Feature::ToolSearchAlwaysDeferMcpTools)
            || candidate_tools.len() >= DIRECT_MCP_TOOL_EXPOSURE_THRESHOLD);

    if !should_defer {
        return McpToolExposure {
            tools: candidate_tools
                .into_iter()
                .map(|(name, tool_info)| {
                    (
                        name,
                        McpToolInput {
                            tool_info,
                            defer_loading: false,
                        },
                    )
                })
                .collect(),
        };
    }

    let direct_tools =
        filter_codex_apps_mcp_tools(all_mcp_tools, explicitly_enabled_connectors, config);
    let mut tools = HashMap::new();
    for (name, tool_info) in direct_tools {
        candidate_tools.remove(&name);
        tools.insert(
            name,
            McpToolInput {
                tool_info,
                defer_loading: false,
            },
        );
    }
    for (name, tool_info) in candidate_tools {
        tools.insert(
            name,
            McpToolInput {
                tool_info,
                defer_loading: true,
            },
        );
    }

    McpToolExposure { tools }
}

fn filter_codex_apps_mcp_tools(
    mcp_tools: &HashMap<String, McpToolInfo>,
    connectors: &[connectors::AppInfo],
    config: &Config,
) -> HashMap<String, McpToolInfo> {
    let allowed: HashSet<&str> = connectors
        .iter()
        .map(|connector| connector.id.as_str())
        .collect();

    mcp_tools
        .iter()
        .filter(|(_, tool)| {
            if tool.server_name != CODEX_APPS_MCP_SERVER_NAME {
                return false;
            }
            let Some(connector_id) = tool.connector_id.as_deref() else {
                return false;
            };
            allowed.contains(connector_id) && connectors::codex_app_tool_is_enabled(config, tool)
        })
        .map(|(name, tool)| (name.clone(), tool.clone()))
        .collect()
}

#[cfg(test)]
#[path = "mcp_tool_exposure_test.rs"]
mod tests;
