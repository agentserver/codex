use crate::JournalEntry;
use crate::JournalError;
use crate::JournalKey;
use crate::KeyFilter;
use crate::PromptView;
use crate::Result;
use codex_protocol::journal::JournalCheckpointItem;
use codex_protocol::journal::JournalContextAudience;
use codex_protocol::journal::JournalContextForkBehavior;
use codex_protocol::journal::JournalContextItem;
use codex_protocol::journal::JournalContextKey;
use codex_protocol::journal::JournalHistoryCursor;
use codex_protocol::journal::JournalHistoryItem;
use codex_protocol::journal::JournalItem;
use codex_protocol::journal::JournalReplacePrefixCheckpoint;
use codex_protocol::journal::JournalTruncateHistoryCheckpoint;
use codex_protocol::models::ResponseItem;
use indexmap::IndexMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;

/// Canonical typed journal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Journal {
    entries: Vec<JournalEntry>,
}

impl Journal {
    /// Creates an empty journal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a journal from an explicit list of entries.
    pub fn from_entries(entries: Vec<JournalEntry>) -> Self {
        Self { entries }
    }

    /// Appends one keyed item to the journal.
    pub fn add<K, T>(&mut self, key: K, item: T)
    where
        K: Into<JournalKey>,
        T: Into<JournalItem>,
    {
        self.entries.push(JournalEntry::new(key, item));
    }

    /// Appends several keyed journal entries.
    pub fn extend<I, T>(&mut self, entries: I)
    where
        I: IntoIterator<Item = T>,
        T: Into<JournalEntry>,
    {
        self.entries.extend(entries.into_iter().map(Into::into));
    }

    /// Returns the raw append-only journal entries.
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Returns a journal containing only journal entries whose keys match the filter.
    pub fn filter(&self, filter: &KeyFilter) -> Self {
        let entries = self
            .entries
            .iter()
            .filter(|entry| filter.matches(&entry.key))
            .cloned()
            .collect();
        Self::from_entries(entries)
    }

    /// Returns the number of journal entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the journal is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Renders the current effective journal view into model prompt items.
    pub fn to_prompt(&self, view: &PromptView) -> Result<Vec<ResponseItem>> {
        self.to_prompt_matching_filter(view, None)
    }

    /// Renders the current effective journal view into model prompt items after selecting
    /// only journal entries whose keys match the filter.
    pub fn to_prompt_with_filter(
        &self,
        view: &PromptView,
        filter: &KeyFilter,
    ) -> Result<Vec<ResponseItem>> {
        self.to_prompt_matching_filter(view, Some(filter))
    }

    /// Produces a flattened child state for the provided view.
    pub fn fork(&self, view: &PromptView) -> Result<Self> {
        self.fork_matching_filter(view, None)
    }

    /// Produces a flattened child state for the provided view after selecting only
    /// journal entries whose keys match the filter.
    pub fn fork_with_filter(&self, view: &PromptView, filter: &KeyFilter) -> Result<Self> {
        self.fork_matching_filter(view, Some(filter))
    }

    /// Drops obsolete journal entries and keeps only the current effective journal view.
    ///
    /// This is the first building block for a rolling in-memory window: callers can
    /// persist the full journal elsewhere, then keep only the flattened journal hot.
    pub fn flatten(&self) -> Result<Self> {
        let resolved = self.resolve(None)?;
        Ok(Self::from_entries(
            resolved
                .contexts
                .into_iter()
                .chain(resolved.history)
                .collect(),
        ))
    }

    /// Keeps only the current effective journal view plus the history suffix that starts
    /// at the resolved cursor.
    ///
    /// This is a lightweight rolling-window helper: callers can persist the full
    /// journal on disk, then keep only a recent hot suffix in memory.
    pub fn with_history_window(&self, start: &JournalHistoryCursor) -> Result<Self> {
        let resolved = self.resolve(None)?;
        let start_index = resolve_cursor(resolved.history.as_slice(), start)?;
        Ok(Self::from_entries(
            resolved
                .contexts
                .into_iter()
                .chain(resolved.history.into_iter().skip(start_index))
                .collect(),
        ))
    }

    /// Persists the raw journal to a JSONL file, one `JournalEntry` per line.
    pub fn persist_jsonl(&self, path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        for entry in &self.entries {
            serde_json::to_writer(&mut writer, entry)?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Loads a raw journal from a JSONL file written by [`Self::persist_jsonl`].
    pub fn load_jsonl(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        for (line_index, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let entry = serde_json::from_str::<JournalEntry>(&line).map_err(|source| {
                JournalError::ParseJson {
                    line_number: line_index + 1,
                    source,
                }
            })?;
            entries.push(entry);
        }
        Ok(Self::from_entries(entries))
    }

    fn to_prompt_matching_filter(
        &self,
        view: &PromptView,
        filter: Option<&KeyFilter>,
    ) -> Result<Vec<ResponseItem>> {
        let resolved = self.resolve(filter)?;
        let mut prompt: Vec<ResponseItem> = resolved
            .contexts
            .into_iter()
            .filter(|entry| context_visible_in_view(context_item(entry), view))
            .map(|entry| match entry.item {
                JournalItem::Context(item) => ResponseItem::from(item),
                _ => unreachable!("resolved context entries must be context items"),
            })
            .collect();
        prompt.extend(resolved.history.into_iter().map(|entry| match entry.item {
            JournalItem::History(item) => ResponseItem::from(item),
            _ => unreachable!("resolved history entries must be history items"),
        }));
        Ok(prompt)
    }

    fn fork_matching_filter(&self, view: &PromptView, filter: Option<&KeyFilter>) -> Result<Self> {
        let resolved = self.resolve(filter)?;
        Ok(Self::from_entries(
            resolved
                .contexts
                .into_iter()
                .filter(|entry| context_item(entry).on_fork == JournalContextForkBehavior::Keep)
                .filter(|entry| context_visible_in_view(context_item(entry), view))
                .chain(resolved.history)
                .collect(),
        ))
    }

    fn resolve(&self, filter: Option<&KeyFilter>) -> Result<ResolvedJournal> {
        let mut history = Vec::<JournalEntry>::new();
        let mut latest_context_by_key = IndexMap::<JournalContextKey, (usize, JournalEntry)>::new();

        for (index, entry) in self.entries.iter().enumerate() {
            if let Some(filter) = filter
                && !filter.matches(&entry.key)
            {
                continue;
            }
            match &entry.item {
                JournalItem::History(_) => history.push(entry.clone()),
                JournalItem::Context(context_item) => {
                    latest_context_by_key.insert(context_item.key.clone(), (index, entry.clone()));
                }
                JournalItem::Checkpoint(checkpoint) => {
                    apply_checkpoint(&mut history, &entry.key, checkpoint)?;
                }
            }
        }

        let mut contexts = latest_context_by_key.into_values().collect::<Vec<_>>();
        contexts.sort_by(|(left_index, left_entry), (right_index, right_entry)| {
            context_item(left_entry)
                .prompt_order
                .cmp(&context_item(right_entry).prompt_order)
                .then_with(|| left_index.cmp(right_index))
        });

        Ok(ResolvedJournal {
            contexts: contexts.into_iter().map(|(_, entry)| entry).collect(),
            history,
        })
    }
}

#[derive(Debug)]
struct ResolvedJournal {
    contexts: Vec<JournalEntry>,
    history: Vec<JournalEntry>,
}

fn apply_checkpoint(
    history: &mut Vec<JournalEntry>,
    checkpoint_key: &JournalKey,
    checkpoint: &JournalCheckpointItem,
) -> Result<()> {
    match checkpoint {
        JournalCheckpointItem::ReplacePrefix(JournalReplacePrefixCheckpoint {
            through,
            replacement,
        }) => {
            let keep_from = resolve_cursor(history.as_slice(), through)?;
            let mut next_history =
                Vec::with_capacity(replacement.len() + history.len().saturating_sub(keep_from));
            next_history.extend(
                replacement
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(index, item)| {
                        JournalEntry::new(
                            replacement_history_key(checkpoint_key, index, &item),
                            item,
                        )
                    }),
            );
            next_history.extend(history[keep_from..].iter().cloned());
            *history = next_history;
            Ok(())
        }
        JournalCheckpointItem::TruncateHistory(JournalTruncateHistoryCheckpoint { through }) => {
            let keep_len = resolve_cursor(history.as_slice(), through)?;
            history.truncate(keep_len);
            Ok(())
        }
    }
}

fn resolve_cursor(history: &[JournalEntry], cursor: &JournalHistoryCursor) -> Result<usize> {
    match cursor {
        JournalHistoryCursor::Start => Ok(0),
        JournalHistoryCursor::End => Ok(history.len()),
        JournalHistoryCursor::AfterItem(history_item_id) => history
            .iter()
            .position(|entry| history_item(entry).id == *history_item_id)
            .map(|index| index + 1)
            .ok_or_else(|| JournalError::UnknownHistoryItemId {
                history_item_id: history_item_id.clone(),
            }),
    }
}

fn replacement_history_key(
    checkpoint_key: &JournalKey,
    index: usize,
    history_item: &JournalHistoryItem,
) -> JournalKey {
    checkpoint_key
        .child("replacement")
        .child(index.to_string())
        .child(history_item.id.clone())
}

fn context_item(entry: &JournalEntry) -> &JournalContextItem {
    match &entry.item {
        JournalItem::Context(item) => item,
        _ => unreachable!("resolved context entries must be context items"),
    }
}

fn history_item(entry: &JournalEntry) -> &JournalHistoryItem {
    match &entry.item {
        JournalItem::History(item) => item,
        _ => unreachable!("resolved history entries must be history items"),
    }
}

fn context_visible_in_view(item: &JournalContextItem, view: &PromptView) -> bool {
    match &item.audience {
        JournalContextAudience::All => true,
        JournalContextAudience::RootOnly => view.is_root,
        JournalContextAudience::SubAgentsOnly => !view.is_root,
        JournalContextAudience::AgentPathPrefix(prefix) => view
            .agent_path
            .as_deref()
            .is_some_and(|agent_path| agent_path.starts_with(prefix)),
        JournalContextAudience::AgentRole(role) => view
            .agent_role
            .as_deref()
            .is_some_and(|agent_role| agent_role == role),
    }
}
