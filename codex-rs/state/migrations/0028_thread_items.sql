CREATE TABLE thread_items (
    thread_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    item_kind TEXT NOT NULL,
    item_at_ms INTEGER NOT NULL,
    turn_status TEXT NOT NULL,
    turn_error_json TEXT,
    turn_started_at INTEGER,
    turn_completed_at INTEGER,
    turn_duration_ms INTEGER,
    search_text TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (thread_id, item_id)
);

CREATE INDEX idx_thread_items_thread_item_at_ms
    ON thread_items(thread_id, item_at_ms DESC);
