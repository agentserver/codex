use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use super::Anchor;
use super::epoch_millis_to_datetime;

/// A lightweight, renderable item persisted for thread history pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadItemRecord {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub item_kind: String,
    pub item_at: DateTime<Utc>,
    pub turn_status: String,
    pub turn_error_json: Option<String>,
    pub turn_started_at: Option<i64>,
    pub turn_completed_at: Option<i64>,
    pub turn_duration_ms: Option<i64>,
    pub search_text: String,
    pub payload_json: String,
}

/// Insert payload for a lightweight persisted thread item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadItemRecordInsert {
    pub turn_id: String,
    pub item_id: String,
    pub item_kind: String,
    pub item_at: DateTime<Utc>,
    pub turn_status: String,
    pub turn_error_json: Option<String>,
    pub turn_started_at: Option<i64>,
    pub turn_completed_at: Option<i64>,
    pub turn_duration_ms: Option<i64>,
    pub search_text: String,
    pub payload_json: String,
}

/// A single page of persisted thread-item results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadItemsPage {
    pub items: Vec<ThreadItemRecord>,
    pub next_anchor: Option<Anchor>,
    pub num_scanned_rows: usize,
}

#[derive(Debug)]
pub(crate) struct ThreadItemRow {
    thread_id: String,
    turn_id: String,
    item_id: String,
    item_kind: String,
    item_at: i64,
    turn_status: String,
    turn_error_json: Option<String>,
    turn_started_at: Option<i64>,
    turn_completed_at: Option<i64>,
    turn_duration_ms: Option<i64>,
    search_text: String,
    payload_json: String,
}

impl ThreadItemRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            thread_id: row.try_get("thread_id")?,
            turn_id: row.try_get("turn_id")?,
            item_id: row.try_get("item_id")?,
            item_kind: row.try_get("item_kind")?,
            item_at: row.try_get("item_at")?,
            turn_status: row.try_get("turn_status")?,
            turn_error_json: row.try_get("turn_error_json")?,
            turn_started_at: row.try_get("turn_started_at")?,
            turn_completed_at: row.try_get("turn_completed_at")?,
            turn_duration_ms: row.try_get("turn_duration_ms")?,
            search_text: row.try_get("search_text")?,
            payload_json: row.try_get("payload_json")?,
        })
    }
}

impl TryFrom<ThreadItemRow> for ThreadItemRecord {
    type Error = anyhow::Error;

    fn try_from(value: ThreadItemRow) -> Result<Self> {
        Ok(Self {
            thread_id: value.thread_id,
            turn_id: value.turn_id,
            item_id: value.item_id,
            item_kind: value.item_kind,
            item_at: epoch_millis_to_datetime(value.item_at)?,
            turn_status: value.turn_status,
            turn_error_json: value.turn_error_json,
            turn_started_at: value.turn_started_at,
            turn_completed_at: value.turn_completed_at,
            turn_duration_ms: value.turn_duration_ms,
            search_text: value.search_text,
            payload_json: value.payload_json,
        })
    }
}

pub(crate) fn anchor_from_thread_item(item: &ThreadItemRecord) -> Anchor {
    Anchor { ts: item.item_at }
}
