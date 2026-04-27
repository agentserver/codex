use codex_mcp::ToolInfo;

#[derive(Clone, Debug)]
pub(crate) struct McpToolInput {
    pub(crate) tool_info: ToolInfo,
    pub(crate) defer_loading: bool,
}
