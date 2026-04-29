//! Resolve saved-session state needed before resuming or forking a thread.
//!
//! The app-server API owns normal thread lifecycle data. This module handles the local fallback
//! path used before a thread has been resumed, where TUI may need the saved cwd or model from the
//! local rollout JSONL to rebuild config or render an inactive session accurately.

use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::cwd_prompt;
use crate::cwd_prompt::CwdPromptAction;
use crate::cwd_prompt::CwdPromptOutcome;
use crate::cwd_prompt::CwdSelection;
use crate::legacy_core::config::Config;
use crate::tui::Tui;
use codex_protocol::ThreadId;
use codex_rollout::state_db::get_state_db;
use codex_utils_path as path_utils;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;

#[derive(Deserialize)]
struct SessionMetadata {
    id: ThreadId,
    cwd: PathBuf,
}

#[derive(Deserialize)]
struct LatestTurnContext {
    cwd: PathBuf,
    model: String,
}

#[derive(Deserialize)]
struct RawRecord {
    #[serde(rename = "type")]
    item_type: String,
    payload: Option<Value>,
}

pub(crate) enum ResolveCwdOutcome {
    Continue(Option<PathBuf>),
    Exit,
}

pub(crate) async fn resolve_session_thread_id(
    path: &Path,
    id_str_if_uuid: Option<&str>,
) -> Option<ThreadId> {
    match id_str_if_uuid {
        Some(id_str) => ThreadId::from_string(id_str).ok(),
        None => read_session_metadata(path)
            .await
            .ok()
            .map(|metadata| metadata.id),
    }
}

pub(crate) async fn read_session_model(
    config: &Config,
    thread_id: ThreadId,
    path: Option<&Path>,
) -> Option<String> {
    if let Some(state_db_ctx) = get_state_db(config).await
        && let Ok(Some(metadata)) = state_db_ctx.get_thread(thread_id).await
        && let Some(model) = metadata.model
    {
        return Some(model);
    }

    let path = path?;
    read_latest_turn_context(path).await.map(|item| item.model)
}

pub(crate) async fn resolve_cwd_for_resume_or_fork(
    tui: &mut Tui,
    config: &Config,
    current_cwd: &Path,
    thread_id: ThreadId,
    path: Option<&Path>,
    action: CwdPromptAction,
    allow_prompt: bool,
) -> color_eyre::Result<ResolveCwdOutcome> {
    let Some(history_cwd) = read_session_cwd(config, thread_id, path).await else {
        return Ok(ResolveCwdOutcome::Continue(None));
    };
    if allow_prompt && cwds_differ(current_cwd, &history_cwd) {
        let selection_outcome =
            cwd_prompt::run_cwd_selection_prompt(tui, action, current_cwd, &history_cwd).await?;
        return Ok(match selection_outcome {
            CwdPromptOutcome::Selection(CwdSelection::Current) => {
                ResolveCwdOutcome::Continue(Some(current_cwd.to_path_buf()))
            }
            CwdPromptOutcome::Selection(CwdSelection::Session) => {
                ResolveCwdOutcome::Continue(Some(history_cwd))
            }
            CwdPromptOutcome::Exit => ResolveCwdOutcome::Exit,
        });
    }
    Ok(ResolveCwdOutcome::Continue(Some(history_cwd)))
}

async fn read_session_cwd(
    config: &Config,
    thread_id: ThreadId,
    path: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(state_db_ctx) = get_state_db(config).await
        && let Ok(Some(metadata)) = state_db_ctx.get_thread(thread_id).await
    {
        return Some(metadata.cwd);
    }

    // Prefer the latest TurnContext cwd so resume/fork reflects the most recent
    // session directory (for the changed-cwd prompt) when DB data is unavailable.
    // The alternative would be mutating the session metadata line when the session cwd
    // changes, but the rollout is an append-only JSONL log and rewriting the head
    // would be error-prone.
    let path = path?;
    if let Some(cwd) = read_latest_turn_context(path).await.map(|item| item.cwd) {
        return Some(cwd);
    }
    match read_session_metadata(path).await {
        Ok(metadata) => Some(metadata.cwd),
        Err(err) => {
            let rollout_path = path.display().to_string();
            tracing::warn!(
                %rollout_path,
                %err,
                "Failed to read session metadata from rollout"
            );
            None
        }
    }
}

pub(crate) fn cwds_differ(current_cwd: &Path, session_cwd: &Path) -> bool {
    !path_utils::paths_match_after_normalization(current_cwd, session_cwd)
}

async fn read_session_metadata(path: &Path) -> io::Result<SessionMetadata> {
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<RawRecord>(trimmed).map_err(|err| {
            io::Error::other(format!(
                "failed to parse rollout line in {}: {err}",
                path.display()
            ))
        })?;
        if record.item_type != "session_meta" {
            return Err(io::Error::other(format!(
                "rollout at {} does not start with session metadata",
                path.display()
            )));
        }
        let payload = record.payload.ok_or_else(|| {
            io::Error::other(format!(
                "session metadata in {} is missing a payload",
                path.display()
            ))
        })?;
        return serde_json::from_value(payload).map_err(|err| {
            io::Error::other(format!(
                "failed to parse session metadata in {}: {err}",
                path.display()
            ))
        });
    }

    Err(io::Error::other(format!(
        "rollout at {} is empty",
        path.display()
    )))
}

async fn read_latest_turn_context(path: &Path) -> Option<LatestTurnContext> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<RawRecord>(trimmed) else {
            continue;
        };
        if record.item_type != "turn_context" {
            continue;
        }
        let Some(payload) = record.payload else {
            continue;
        };
        if let Ok(item) = serde_json::from_value(payload) {
            return Some(item);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy_core::config::ConfigBuilder;
    use codex_features::Feature;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    async fn build_config(temp_dir: &TempDir) -> std::io::Result<Config> {
        ConfigBuilder::default()
            .codex_home(temp_dir.path().to_path_buf())
            .build()
            .await
    }

    fn rollout_line(
        timestamp: &str,
        item_type: &str,
        payload: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "timestamp": timestamp,
            "type": item_type,
            "payload": payload,
        })
    }

    fn turn_context_line(config: &Config, cwd: PathBuf, timestamp: &str) -> serde_json::Value {
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "gpt-5.1".to_string());
        rollout_line(
            timestamp,
            "turn_context",
            serde_json::json!({
                "cwd": cwd,
                "model": model,
            }),
        )
    }

    fn session_meta_line(thread_id: ThreadId, cwd: PathBuf, timestamp: &str) -> serde_json::Value {
        rollout_line(
            timestamp,
            "session_meta",
            serde_json::json!({
                "id": thread_id.to_string(),
                "timestamp": timestamp,
                "cwd": cwd,
                "originator": "test",
                "cli_version": "test",
            }),
        )
    }

    fn write_rollout_lines(path: &Path, lines: &[serde_json::Value]) -> std::io::Result<()> {
        let mut text = String::new();
        for line in lines {
            text.push_str(&serde_json::to_string(line).expect("serialize rollout"));
            text.push('\n');
        }
        std::fs::write(path, text)
    }

    #[tokio::test]
    async fn read_session_cwd_returns_none_without_sqlite_or_rollout_path() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;

        let cwd = read_session_cwd(&config, ThreadId::new(), /*path*/ None).await;

        assert_eq!(cwd, None);
        Ok(())
    }

    #[tokio::test]
    async fn read_session_cwd_prefers_latest_turn_context() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let first = temp_dir.path().join("first");
        let second = temp_dir.path().join("second");
        std::fs::create_dir_all(&first)?;
        std::fs::create_dir_all(&second)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        write_rollout_lines(
            &rollout_path,
            &[
                turn_context_line(&config, first, "t0"),
                turn_context_line(&config, second.clone(), "t1"),
            ],
        )?;

        let cwd = read_session_cwd(&config, ThreadId::new(), Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(cwd, second);
        Ok(())
    }

    #[tokio::test]
    async fn should_prompt_when_meta_matches_current_but_latest_turn_differs() -> std::io::Result<()>
    {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let current = temp_dir.path().join("current");
        let latest = temp_dir.path().join("latest");
        std::fs::create_dir_all(&current)?;
        std::fs::create_dir_all(&latest)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        write_rollout_lines(
            &rollout_path,
            &[
                session_meta_line(ThreadId::new(), current.clone(), "t0"),
                turn_context_line(&config, latest.clone(), "t1"),
            ],
        )?;

        let session_cwd = read_session_cwd(&config, ThreadId::new(), Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(session_cwd, latest);
        assert!(cwds_differ(&current, &session_cwd));
        Ok(())
    }

    #[tokio::test]
    async fn read_session_cwd_falls_back_to_session_meta() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let config = build_config(&temp_dir).await?;
        let session_cwd = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_cwd)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        write_rollout_lines(
            &rollout_path,
            &[session_meta_line(
                ThreadId::new(),
                session_cwd.clone(),
                "t0",
            )],
        )?;

        let cwd = read_session_cwd(&config, ThreadId::new(), Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(cwd, session_cwd);
        Ok(())
    }

    #[tokio::test]
    async fn read_session_cwd_prefers_sqlite_when_thread_id_present() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut config = build_config(&temp_dir).await?;
        config
            .features
            .enable(Feature::Sqlite)
            .expect("test config should allow sqlite");

        let thread_id = ThreadId::new();
        let rollout_cwd = temp_dir.path().join("rollout-cwd");
        let sqlite_cwd = temp_dir.path().join("sqlite-cwd");
        std::fs::create_dir_all(&rollout_cwd)?;
        std::fs::create_dir_all(&sqlite_cwd)?;

        let rollout_path = temp_dir.path().join("rollout.jsonl");
        write_rollout_lines(
            &rollout_path,
            &[turn_context_line(&config, rollout_cwd, "t0")],
        )?;

        let runtime = codex_state::StateRuntime::init(
            config.codex_home.to_path_buf(),
            config.model_provider_id.clone(),
        )
        .await
        .map_err(std::io::Error::other)?;
        runtime
            .mark_backfill_complete(/*last_watermark*/ None)
            .await
            .map_err(std::io::Error::other)?;

        let mut builder = codex_state::ThreadMetadataBuilder::new(
            thread_id,
            rollout_path.clone(),
            chrono::Utc::now(),
            serde_json::from_value(serde_json::json!("cli"))
                .expect("cli session source should deserialize"),
        );
        builder.cwd = sqlite_cwd.clone();
        let metadata = builder.build(config.model_provider_id.as_str());
        runtime
            .upsert_thread(&metadata)
            .await
            .map_err(std::io::Error::other)?;

        let cwd = read_session_cwd(&config, thread_id, Some(&rollout_path))
            .await
            .expect("expected cwd");
        assert_eq!(cwd, sqlite_cwd);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_session_thread_id_reads_minimal_session_metadata() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let thread_id = ThreadId::new();
        let rollout_path = temp_dir.path().join("rollout.jsonl");
        write_rollout_lines(
            &rollout_path,
            &[session_meta_line(
                thread_id,
                temp_dir.path().to_path_buf(),
                "t0",
            )],
        )?;

        let resolved = resolve_session_thread_id(&rollout_path, /*id_str_if_uuid*/ None).await;

        assert_eq!(resolved, Some(thread_id));
        Ok(())
    }
}
