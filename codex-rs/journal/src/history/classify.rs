use codex_protocol::items::parse_hook_prompt_fragment;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::ENVIRONMENT_CONTEXT_CLOSE_TAG;
use codex_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;
use codex_protocol::protocol::InterAgentCommunication;

const USER_INSTRUCTIONS_START_MARKER: &str = "# AGENTS.md instructions for ";
const USER_INSTRUCTIONS_END_MARKER: &str = "</INSTRUCTIONS>";
const SKILL_START_MARKER: &str = "<skill>";
const SKILL_END_MARKER: &str = "</skill>";
const USER_SHELL_COMMAND_START_MARKER: &str = "<user_shell_command>";
const USER_SHELL_COMMAND_END_MARKER: &str = "</user_shell_command>";
const TURN_ABORTED_START_MARKER: &str = "<turn_aborted>";
const TURN_ABORTED_END_MARKER: &str = "</turn_aborted>";
const SUBAGENT_NOTIFICATION_START_MARKER: &str = "<subagent_notification>";
const SUBAGENT_NOTIFICATION_END_MARKER: &str = "</subagent_notification>";

/// Returns whether an item should be carried in API-visible conversation history.
pub fn is_api_message(message: &ResponseItem) -> bool {
    match message {
        ResponseItem::Message { role, .. } => role.as_str() != "system",
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. } => true,
        ResponseItem::Other => false,
    }
}

/// Returns whether an item originated from model generation rather than client-side bookkeeping.
pub fn is_model_generated_item(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, .. } => role == "assistant",
        ResponseItem::Reasoning { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::Compaction { .. } => true,
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::Other => false,
    }
}

/// Returns whether an item was injected by Codex rather than supplied by the user or model.
pub fn is_codex_generated_item(item: &ResponseItem) -> bool {
    matches!(
        item,
        ResponseItem::FunctionCallOutput { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::CustomToolCallOutput { .. }
    ) || matches!(item, ResponseItem::Message { role, .. } if role == "developer")
}

/// Returns whether an item should count as an instruction-turn boundary for history rewrites.
pub fn is_user_turn_boundary(item: &ResponseItem) -> bool {
    let ResponseItem::Message { role, content, .. } = item else {
        return false;
    };

    (role == "user" && !is_contextual_user_message_content(content))
        || (role == "assistant" && is_inter_agent_instruction_content(content))
}

/// Returns the indexes of all instruction-turn boundaries in order.
pub fn user_turn_boundary_positions(items: &[ResponseItem]) -> Vec<usize> {
    let mut positions = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if is_user_turn_boundary(item) {
            positions.push(index);
        }
    }
    positions
}

fn is_inter_agent_instruction_content(content: &[ContentItem]) -> bool {
    InterAgentCommunication::is_message_content(content)
}

fn is_contextual_user_message_content(content: &[ContentItem]) -> bool {
    content.iter().any(|item| match item {
        ContentItem::InputText { text } => {
            parse_hook_prompt_fragment(text).is_some()
                || matches_fragment(
                    text,
                    USER_INSTRUCTIONS_START_MARKER,
                    USER_INSTRUCTIONS_END_MARKER,
                )
                || matches_fragment(
                    text,
                    ENVIRONMENT_CONTEXT_OPEN_TAG,
                    ENVIRONMENT_CONTEXT_CLOSE_TAG,
                )
                || matches_fragment(text, SKILL_START_MARKER, SKILL_END_MARKER)
                || matches_fragment(
                    text,
                    USER_SHELL_COMMAND_START_MARKER,
                    USER_SHELL_COMMAND_END_MARKER,
                )
                || matches_fragment(text, TURN_ABORTED_START_MARKER, TURN_ABORTED_END_MARKER)
                || matches_fragment(
                    text,
                    SUBAGENT_NOTIFICATION_START_MARKER,
                    SUBAGENT_NOTIFICATION_END_MARKER,
                )
        }
        ContentItem::InputImage { .. } | ContentItem::OutputText { .. } => false,
    })
}

fn matches_fragment(text: &str, start_marker: &str, end_marker: &str) -> bool {
    let trimmed = text.trim_start();
    let starts_with_marker = trimmed
        .get(..start_marker.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(start_marker));
    let trimmed = trimmed.trim_end();
    let ends_with_marker = trimmed
        .get(trimmed.len().saturating_sub(end_marker.len())..)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(end_marker));
    starts_with_marker && ends_with_marker
}
