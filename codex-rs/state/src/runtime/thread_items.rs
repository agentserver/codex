use super::*;
use crate::SortDirection;
use crate::ThreadItemRecordInsert;
use crate::ThreadItemsPage;
use crate::model::ThreadItemRow;
use crate::model::anchor_from_thread_item;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::Ordering;

impl StateRuntime {
    pub async fn replace_thread_items(
        &self,
        thread_id: &str,
        items: &[ThreadItemRecordInsert],
    ) -> anyhow::Result<()> {
        let existing_rows =
            sqlx::query("SELECT item_id, item_at_ms FROM thread_items WHERE thread_id = ?")
                .bind(thread_id)
                .fetch_all(self.pool.as_ref())
                .await?;
        let existing_item_at_millis: HashMap<String, i64> = existing_rows
            .into_iter()
            .filter_map(|row| {
                let item_id = row.try_get::<String, _>("item_id").ok()?;
                let item_at_ms = row.try_get::<i64, _>("item_at_ms").ok()?;
                Some((item_id, item_at_ms))
            })
            .collect();
        let mut assigned_item_at_millis = HashSet::new();

        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM thread_items WHERE thread_id = ?")
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;

        for item in items {
            let mut item_at_ms =
                if let Some(item_at_ms) = existing_item_at_millis.get(&item.item_id) {
                    *item_at_ms
                } else {
                    datetime_to_epoch_millis(self.allocate_thread_item_at(item.item_at)?)
                };
            while !assigned_item_at_millis.insert(item_at_ms) {
                item_at_ms = item_at_ms.saturating_add(1);
            }
            sqlx::query(
                r#"
INSERT INTO thread_items (
    thread_id,
    turn_id,
    item_id,
    item_kind,
    item_at_ms,
    turn_status,
    turn_error_json,
    turn_started_at,
    turn_completed_at,
    turn_duration_ms,
    search_text,
    payload_json
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(thread_id)
            .bind(item.turn_id.as_str())
            .bind(item.item_id.as_str())
            .bind(item.item_kind.as_str())
            .bind(item_at_ms)
            .bind(item.turn_status.as_str())
            .bind(item.turn_error_json.as_deref())
            .bind(item.turn_started_at)
            .bind(item.turn_completed_at)
            .bind(item.turn_duration_ms)
            .bind(item.search_text.as_str())
            .bind(item.payload_json.as_str())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn list_thread_items(
        &self,
        thread_id: &str,
        page_size: usize,
        anchor: Option<&crate::Anchor>,
        sort_direction: crate::SortDirection,
    ) -> anyhow::Result<crate::ThreadItemsPage> {
        let limit = page_size.saturating_add(1);
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    thread_id,
    turn_id,
    item_id,
    item_kind,
    item_at_ms AS item_at,
    turn_status,
    turn_error_json,
    turn_started_at,
    turn_completed_at,
    turn_duration_ms,
    search_text,
    payload_json
FROM thread_items
WHERE thread_id = 
            "#,
        );
        builder.push_bind(thread_id);
        if let Some(anchor) = anchor {
            let item_at_ms = datetime_to_epoch_millis(anchor.ts);
            match sort_direction {
                SortDirection::Asc => builder.push(" AND item_at_ms > ").push_bind(item_at_ms),
                SortDirection::Desc => builder.push(" AND item_at_ms < ").push_bind(item_at_ms),
            };
        }
        let sort_sql = match sort_direction {
            SortDirection::Asc => " ASC",
            SortDirection::Desc => " DESC",
        };
        builder
            .push(" ORDER BY item_at_ms")
            .push(sort_sql)
            .push(" LIMIT ")
            .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));

        let rows = builder.build().fetch_all(self.pool.as_ref()).await?;
        let mut items = rows
            .into_iter()
            .map(|row| {
                ThreadItemRow::try_from_row(&row).and_then(crate::ThreadItemRecord::try_from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let num_scanned_rows = items.len();
        let next_anchor = if items.len() > page_size {
            items.pop();
            items.last().map(anchor_from_thread_item)
        } else {
            None
        };
        Ok(ThreadItemsPage {
            items,
            next_anchor,
            num_scanned_rows,
        })
    }

    pub async fn get_thread_item(
        &self,
        thread_id: &str,
        item_id: &str,
    ) -> anyhow::Result<Option<crate::ThreadItemRecord>> {
        let row = sqlx::query(
            r#"
SELECT
    thread_id,
    turn_id,
    item_id,
    item_kind,
    item_at_ms AS item_at,
    turn_status,
    turn_error_json,
    turn_started_at,
    turn_completed_at,
    turn_duration_ms,
    search_text,
    payload_json
FROM thread_items
WHERE thread_id = ? AND item_id = ?
            "#,
        )
        .bind(thread_id)
        .bind(item_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(|row| ThreadItemRow::try_from_row(&row).and_then(crate::ThreadItemRecord::try_from))
            .transpose()
    }

    fn allocate_thread_item_at(&self, item_at: DateTime<Utc>) -> anyhow::Result<DateTime<Utc>> {
        let candidate = datetime_to_epoch_millis(item_at);
        let allocated = loop {
            let current = self.thread_item_at_millis.load(Ordering::Relaxed);
            if candidate > current {
                if self
                    .thread_item_at_millis
                    .compare_exchange(current, candidate, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break candidate;
                }
                continue;
            }
            if candidate.saturating_add(1000) <= current {
                break candidate;
            }
            let bumped = current.saturating_add(1);
            if self
                .thread_item_at_millis
                .compare_exchange(current, bumped, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break bumped;
            }
        };
        epoch_millis_to_datetime(allocated)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::unique_temp_dir;
    use super::StateRuntime;
    use super::anchor_from_thread_item;
    use crate::SortDirection;
    use crate::ThreadItemRecordInsert;
    use chrono::DateTime;
    use chrono::Utc;
    use pretty_assertions::assert_eq;

    fn item(item_id: &str, item_at_ms: i64) -> ThreadItemRecordInsert {
        ThreadItemRecordInsert {
            turn_id: "turn-1".to_string(),
            item_id: item_id.to_string(),
            item_kind: "agentMessage".to_string(),
            item_at: DateTime::<Utc>::from_timestamp_millis(item_at_ms).expect("timestamp millis"),
            turn_status: "completed".to_string(),
            turn_error_json: None,
            turn_started_at: Some(item_at_ms / 1000),
            turn_completed_at: Some(item_at_ms / 1000),
            turn_duration_ms: Some(1),
            search_text: format!("search {item_id}"),
            payload_json: format!(
                r#"{{"type":"agentMessage","id":"{item_id}","text":"{item_id}"}}"#
            ),
        }
    }

    #[tokio::test]
    async fn replace_thread_items_pages_and_reuses_existing_timestamps() {
        let codex_home = unique_temp_dir();
        let runtime = StateRuntime::init(codex_home, "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id = "thread-1";

        runtime
            .replace_thread_items(
                thread_id,
                &[
                    item("item-a", 1_700_001_111_123),
                    item("item-b", 1_700_001_111_123),
                ],
            )
            .await
            .expect("initial replace should succeed");

        let page = runtime
            .list_thread_items(thread_id, 1, None, SortDirection::Desc)
            .await
            .expect("list should succeed");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].item_id, "item-b");

        let next_anchor = page.next_anchor.expect("expected next anchor");
        let older_page = runtime
            .list_thread_items(thread_id, 10, Some(&next_anchor), SortDirection::Desc)
            .await
            .expect("older page should succeed");
        assert_eq!(
            older_page
                .items
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["item-a"]
        );

        let previous_item_b = runtime
            .get_thread_item(thread_id, "item-b")
            .await
            .expect("item should load")
            .expect("item should exist");

        runtime
            .replace_thread_items(
                thread_id,
                &[
                    item("item-b", 1_700_001_111_123),
                    item("item-c", 1_700_001_111_100),
                ],
            )
            .await
            .expect("replacement should succeed");

        let current_item_b = runtime
            .get_thread_item(thread_id, "item-b")
            .await
            .expect("item should load")
            .expect("item should exist");
        assert_eq!(current_item_b.item_at, previous_item_b.item_at);

        let asc_page = runtime
            .list_thread_items(thread_id, 10, None, SortDirection::Asc)
            .await
            .expect("asc list should succeed");
        assert_eq!(
            asc_page
                .items
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["item-b", "item-c"]
        );
        assert!(asc_page.items[0].item_at < asc_page.items[1].item_at);
    }

    #[tokio::test]
    async fn replace_thread_items_next_anchor_tracks_last_row_on_page() {
        let codex_home = unique_temp_dir();
        let runtime = StateRuntime::init(codex_home, "test-provider".to_string())
            .await
            .expect("state db should initialize");
        let thread_id = "thread-2";

        runtime
            .replace_thread_items(
                thread_id,
                &[
                    item("item-a", 1_700_001_111_100),
                    item("item-b", 1_700_001_111_200),
                    item("item-c", 1_700_001_111_300),
                ],
            )
            .await
            .expect("replace should succeed");

        let page = runtime
            .list_thread_items(thread_id, 2, None, SortDirection::Desc)
            .await
            .expect("list should succeed");
        let anchor = page.next_anchor.expect("expected next anchor");
        assert_eq!(anchor, anchor_from_thread_item(&page.items[1]));
    }
}
