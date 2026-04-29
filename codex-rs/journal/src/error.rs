use thiserror::Error;

/// Convenience result type for journal operations.
pub type Result<T> = std::result::Result<T, JournalError>;

/// Errors produced while materializing or persisting a journal.
#[derive(Debug, Error)]
pub enum JournalError {
    #[error("failed to read or write journal")]
    Io(#[from] std::io::Error),
    #[error("failed to serialize journal item")]
    SerializeJson {
        #[from]
        source: serde_json::Error,
    },
    #[error("failed to parse journal item at line {line_number}")]
    ParseJson {
        line_number: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("history cursor referenced unknown history item id `{history_item_id}`")]
    UnknownHistoryItemId { history_item_id: String },
}
