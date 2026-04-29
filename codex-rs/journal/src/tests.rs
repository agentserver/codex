use crate::Journal;
use crate::JournalCheckpointItem;
use crate::JournalContextAudience;
use crate::JournalContextForkBehavior;
use crate::JournalContextItem;
use crate::JournalContextKey;
use crate::JournalEntry;
use crate::JournalHistoryCursor;
use crate::JournalHistoryItem;
use crate::JournalReplacePrefixCheckpoint;
use crate::JournalTruncateHistoryCheckpoint;
use crate::KeyFilter;
use crate::PromptMessage;
use crate::PromptView;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

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

fn developer_context(
    namespace: &str,
    name: &str,
    text: &str,
    prompt_order: i64,
) -> JournalContextItem {
    JournalContextItem::new(
        JournalContextKey::new(namespace, name, None),
        PromptMessage::developer_text(text),
    )
    .with_prompt_order(prompt_order)
}

#[test]
fn to_prompt_uses_latest_context_for_key() {
    let mut state = Journal::new();
    state.add(
        ["prompt", "permissions", "older"],
        developer_context("context", "permissions", "older permissions", 10),
    );
    state.add(["history", "hello"], user_message("hello"));
    state.add(
        ["prompt", "permissions", "newer"],
        developer_context("context", "permissions", "newer permissions", 10),
    );

    let prompt = state
        .to_prompt(&PromptView::root())
        .expect("prompt should render");

    assert_eq!(
        prompt,
        vec![
            ResponseItem::from(PromptMessage::developer_text("newer permissions")),
            user_message("hello"),
        ]
    );
}

#[test]
fn to_prompt_filters_context_by_audience() {
    let mut state = Journal::new();
    state.add(
        ["prompt", "root", "hint"],
        developer_context("context", "root", "root-only", 0)
            .with_audience(JournalContextAudience::RootOnly),
    );
    state.add(
        ["prompt", "child", "hint"],
        developer_context("context", "child", "child-only", 1)
            .with_audience(JournalContextAudience::SubAgentsOnly),
    );

    let root_prompt = state
        .to_prompt(&PromptView::root())
        .expect("root prompt should render");
    let child_prompt = state
        .to_prompt(&PromptView::subagent(
            "/root/worker",
            Option::<String>::None,
        ))
        .expect("child prompt should render");

    assert_eq!(
        root_prompt,
        vec![ResponseItem::from(PromptMessage::developer_text(
            "root-only"
        ))]
    );
    assert_eq!(
        child_prompt,
        vec![ResponseItem::from(PromptMessage::developer_text(
            "child-only"
        ))]
    );
}

#[test]
fn to_prompt_with_filter_matches_key_prefix() {
    let mut state = Journal::new();
    state.add(
        ["prompt", "root", "keep"],
        developer_context("context", "keep", "keep me", 0),
    );
    state.add(
        ["prompt", "child", "drop"],
        developer_context("context", "drop", "drop me", 1),
    );

    let prompt = state
        .to_prompt_with_filter(&PromptView::root(), &KeyFilter::prefix(["prompt", "root"]))
        .expect("prompt should render");

    assert_eq!(
        prompt,
        vec![ResponseItem::from(PromptMessage::developer_text("keep me"))]
    );
}

#[test]
fn checkpoints_replace_prefix_and_then_truncate_history() {
    let first = JournalHistoryItem::new(user_message("turn 1"));
    let second = JournalHistoryItem::new(assistant_message("turn 1 answer"));
    let third = JournalHistoryItem::new(user_message("turn 2"));
    let summary = JournalHistoryItem {
        id: "summary".to_string(),
        turn_id: None,
        item: assistant_message("summary"),
    };

    let state = Journal::from_entries(vec![
        JournalEntry::new(["history", "1"], first),
        JournalEntry::new(["history", "2"], second.clone()),
        JournalEntry::new(["history", "3"], third),
        JournalEntry::new(
            ["checkpoint", "replace"],
            JournalCheckpointItem::ReplacePrefix(JournalReplacePrefixCheckpoint {
                through: JournalHistoryCursor::AfterItem(second.id),
                replacement: vec![summary.clone()],
            }),
        ),
        JournalEntry::new(
            ["checkpoint", "truncate"],
            JournalCheckpointItem::TruncateHistory(JournalTruncateHistoryCheckpoint {
                through: JournalHistoryCursor::AfterItem(summary.id),
            }),
        ),
    ]);

    let prompt = state
        .to_prompt(&PromptView::root())
        .expect("prompt should render");

    assert_eq!(prompt, vec![assistant_message("summary")]);
}

#[test]
fn flatten_preserves_prompt_and_drops_obsolete_items() {
    let first = JournalHistoryItem::new(user_message("turn 1"));
    let answer = JournalHistoryItem::new(assistant_message("turn 1 answer"));
    let summary = JournalHistoryItem {
        id: "summary".to_string(),
        turn_id: None,
        item: assistant_message("summary"),
    };
    let state = Journal::from_entries(vec![
        JournalEntry::new(
            ["prompt", "permissions", "old"],
            developer_context("context", "permissions", "old", 0),
        ),
        JournalEntry::new(
            ["prompt", "permissions", "new"],
            developer_context("context", "permissions", "new", 0),
        ),
        JournalEntry::new(["history", "1"], first),
        JournalEntry::new(["history", "2"], answer.clone()),
        JournalEntry::new(
            ["checkpoint", "replace"],
            JournalCheckpointItem::ReplacePrefix(JournalReplacePrefixCheckpoint {
                through: JournalHistoryCursor::AfterItem(answer.id),
                replacement: vec![summary.clone()],
            }),
        ),
    ]);

    let before = state
        .to_prompt(&PromptView::root())
        .expect("prompt should render");
    let flattened = state.flatten().expect("flatten should succeed");
    let after = flattened
        .to_prompt(&PromptView::root())
        .expect("flattened prompt should render");

    assert_eq!(before, after);
    assert_eq!(
        flattened.entries(),
        vec![
            JournalEntry::new(
                ["prompt", "permissions", "new"],
                developer_context("context", "permissions", "new", 0),
            ),
            JournalEntry::new(
                ["checkpoint", "replace", "replacement", "0", "summary"],
                summary,
            ),
        ]
    );
}

#[test]
fn with_history_window_keeps_only_recent_effective_history() {
    let first = JournalHistoryItem::new(user_message("turn 1"));
    let second = JournalHistoryItem::new(assistant_message("turn 1 answer"));
    let third = JournalHistoryItem::new(user_message("turn 2"));
    let state = Journal::from_entries(vec![
        JournalEntry::new(
            ["prompt", "permissions", "current"],
            developer_context("context", "permissions", "p", 0),
        ),
        JournalEntry::new(["history", "1"], first),
        JournalEntry::new(["history", "2"], second.clone()),
        JournalEntry::new(["history", "3"], third.clone()),
    ]);

    let windowed = state
        .with_history_window(&JournalHistoryCursor::AfterItem(second.id))
        .expect("windowing should succeed");

    assert_eq!(
        windowed.entries(),
        vec![
            JournalEntry::new(
                ["prompt", "permissions", "current"],
                developer_context("context", "permissions", "p", 0),
            ),
            JournalEntry::new(["history", "3"], third),
        ]
    );
}

#[test]
fn fork_drops_non_keep_context_and_respects_audience() {
    let history = JournalHistoryItem::new(user_message("hello"));
    let state = Journal::from_entries(vec![
        JournalEntry::new(
            ["prompt", "child", "shared"],
            developer_context("context", "shared", "shared child context", 0)
                .with_audience(JournalContextAudience::SubAgentsOnly),
        ),
        JournalEntry::new(
            ["prompt", "child", "regenerate"],
            developer_context("context", "regenerate", "usage hint", 1)
                .with_audience(JournalContextAudience::SubAgentsOnly)
                .with_on_fork(JournalContextForkBehavior::Regenerate),
        ),
        JournalEntry::new(
            ["prompt", "root", "only"],
            developer_context("context", "root", "root only", 2)
                .with_audience(JournalContextAudience::RootOnly),
        ),
        JournalEntry::new(["history", "hello"], history.clone()),
    ]);

    let forked = state
        .fork(&PromptView::subagent(
            "/root/worker",
            Option::<String>::None,
        ))
        .expect("fork should succeed");

    assert_eq!(
        forked.entries(),
        vec![
            JournalEntry::new(
                ["prompt", "child", "shared"],
                developer_context("context", "shared", "shared child context", 0)
                    .with_audience(JournalContextAudience::SubAgentsOnly),
            ),
            JournalEntry::new(["history", "hello"], history),
        ]
    );
}

#[test]
fn persist_and_load_jsonl_round_trip() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("journal.jsonl");
    let history = JournalHistoryItem::new(user_message("hello"));
    let state = Journal::from_entries(vec![
        JournalEntry::new(
            ["prompt", "permissions", "current"],
            developer_context("context", "permissions", "p", 0),
        ),
        JournalEntry::new(["history", "hello"], history),
    ]);

    state
        .persist_jsonl(path.as_path())
        .expect("journal should persist");
    let loaded = Journal::load_jsonl(path.as_path()).expect("journal should load");

    assert_eq!(loaded, state);
}

#[test]
fn filter_returns_matching_raw_entries() {
    let state = Journal::from_entries(vec![
        JournalEntry::new(
            ["prompt", "root", "keep"],
            developer_context("context", "keep", "keep me", 0),
        ),
        JournalEntry::new(
            ["prompt", "child", "drop"],
            developer_context("context", "drop", "drop me", 1),
        ),
        JournalEntry::new(["history", "hello"], user_message("hello")),
    ]);

    let filtered = state.filter(&KeyFilter::prefix(["prompt", "root"]));

    assert_eq!(
        filtered.entries(),
        vec![JournalEntry::new(
            ["prompt", "root", "keep"],
            developer_context("context", "keep", "keep me", 0),
        )]
    );
}
