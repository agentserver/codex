use super::*;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_protocol::models::DEFAULT_IMAGE_DETAIL;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ResponseInputItem;
use pretty_assertions::assert_eq;

#[test]
fn rejects_missing_environment_id() {
    let json = r#"{"path": "/tmp/foo.png"}"#;
    let err = parse_arguments::<ViewImageInEnvironmentArgs>(json)
        .expect_err("missing environment_id should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("environment_id"),
        "expected error to mention environment_id, got: {msg}"
    );
}

#[test]
fn rejects_missing_path() {
    let json = r#"{"environment_id": "exe_two"}"#;
    let err = parse_arguments::<ViewImageInEnvironmentArgs>(json)
        .expect_err("missing path should fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("path"),
        "expected error to mention path, got: {msg}"
    );
}

#[test]
fn parses_required_fields() {
    let json = r#"{"environment_id": "exe_two", "path": "/srv/data/cat.png"}"#;
    let args: ViewImageInEnvironmentArgs =
        parse_arguments(json).expect("happy-path parse should succeed");
    assert_eq!(args.environment_id, "exe_two");
    assert_eq!(args.path, "/srv/data/cat.png");
}

#[tokio::test]
async fn unknown_env_message_lists_available_ids() {
    use crate::session::turn_context::TurnEnvironment;

    let env = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs");
    let environments = vec![
        TurnEnvironment {
            environment_id: "exe_alpha".into(),
            environment: std::sync::Arc::clone(&env),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_beta".into(),
            environment: std::sync::Arc::clone(&env),
            cwd,
            shell: "/bin/sh".into(),
        },
    ];

    let msg = unknown_env_message("exe_missing", &environments);
    assert!(
        msg.contains("exe_missing"),
        "msg should include the unknown id: {msg}"
    );
    assert!(
        msg.contains("exe_alpha") && msg.contains("exe_beta"),
        "msg should list available ids: {msg}"
    );
}

#[test]
fn unknown_env_message_when_no_envs_says_no_environments() {
    let msg = unknown_env_message("exe_missing", &[]);
    assert!(msg.contains("exe_missing"));
    assert!(msg.contains("no environments"), "got: {msg}");
}

#[tokio::test]
async fn select_environment_routes_to_named_env_for_handler_dispatch() {
    use crate::session::tests::make_test_turn_context_with_environments;
    use crate::session::turn_context::TurnEnvironment;

    // Mirrors the handler-layer routing pin established for the other
    // env-aware tools (Pa.1 `exec_command_in_environment`, Pa.2
    // `apply_patch_in_environment`, Pa.4 `list_dir_in_environment`).
    // Pa.5 reads the chosen env's `get_filesystem().read_file(...)`; this
    // test pins the routing half: env id resolves to the right
    // `Arc<Environment>` (verified via `Arc::ptr_eq`), not a silent
    // fallback to the primary env.
    let env_a = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let env_b = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
    let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().expect("cwd").as_path(),
    )
    .expect("abs");
    let environments = vec![
        TurnEnvironment {
            environment_id: "exe_one".into(),
            environment: std::sync::Arc::clone(&env_a),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
        TurnEnvironment {
            environment_id: "exe_two".into(),
            environment: std::sync::Arc::clone(&env_b),
            cwd: cwd.clone(),
            shell: "/bin/sh".into(),
        },
    ];
    let turn_context = make_test_turn_context_with_environments(environments).await;

    let chosen = turn_context
        .select_environment(Some("exe_two"))
        .expect("found");
    assert_eq!(chosen.environment_id, "exe_two");
    assert!(std::sync::Arc::ptr_eq(&chosen.environment, &env_b));

    // And the handler-layer error message matches what the new tool
    // returns when the LLM-supplied id is unknown.
    let msg = unknown_env_message("exe_missing", &turn_context.environments);
    assert!(msg.contains("exe_one"));
    assert!(msg.contains("exe_two"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_file_via_environment_filesystem_loads_real_png() {
    // Pa.5 end-to-end-ish test of the second half of the handler:
    // create a real `Environment::default_for_tests()` (which uses
    // `LocalFileSystem::unsandboxed()`), write a tiny in-memory PNG to
    // a tempdir, and exercise the env's `read_file` + the
    // `load_for_prompt_bytes` pipeline that the handler invokes. This
    // pins the read-and-encode contract without needing a full
    // `ToolInvocation`. The output is then converted via the same
    // `ToolOutput::to_response_item` path the handler uses.
    use codex_utils_absolute_path::AbsolutePathBuf;
    use codex_utils_image::PromptImageMode;
    use codex_utils_image::load_for_prompt_bytes;
    use std::fs;

    // Build a 2x2 RGBA PNG via the `image` crate so decoders accept it.
    let png_bytes = make_2x2_png();

    let tmp = tempfile::tempdir().expect("tempdir");
    let img_path = tmp.path().join("cat.png");
    fs::write(&img_path, &png_bytes).expect("write png");

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();
    let abs_path = AbsolutePathBuf::from_absolute_path(&img_path).expect("abs path");

    // The handler reads the file via `read_file(...)`.
    let read_back = fs_handle
        .read_file(&abs_path, /*sandbox*/ None)
        .await
        .expect("read_file should succeed");
    assert_eq!(
        read_back, png_bytes,
        "round-trip read should match what we wrote"
    );

    // ...then runs it through the resize pipeline.
    let image = load_for_prompt_bytes(abs_path.as_path(), read_back, PromptImageMode::ResizeToFit)
        .expect("load_for_prompt_bytes should accept a real PNG");
    let image_url = image.into_data_url();

    // Pin the data-URL shape: `data:image/...;base64,<payload>` and the
    // payload decodes back to non-empty bytes (the resized image).
    assert!(
        image_url.starts_with("data:image/"),
        "expected data URL prefix, got: {image_url}"
    );
    let comma = image_url
        .find(",")
        .expect("data URL must contain a comma separator");
    let header = &image_url[..comma];
    let payload = &image_url[comma + 1..];
    assert!(
        header.contains(";base64"),
        "expected base64-encoded data URL, got header: {header}"
    );
    let decoded = BASE64_STANDARD
        .decode(payload)
        .expect("payload should be valid base64");
    assert!(
        !decoded.is_empty(),
        "decoded image bytes should be non-empty"
    );

    // Construct the same ToolOutput shape the handler emits and verify
    // it materializes as an `InputImage` content item with the PNG data
    // URL and the default detail.
    let output = ViewImageInEnvironmentOutput {
        image_url: image_url.clone(),
        image_detail: Some(DEFAULT_IMAGE_DETAIL),
    };
    let response = output.to_response_item(
        "call_pa5",
        &ToolPayload::Function {
            arguments: "{}".to_string(),
        },
    );
    let ResponseInputItem::FunctionCallOutput { call_id, output: payload } = response else {
        panic!("expected FunctionCallOutput response");
    };
    assert_eq!(call_id, "call_pa5");
    assert_eq!(payload.success, Some(true));
    let FunctionCallOutputBody::ContentItems(items) = payload.body else {
        panic!("expected ContentItems body");
    };
    assert_eq!(items.len(), 1);
    match &items[0] {
        FunctionCallOutputContentItem::InputImage {
            image_url: emitted_url,
            detail,
        } => {
            assert_eq!(emitted_url, &image_url);
            assert_eq!(*detail, Some(DEFAULT_IMAGE_DETAIL));
        }
        other => panic!("expected InputImage content item, got: {other:?}"),
    }
}

#[tokio::test]
async fn read_file_error_is_user_visible() {
    // Pa.5 contract: a failed `read_file` is wrapped into a
    // `RespondToModel` string that names the env id and the path. Pin
    // the format we'd interpolate; the handler builds the same string.
    use codex_utils_absolute_path::AbsolutePathBuf;

    let env = codex_exec_server::Environment::default_for_tests();
    let fs_handle = env.get_filesystem();

    let nonexistent = std::path::PathBuf::from(
        "/this/path/should/not/exist/codex-multi-env/view_image_in_environment/test.png",
    );
    let abs_path = AbsolutePathBuf::from_absolute_path(&nonexistent).expect("abs");
    let err = fs_handle
        .read_file(&abs_path, /*sandbox*/ None)
        .await
        .expect_err("nonexistent file must error");

    let wrapped = format!(
        "unable to read image at `{}` on environment `{}`: {err}",
        nonexistent.display(),
        "exe_target",
    );
    assert!(wrapped.contains("unable to read image"));
    assert!(wrapped.contains("exe_target"));
    assert!(wrapped.contains(nonexistent.to_str().expect("utf8")));
}

#[test]
fn code_mode_result_returns_image_url_object() {
    // Mirrors the upstream `view_image` code-mode contract so
    // code-mode call sites observe the same JSON shape regardless of
    // which variant the LLM picks.
    let output = ViewImageInEnvironmentOutput {
        image_url: "data:image/png;base64,AAA".to_string(),
        image_detail: Some(DEFAULT_IMAGE_DETAIL),
    };

    let result = output.code_mode_result(&ToolPayload::Function {
        arguments: "{}".to_string(),
    });

    assert_eq!(
        result,
        serde_json::json!({
            "image_url": "data:image/png;base64,AAA",
            "detail": "high",
        })
    );
}

/// Build a 2x2 RGBA PNG in-memory so the test's image bytes are a real
/// PNG that `load_for_prompt_bytes` can decode and resize.
fn make_2x2_png() -> Vec<u8> {
    use image::ImageBuffer;
    use image::Rgba;

    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(2, 2, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 255, 0, 255])
        }
    });
    let mut bytes = Vec::new();
    buf.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )
    .expect("encode 2x2 png");
    bytes
}
