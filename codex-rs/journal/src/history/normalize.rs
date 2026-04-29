use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::InputModality;
use tracing::error;
use tracing::info;

const IMAGE_CONTENT_OMITTED_PLACEHOLDER: &str =
    "image content omitted because you do not support image input";

/// Ensures every tool-call item has a matching output item, inserting synthetic aborted outputs
/// immediately after calls that are missing one.
pub fn ensure_call_outputs_present(items: &mut Vec<ResponseItem>) {
    let mut missing_outputs_to_insert: Vec<(usize, ResponseItem)> = Vec::new();

    for (index, item) in items.iter().enumerate() {
        match item {
            ResponseItem::FunctionCall { call_id, .. } => {
                let has_output = items.iter().any(|other| match other {
                    ResponseItem::FunctionCallOutput {
                        call_id: existing, ..
                    } => existing == call_id,
                    _ => false,
                });

                if !has_output {
                    info!("Function call output is missing for call id: {call_id}");
                    missing_outputs_to_insert.push((
                        index,
                        ResponseItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                        },
                    ));
                }
            }
            ResponseItem::ToolSearchCall {
                call_id: Some(call_id),
                ..
            } => {
                let has_output = items.iter().any(|other| match other {
                    ResponseItem::ToolSearchOutput {
                        call_id: Some(existing),
                        ..
                    } => existing == call_id,
                    _ => false,
                });

                if !has_output {
                    info!("Tool search output is missing for call id: {call_id}");
                    missing_outputs_to_insert.push((
                        index,
                        ResponseItem::ToolSearchOutput {
                            call_id: Some(call_id.clone()),
                            status: "completed".to_string(),
                            execution: "client".to_string(),
                            tools: Vec::new(),
                        },
                    ));
                }
            }
            ResponseItem::CustomToolCall { call_id, .. } => {
                let has_output = items.iter().any(|other| match other {
                    ResponseItem::CustomToolCallOutput {
                        call_id: existing, ..
                    } => existing == call_id,
                    _ => false,
                });

                if !has_output {
                    report_invariant_violation(format!(
                        "Custom tool call output is missing for call id: {call_id}"
                    ));
                    missing_outputs_to_insert.push((
                        index,
                        ResponseItem::CustomToolCallOutput {
                            call_id: call_id.clone(),
                            name: None,
                            output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                        },
                    ));
                }
            }
            ResponseItem::LocalShellCall { call_id, .. } => {
                if let Some(call_id) = call_id.as_ref() {
                    let has_output = items.iter().any(|other| match other {
                        ResponseItem::FunctionCallOutput {
                            call_id: existing, ..
                        } => existing == call_id,
                        _ => false,
                    });

                    if !has_output {
                        report_invariant_violation(format!(
                            "Local shell call output is missing for call id: {call_id}"
                        ));
                        missing_outputs_to_insert.push((
                            index,
                            ResponseItem::FunctionCallOutput {
                                call_id: call_id.clone(),
                                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                            },
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    for (index, output_item) in missing_outputs_to_insert.into_iter().rev() {
        items.insert(index + 1, output_item);
    }
}

/// Removes output items whose corresponding tool-call items no longer exist.
pub fn remove_orphan_outputs(items: &mut Vec<ResponseItem>) {
    let function_call_ids = items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::FunctionCall { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();

    let tool_search_call_ids = items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::ToolSearchCall {
                call_id: Some(call_id),
                ..
            } => Some(call_id.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();

    let local_shell_call_ids = items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::LocalShellCall {
                call_id: Some(call_id),
                ..
            } => Some(call_id.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();

    let custom_tool_call_ids = items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::CustomToolCall { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();

    items.retain(|item| match item {
        ResponseItem::FunctionCallOutput { call_id, .. } => {
            let has_match =
                function_call_ids.contains(call_id) || local_shell_call_ids.contains(call_id);
            if !has_match {
                report_invariant_violation(format!(
                    "Orphan function call output for call id: {call_id}"
                ));
            }
            has_match
        }
        ResponseItem::CustomToolCallOutput { call_id, .. } => {
            let has_match = custom_tool_call_ids.contains(call_id);
            if !has_match {
                report_invariant_violation(format!(
                    "Orphan custom tool call output for call id: {call_id}"
                ));
            }
            has_match
        }
        ResponseItem::ToolSearchOutput { execution, .. } if execution == "server" => true,
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            ..
        } => {
            let has_match = tool_search_call_ids.contains(call_id);
            if !has_match {
                report_invariant_violation(format!(
                    "Orphan tool search output for call id: {call_id}"
                ));
            }
            has_match
        }
        ResponseItem::ToolSearchOutput { call_id: None, .. } => true,
        _ => true,
    });
}

/// Removes the matching paired item for `item` from the history, if one exists.
pub fn remove_corresponding_for(items: &mut Vec<ResponseItem>, item: &ResponseItem) {
    match item {
        ResponseItem::FunctionCall { call_id, .. } => {
            remove_first_matching(items, |other| {
                matches!(
                    other,
                    ResponseItem::FunctionCallOutput {
                        call_id: existing, ..
                    } if existing == call_id
                )
            });
        }
        ResponseItem::FunctionCallOutput { call_id, .. } => {
            if let Some(position) = items.iter().position(|other| {
                matches!(
                    other,
                    ResponseItem::FunctionCall {
                        call_id: existing,
                        ..
                    } if existing == call_id
                )
            }) {
                items.remove(position);
            } else if let Some(position) = items.iter().position(|other| {
                matches!(
                    other,
                    ResponseItem::LocalShellCall {
                        call_id: Some(existing),
                        ..
                    } if existing == call_id
                )
            }) {
                items.remove(position);
            }
        }
        ResponseItem::ToolSearchCall {
            call_id: Some(call_id),
            ..
        } => {
            remove_first_matching(items, |other| {
                matches!(
                    other,
                    ResponseItem::ToolSearchOutput {
                        call_id: Some(existing),
                        ..
                    } if existing == call_id
                )
            });
        }
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            ..
        } => {
            remove_first_matching(items, |other| {
                matches!(
                    other,
                    ResponseItem::ToolSearchCall {
                        call_id: Some(existing),
                        ..
                    } if existing == call_id
                )
            });
        }
        ResponseItem::CustomToolCall { call_id, .. } => {
            remove_first_matching(items, |other| {
                matches!(
                    other,
                    ResponseItem::CustomToolCallOutput {
                        call_id: existing, ..
                    } if existing == call_id
                )
            });
        }
        ResponseItem::CustomToolCallOutput { call_id, .. } => {
            remove_first_matching(items, |other| {
                matches!(
                    other,
                    ResponseItem::CustomToolCall {
                        call_id: existing,
                        ..
                    } if existing == call_id
                )
            });
        }
        ResponseItem::LocalShellCall {
            call_id: Some(call_id),
            ..
        } => {
            remove_first_matching(items, |other| {
                matches!(
                    other,
                    ResponseItem::FunctionCallOutput {
                        call_id: existing, ..
                    } if existing == call_id
                )
            });
        }
        _ => {}
    }
}

/// Strips image inputs from messages and tool outputs when the model does not support images.
///
/// Image-generation call results are cleared in the same mode so text-only prompts do not carry
/// image payloads forward.
pub fn strip_images_when_unsupported(
    input_modalities: &[InputModality],
    items: &mut [ResponseItem],
) {
    if input_modalities.contains(&InputModality::Image) {
        return;
    }

    for item in items.iter_mut() {
        match item {
            ResponseItem::Message { content, .. } => {
                let mut normalized_content = Vec::with_capacity(content.len());
                for content_item in content.iter() {
                    match content_item {
                        ContentItem::InputImage { .. } => {
                            normalized_content.push(ContentItem::InputText {
                                text: IMAGE_CONTENT_OMITTED_PLACEHOLDER.to_string(),
                            });
                        }
                        _ => normalized_content.push(content_item.clone()),
                    }
                }
                *content = normalized_content;
            }
            ResponseItem::FunctionCallOutput { output, .. }
            | ResponseItem::CustomToolCallOutput { output, .. } => {
                if let Some(content_items) = output.content_items_mut() {
                    let mut normalized_content_items = Vec::with_capacity(content_items.len());
                    for content_item in content_items.iter() {
                        match content_item {
                            FunctionCallOutputContentItem::InputImage { .. } => {
                                normalized_content_items.push(
                                    FunctionCallOutputContentItem::InputText {
                                        text: IMAGE_CONTENT_OMITTED_PLACEHOLDER.to_string(),
                                    },
                                );
                            }
                            _ => normalized_content_items.push(content_item.clone()),
                        }
                    }
                    *content_items = normalized_content_items;
                }
            }
            ResponseItem::ImageGenerationCall { result, .. } => {
                result.clear();
            }
            _ => {}
        }
    }
}

fn remove_first_matching<F>(items: &mut Vec<ResponseItem>, predicate: F)
where
    F: Fn(&ResponseItem) -> bool,
{
    if let Some(position) = items.iter().position(predicate) {
        items.remove(position);
    }
}

fn report_invariant_violation(message: String) {
    if cfg!(debug_assertions) {
        panic!("{message}");
    } else {
        error!("{message}");
    }
}
