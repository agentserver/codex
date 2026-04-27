use chrono::DateTime;
use chrono::Utc;
use codex_app_server_protocol::ThreadItem as ApiThreadItem;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnError;
use codex_app_server_protocol::build_turns_from_rollout_items;
use codex_protocol::protocol::RolloutItem;
use codex_state::ThreadItemRecordInsert;

/// Build the lightweight, renderable thread-item subset we persist in SQLite.
pub(crate) fn build_persisted_thread_items(
    items: &[RolloutItem],
) -> anyhow::Result<Vec<ThreadItemRecordInsert>> {
    let turns = build_turns_from_rollout_items(items);
    let mut persisted = Vec::new();
    let mut last_item_at_ms = None;

    for turn in turns {
        for (index, item) in turn.items.iter().enumerate() {
            if !should_persist_item(item) {
                continue;
            }
            let item_at = item_timestamp_for_turn(&turn, index, last_item_at_ms)?;
            last_item_at_ms = Some(item_at.timestamp_millis());
            persisted.push(ThreadItemRecordInsert {
                turn_id: turn.id.clone(),
                item_id: item.id().to_string(),
                item_kind: item_kind(item).to_string(),
                item_at,
                turn_status: serde_json::to_value(turn.status.clone())?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                turn_error_json: serialize_turn_error(turn.error.as_ref())?,
                turn_started_at: turn.started_at,
                turn_completed_at: turn.completed_at,
                turn_duration_ms: turn.duration_ms,
                search_text: search_text(item),
                payload_json: serde_json::to_string(item)?,
            });
        }
    }

    Ok(persisted)
}

fn should_persist_item(item: &ApiThreadItem) -> bool {
    matches!(
        item,
        ApiThreadItem::UserMessage { .. }
            | ApiThreadItem::HookPrompt { .. }
            | ApiThreadItem::AgentMessage { .. }
            | ApiThreadItem::Plan { .. }
            | ApiThreadItem::Reasoning { .. }
            | ApiThreadItem::WebSearch { .. }
            | ApiThreadItem::ImageView { .. }
            | ApiThreadItem::ImageGeneration { .. }
            | ApiThreadItem::EnteredReviewMode { .. }
            | ApiThreadItem::ExitedReviewMode { .. }
            | ApiThreadItem::ContextCompaction { .. }
    )
}

fn item_kind(item: &ApiThreadItem) -> &'static str {
    match item {
        ApiThreadItem::UserMessage { .. } => "userMessage",
        ApiThreadItem::HookPrompt { .. } => "hookPrompt",
        ApiThreadItem::AgentMessage { .. } => "agentMessage",
        ApiThreadItem::Plan { .. } => "plan",
        ApiThreadItem::Reasoning { .. } => "reasoning",
        ApiThreadItem::CommandExecution { .. } => "commandExecution",
        ApiThreadItem::FileChange { .. } => "fileChange",
        ApiThreadItem::McpToolCall { .. } => "mcpToolCall",
        ApiThreadItem::DynamicToolCall { .. } => "dynamicToolCall",
        ApiThreadItem::CollabAgentToolCall { .. } => "collabAgentToolCall",
        ApiThreadItem::WebSearch { .. } => "webSearch",
        ApiThreadItem::ImageView { .. } => "imageView",
        ApiThreadItem::ImageGeneration { .. } => "imageGeneration",
        ApiThreadItem::EnteredReviewMode { .. } => "enteredReviewMode",
        ApiThreadItem::ExitedReviewMode { .. } => "exitedReviewMode",
        ApiThreadItem::ContextCompaction { .. } => "contextCompaction",
    }
}

fn serialize_turn_error(turn_error: Option<&TurnError>) -> anyhow::Result<Option<String>> {
    turn_error
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn item_timestamp_for_turn(
    turn: &Turn,
    index: usize,
    last_item_at_ms: Option<i64>,
) -> anyhow::Result<DateTime<Utc>> {
    let base_ms = turn
        .started_at
        .or(turn.completed_at)
        .map(|seconds| seconds.saturating_mul(1000))
        .or(last_item_at_ms.map(|value| value.saturating_add(1)))
        .unwrap_or_else(|| i64::try_from(index).unwrap_or(i64::MAX));
    let candidate_ms = base_ms.saturating_add(i64::try_from(index).unwrap_or(i64::MAX));
    DateTime::<Utc>::from_timestamp_millis(candidate_ms)
        .ok_or_else(|| anyhow::anyhow!("invalid thread item timestamp millis: {candidate_ms}"))
}

fn search_text(item: &ApiThreadItem) -> String {
    match item {
        ApiThreadItem::UserMessage { content, .. } => content
            .iter()
            .map(|entry| match entry {
                codex_app_server_protocol::UserInput::Text { text, .. } => text.clone(),
                codex_app_server_protocol::UserInput::Image { url } => url.clone(),
                codex_app_server_protocol::UserInput::LocalImage { path } => {
                    path.display().to_string()
                }
                codex_app_server_protocol::UserInput::Skill { name, path } => {
                    format!("{name} {}", path.display())
                }
                codex_app_server_protocol::UserInput::Mention { name, path } => {
                    format!("{name} {path}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        ApiThreadItem::HookPrompt { fragments, .. } => fragments
            .iter()
            .map(|fragment| fragment.text.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        ApiThreadItem::AgentMessage { text, .. } => text.clone(),
        ApiThreadItem::Plan { text, .. } => text.clone(),
        ApiThreadItem::Reasoning {
            summary, content, ..
        } => summary
            .iter()
            .chain(content.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        ApiThreadItem::WebSearch { query, .. } => query.clone(),
        ApiThreadItem::ImageView { path, .. } => path.display().to_string(),
        ApiThreadItem::ImageGeneration {
            revised_prompt,
            saved_path,
            ..
        } => [
            revised_prompt.clone().unwrap_or_default(),
            saved_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n"),
        ApiThreadItem::EnteredReviewMode { review, .. }
        | ApiThreadItem::ExitedReviewMode { review, .. } => review.clone(),
        ApiThreadItem::ContextCompaction { .. }
        | ApiThreadItem::CommandExecution { .. }
        | ApiThreadItem::FileChange { .. }
        | ApiThreadItem::McpToolCall { .. }
        | ApiThreadItem::DynamicToolCall { .. }
        | ApiThreadItem::CollabAgentToolCall { .. } => String::new(),
    }
}
