use crate::history::is_user_turn_boundary;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_function_output_items_with_policy;
use codex_utils_output_truncation::truncate_text;

/// Truncates one history item before it is recorded in the in-memory journal.
///
/// Tool outputs receive a small serialization headroom multiplier so the JSON wrapper bytes do not
/// cause unexpected overages after truncation.
pub fn truncate_history_item(item: &ResponseItem, policy: TruncationPolicy) -> ResponseItem {
    let policy_with_serialization_budget = policy * 1.2;
    match item {
        ResponseItem::FunctionCallOutput { call_id, output } => ResponseItem::FunctionCallOutput {
            call_id: call_id.clone(),
            output: truncate_function_output_payload(output, policy_with_serialization_budget),
        },
        ResponseItem::CustomToolCallOutput {
            call_id,
            name,
            output,
        } => ResponseItem::CustomToolCallOutput {
            call_id: call_id.clone(),
            name: name.clone(),
            output: truncate_function_output_payload(output, policy_with_serialization_budget),
        },
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::Other => item.clone(),
    }
}

/// Replaces image content in the last tool-output item of the current turn.
pub fn replace_last_turn_images(items: &mut [ResponseItem], placeholder: &str) -> bool {
    let Some(index) = items.iter().rposition(|item| {
        matches!(item, ResponseItem::FunctionCallOutput { .. }) || is_user_turn_boundary(item)
    }) else {
        return false;
    };

    match &mut items[index] {
        ResponseItem::FunctionCallOutput { output, .. } => {
            let Some(content_items) = output.content_items_mut() else {
                return false;
            };

            let mut replaced = false;
            let placeholder = placeholder.to_string();
            for item in content_items.iter_mut() {
                if matches!(item, FunctionCallOutputContentItem::InputImage { .. }) {
                    *item = FunctionCallOutputContentItem::InputText {
                        text: placeholder.clone(),
                    };
                    replaced = true;
                }
            }
            replaced
        }
        ResponseItem::Message { .. } => false,
        _ => false,
    }
}

fn truncate_function_output_payload(
    output: &FunctionCallOutputPayload,
    policy: TruncationPolicy,
) -> FunctionCallOutputPayload {
    let body = match &output.body {
        FunctionCallOutputBody::Text(content) => {
            FunctionCallOutputBody::Text(truncate_text(content, policy))
        }
        FunctionCallOutputBody::ContentItems(items) => FunctionCallOutputBody::ContentItems(
            truncate_function_output_items_with_policy(items, policy),
        ),
    };

    FunctionCallOutputPayload {
        body,
        success: output.success,
    }
}
