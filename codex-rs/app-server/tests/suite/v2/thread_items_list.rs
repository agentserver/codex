use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::test_absolute_path;
use app_test_support::to_response;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SortDirection;
use codex_app_server_protocol::ThreadHistoryItem;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadItemsListParams;
use codex_app_server_protocol::ThreadItemsListResponse;
use codex_protocol::protocol::AgentMessageEvent;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ExecCommandEndEvent;
use codex_protocol::protocol::ExecCommandSource;
use codex_protocol::protocol::ExecCommandStatus;
use codex_protocol::protocol::ImageGenerationEndEvent;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::protocol::RolloutLine;
use codex_protocol::protocol::SessionMeta;
use codex_protocol::protocol::SessionMetaLine;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TurnCompleteEvent;
use codex_protocol::protocol::TurnStartedEvent;
use codex_protocol::protocol::UserMessageEvent;
use codex_rollout::state_db::sync_renderable_thread_items;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;
use uuid::Uuid;

#[cfg(windows)]
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);
#[cfg(not(windows))]
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn thread_items_list_pages_renderable_items_and_skips_command_execution() -> Result<()> {
    let codex_home = TempDir::new()?;
    let thread_id = write_thread_rollout(
        codex_home.path(),
        &[
            base_turn_events("turn-1", "first", "assistant one")?,
            user_turn("turn-2", "second")?,
        ]
        .concat(),
    )?;
    create_config_toml(codex_home.path())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let read_id = mcp
        .send_thread_items_list_request(ThreadItemsListParams {
            thread_id: thread_id.clone(),
            cursor: None,
            limit: Some(2),
            sort_direction: Some(SortDirection::Desc),
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadItemsListResponse {
        data, next_cursor, ..
    } = to_response::<ThreadItemsListResponse>(read_resp)?;
    assert_eq!(history_kinds(&data), vec!["userMessage", "imageGeneration"]);
    let next_cursor = next_cursor.expect("expected next cursor");

    let read_id = mcp
        .send_thread_items_list_request(ThreadItemsListParams {
            thread_id: thread_id.clone(),
            cursor: Some(next_cursor),
            limit: Some(10),
            sort_direction: Some(SortDirection::Desc),
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadItemsListResponse { data, .. } = to_response::<ThreadItemsListResponse>(read_resp)?;
    assert_eq!(history_kinds(&data), vec!["agentMessage", "userMessage"]);

    let read_id = mcp
        .send_thread_items_list_request(ThreadItemsListParams {
            thread_id,
            cursor: None,
            limit: Some(10),
            sort_direction: Some(SortDirection::Desc),
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadItemsListResponse { data, .. } = to_response::<ThreadItemsListResponse>(read_resp)?;
    assert_eq!(
        history_kinds(&data),
        vec![
            "userMessage",
            "imageGeneration",
            "agentMessage",
            "userMessage",
        ]
    );

    Ok(())
}

#[tokio::test]
async fn thread_items_list_backwards_cursor_includes_anchor_for_newer_items() -> Result<()> {
    let codex_home = TempDir::new()?;
    let thread_id = write_thread_rollout(
        codex_home.path(),
        &[
            user_turn("turn-1", "first")?,
            user_turn("turn-2", "second")?,
        ]
        .concat(),
    )?;
    create_config_toml(codex_home.path())?;

    let rollout_path = rollout_path(codex_home.path(), &thread_id);
    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let state_db =
        codex_state::StateRuntime::init(codex_home.path().to_path_buf(), "mock_provider".into())
            .await?;

    let read_id = mcp
        .send_thread_items_list_request(ThreadItemsListParams {
            thread_id: thread_id.clone(),
            cursor: None,
            limit: Some(1),
            sort_direction: Some(SortDirection::Desc),
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadItemsListResponse {
        backwards_cursor, ..
    } = to_response::<ThreadItemsListResponse>(read_resp)?;
    let backwards_cursor = backwards_cursor.expect("expected backwards cursor");

    append_rollout_events(rollout_path.as_path(), &user_turn("turn-3", "third")?)?;
    sync_renderable_thread_items(
        Some(state_db.as_ref()),
        codex_protocol::ThreadId::from_string(&thread_id)?,
        rollout_path.as_path(),
        "thread_items_list_test",
    )
    .await;

    let read_id = mcp
        .send_thread_items_list_request(ThreadItemsListParams {
            thread_id,
            cursor: Some(backwards_cursor),
            limit: Some(10),
            sort_direction: Some(SortDirection::Asc),
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadItemsListResponse { data, .. } = to_response::<ThreadItemsListResponse>(read_resp)?;
    assert_eq!(user_texts(&data), vec!["second", "third"]);

    Ok(())
}

fn base_turn_events(turn_id: &str, user_text: &str, agent_text: &str) -> Result<Vec<RolloutLine>> {
    let mut events = user_turn(turn_id, user_text)?;
    events.insert(
        2,
        rollout_line(
            "2025-01-05T12:00:02Z",
            RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
                message: agent_text.to_string(),
                phase: None,
                memory_citation: None,
            })),
        ),
    );
    events.insert(
        3,
        rollout_line(
            "2025-01-05T12:00:03Z",
            RolloutItem::EventMsg(EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: "cmd-1".to_string(),
                process_id: None,
                turn_id: turn_id.to_string(),
                command: vec!["echo".to_string(), "secret".to_string()],
                cwd: test_absolute_path("/"),
                parsed_cmd: Vec::new(),
                source: ExecCommandSource::Agent,
                interaction_input: None,
                stdout: "secret".to_string(),
                stderr: String::new(),
                aggregated_output: "secret".to_string(),
                exit_code: 0,
                duration: Duration::from_millis(10),
                formatted_output: "secret".to_string(),
                status: ExecCommandStatus::Completed,
            })),
        ),
    );
    events.insert(
        4,
        rollout_line(
            "2025-01-05T12:00:04Z",
            RolloutItem::EventMsg(EventMsg::ImageGenerationEnd(ImageGenerationEndEvent {
                call_id: "img-1".to_string(),
                status: "completed".to_string(),
                revised_prompt: Some("draw cat".to_string()),
                result: "https://example.com/generated.png".to_string(),
                saved_path: None,
            })),
        ),
    );
    Ok(events)
}

fn user_turn(turn_id: &str, text: &str) -> Result<Vec<RolloutLine>> {
    Ok(vec![
        rollout_line(
            "2025-01-05T12:00:00Z",
            RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: turn_id.to_string(),
                started_at: Some(1_736_078_400),
                model_context_window: None,
                collaboration_mode_kind: Default::default(),
            })),
        ),
        rollout_line(
            "2025-01-05T12:00:01Z",
            RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
                message: text.to_string(),
                images: Some(Vec::new()),
                local_images: Vec::new(),
                text_elements: Vec::new(),
            })),
        ),
        rollout_line(
            "2025-01-05T12:00:05Z",
            RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: turn_id.to_string(),
                last_agent_message: None,
                completed_at: Some(1_736_078_405),
                duration_ms: Some(5_000),
                time_to_first_token_ms: None,
            })),
        ),
    ])
}

fn write_thread_rollout(codex_home: &Path, events: &[RolloutLine]) -> Result<String> {
    let thread_id = Uuid::now_v7().to_string();
    let rollout_path = rollout_path(codex_home, &thread_id);
    let Some(parent) = rollout_path.parent() else {
        anyhow::bail!(
            "rollout path should have parent: {}",
            rollout_path.display()
        );
    };
    std::fs::create_dir_all(parent)?;

    let session_meta = RolloutLine {
        timestamp: "2025-01-05T12:00:00Z".to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id: codex_protocol::ThreadId::from_string(&thread_id)?,
                forked_from_id: None,
                timestamp: "2025-01-05T12:00:00Z".to_string(),
                cwd: test_absolute_path("/").into(),
                originator: "test".to_string(),
                cli_version: "0.0.0".to_string(),
                source: SessionSource::Cli,
                agent_path: None,
                agent_nickname: None,
                agent_role: None,
                model_provider: Some("mock_provider".to_string()),
                base_instructions: None,
                dynamic_tools: None,
                memory_mode: None,
            },
            git: None,
        }),
    };
    let mut lines = vec![session_meta];
    lines.extend_from_slice(events);
    let jsonl = lines
        .iter()
        .map(serde_json::to_string)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join("\n");
    std::fs::write(rollout_path, format!("{jsonl}\n"))?;
    Ok(thread_id)
}

fn append_rollout_events(path: &Path, events: &[RolloutLine]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    for line in events {
        writeln!(file, "{}", serde_json::to_string(line)?)?;
    }
    Ok(())
}

fn rollout_line(timestamp: &str, item: RolloutItem) -> RolloutLine {
    RolloutLine {
        timestamp: timestamp.to_string(),
        item,
    }
}

fn rollout_path(codex_home: &Path, thread_id: &str) -> std::path::PathBuf {
    codex_home.join(format!(
        "sessions/2025/01/05/rollout-2025-01-05T12-00-00-{thread_id}.jsonl"
    ))
}

fn history_kinds(items: &[ThreadHistoryItem]) -> Vec<&'static str> {
    items
        .iter()
        .map(|item| match item.item {
            ThreadItem::UserMessage { .. } => "userMessage",
            ThreadItem::AgentMessage { .. } => "agentMessage",
            ThreadItem::ImageGeneration { .. } => "imageGeneration",
            ThreadItem::HookPrompt { .. } => "hookPrompt",
            ThreadItem::Plan { .. } => "plan",
            ThreadItem::Reasoning { .. } => "reasoning",
            ThreadItem::CommandExecution { .. } => "commandExecution",
            ThreadItem::FileChange { .. } => "fileChange",
            ThreadItem::McpToolCall { .. } => "mcpToolCall",
            ThreadItem::DynamicToolCall { .. } => "dynamicToolCall",
            ThreadItem::CollabAgentToolCall { .. } => "collabAgentToolCall",
            ThreadItem::WebSearch { .. } => "webSearch",
            ThreadItem::ImageView { .. } => "imageView",
            ThreadItem::EnteredReviewMode { .. } => "enteredReviewMode",
            ThreadItem::ExitedReviewMode { .. } => "exitedReviewMode",
            ThreadItem::ContextCompaction { .. } => "contextCompaction",
        })
        .collect()
}

fn user_texts(items: &[ThreadHistoryItem]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|item| match &item.item {
            ThreadItem::UserMessage { content, .. } => match content.first()? {
                codex_app_server_protocol::UserInput::Text { text, .. } => Some(text.as_str()),
                codex_app_server_protocol::UserInput::Image { .. }
                | codex_app_server_protocol::UserInput::LocalImage { .. }
                | codex_app_server_protocol::UserInput::Skill { .. }
                | codex_app_server_protocol::UserInput::Mention { .. } => None,
            },
            _ => None,
        })
        .collect()
}

fn create_config_toml(codex_home: &Path) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        r#"
model = "mock-model"
model_provider = "mock_provider"
approval_policy = "never"
suppress_unstable_features_warning = true

[features]
sqlite = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "http://127.0.0.1:1/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
    )
}
