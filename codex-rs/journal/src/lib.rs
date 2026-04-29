//! Typed journal model for prompt rendering, filtering, forking, and persistence.

mod error;
pub mod history;
mod journal;
mod prompt_view;

#[cfg(test)]
mod tests;

pub use codex_protocol::journal::JournalCheckpointItem;
pub use codex_protocol::journal::JournalContextAudience;
pub use codex_protocol::journal::JournalContextForkBehavior;
pub use codex_protocol::journal::JournalContextItem;
pub use codex_protocol::journal::JournalContextKey;
pub use codex_protocol::journal::JournalEntry;
pub use codex_protocol::journal::JournalHistoryCursor;
pub use codex_protocol::journal::JournalHistoryItem;
pub use codex_protocol::journal::JournalItem;
pub use codex_protocol::journal::JournalKey;
pub use codex_protocol::journal::JournalReplacePrefixCheckpoint;
pub use codex_protocol::journal::JournalTruncateHistoryCheckpoint;
pub use codex_protocol::journal::KeyFilter;
pub use codex_protocol::journal::PromptMessage;
pub use codex_protocol::journal::PromptMessageRole;
pub use error::JournalError;
pub use error::Result;
pub use journal::Journal;
pub use prompt_view::PromptView;
