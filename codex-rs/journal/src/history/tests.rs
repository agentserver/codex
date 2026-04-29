use super::ensure_call_outputs_present;
use super::estimate_response_item_model_visible_bytes;
use super::is_user_turn_boundary;
use super::remove_corresponding_for;
use super::replace_last_turn_images;
use super::strip_images_when_unsupported;
use super::truncate_history_item;
use codex_protocol::AgentPath;
use codex_protocol::models::ContentItem;
use codex_protocol::models::DEFAULT_IMAGE_DETAIL;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::InputModality;
use codex_protocol::protocol::InterAgentCommunication;
use codex_utils_output_truncation::TruncationPolicy;
use pretty_assertions::assert_eq;

fn user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        phase: None,
    }
}

fn assistant_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        phase: None,
    }
}

#[test]
fn ensure_call_outputs_present_inserts_missing_function_output_after_call() {
    let mut items = vec![ResponseItem::FunctionCall {
        id: None,
        name: "shell".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: "call-1".to_string(),
    }];

    ensure_call_outputs_present(&mut items);

    assert_eq!(
        items,
        vec![
            ResponseItem::FunctionCall {
                id: None,
                name: "shell".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "call-1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
        ]
    );
}

#[test]
fn remove_corresponding_for_removes_matching_tool_output() {
    let removed = ResponseItem::FunctionCall {
        id: None,
        name: "shell".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: "call-1".to_string(),
    };
    let mut items = vec![
        removed.clone(),
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_text("done".to_string()),
        },
    ];

    items.remove(0);
    remove_corresponding_for(&mut items, &removed);

    assert_eq!(items, Vec::new());
}

#[test]
fn strip_images_when_unsupported_rewrites_messages_and_tool_outputs() {
    let mut items = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "look".to_string(),
                },
                ContentItem::InputImage {
                    image_url: "https://example.com/img.png".to_string(),
                    detail: Some(DEFAULT_IMAGE_DETAIL),
                },
            ],
            phase: None,
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputImage {
                    image_url: "https://example.com/tool.png".to_string(),
                    detail: Some(DEFAULT_IMAGE_DETAIL),
                },
            ]),
        },
        ResponseItem::ImageGenerationCall {
            id: "ig-1".to_string(),
            status: "completed".to_string(),
            revised_prompt: None,
            result: "Zm9v".to_string(),
        },
    ];

    strip_images_when_unsupported(&[InputModality::Text], &mut items);

    assert_eq!(
        items,
        vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![
                    ContentItem::InputText {
                        text: "look".to_string(),
                    },
                    ContentItem::InputText {
                        text: "image content omitted because you do not support image input"
                            .to_string(),
                    },
                ],
                phase: None,
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::InputText {
                        text: "image content omitted because you do not support image input"
                            .to_string(),
                    },
                ]),
            },
            ResponseItem::ImageGenerationCall {
                id: "ig-1".to_string(),
                status: "completed".to_string(),
                revised_prompt: None,
                result: String::new(),
            },
        ]
    );
}

#[test]
fn inter_agent_assistant_messages_are_turn_boundaries() {
    let communication = InterAgentCommunication::new(
        AgentPath::root(),
        AgentPath::root()
            .join("worker")
            .expect("sub-agent path should be valid"),
        Vec::new(),
        "continue".to_string(),
        /*trigger_turn*/ true,
    );
    let item = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: serde_json::to_string(&communication).expect("message should serialize"),
        }],
        phase: None,
    };

    assert!(is_user_turn_boundary(&item));
}

#[test]
fn estimate_response_item_model_visible_bytes_discounts_inline_image_data_urls() {
    let payload = "a".repeat(20_000);
    let image_item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: format!("data:image/png;base64,{payload}"),
            detail: Some(DEFAULT_IMAGE_DETAIL),
        }],
        phase: None,
    };

    let estimated = estimate_response_item_model_visible_bytes(&image_item);
    let raw = i64::try_from(
        serde_json::to_string(&image_item)
            .expect("item should serialize")
            .len(),
    )
    .expect("raw length should fit");

    assert!(estimated > 0);
    assert!(estimated < raw);
}

#[test]
fn truncate_history_item_truncates_tool_outputs_but_not_messages() {
    let output = "x".repeat(20_000);
    let function_output = ResponseItem::FunctionCallOutput {
        call_id: "call-1".to_string(),
        output: FunctionCallOutputPayload::from_text(output),
    };
    let user = user_message("hello");

    let truncated = truncate_history_item(
        &function_output,
        TruncationPolicy::Tokens(/*max_tokens*/ 64),
    );

    assert_ne!(truncated, function_output);
    assert_eq!(
        truncate_history_item(&user, TruncationPolicy::Tokens(/*max_tokens*/ 64)),
        user
    );
}

#[test]
fn replace_last_turn_images_only_rewrites_latest_tool_output() {
    let mut items = vec![
        assistant_message("already done"),
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputImage {
                    image_url: "https://example.com/older.png".to_string(),
                    detail: Some(DEFAULT_IMAGE_DETAIL),
                },
            ]),
        },
        user_message("new turn"),
        ResponseItem::FunctionCallOutput {
            call_id: "call-2".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputImage {
                    image_url: "https://example.com/newer.png".to_string(),
                    detail: Some(DEFAULT_IMAGE_DETAIL),
                },
            ]),
        },
    ];

    let replaced = replace_last_turn_images(&mut items, "omitted");

    assert!(replaced);
    assert_eq!(
        items[1],
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputImage {
                    image_url: "https://example.com/older.png".to_string(),
                    detail: Some(DEFAULT_IMAGE_DETAIL),
                },
            ]),
        }
    );
    assert_eq!(
        items[3],
        ResponseItem::FunctionCallOutput {
            call_id: "call-2".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "omitted".to_string(),
                },
            ]),
        }
    );
}
