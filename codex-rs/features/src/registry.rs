use crate::machinery::Feature;
use crate::machinery::Stage;

/// Single, easy-to-read registry of all feature definitions.
#[derive(Debug, Clone, Copy)]
pub struct FeatureSpec {
    pub id: Feature,
    pub key: &'static str,
    pub stage: Stage,
    pub default_enabled: bool,
}

pub const FEATURES: &[FeatureSpec] = &[
    // Stable features.
    FeatureSpec {
        id: Feature::GhostCommit,
        key: "undo",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ShellTool,
        key: "shell_tool",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::UnifiedExec,
        key: "unified_exec",
        stage: Stage::Stable,
        default_enabled: !cfg!(windows),
    },
    FeatureSpec {
        id: Feature::ShellZshFork,
        key: "shell_zsh_fork",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ShellSnapshot,
        key: "shell_snapshot",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::JsRepl,
        key: "js_repl",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::CodeMode,
        key: "code_mode",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::CodeModeOnly,
        key: "code_mode_only",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::JsReplToolsOnly,
        key: "js_repl_tools_only",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::TerminalResizeReflow,
        key: "terminal_resize_reflow",
        stage: Stage::Experimental {
            name: "Terminal resize reflow",
            menu_description: "Rebuild Codex-owned transcript scrollback when the terminal width changes.",
            announcement: "",
        },
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::WebSearchRequest,
        key: "web_search_request",
        stage: Stage::Deprecated,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WebSearchCached,
        key: "web_search_cached",
        stage: Stage::Deprecated,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::SearchTool,
        key: "search_tool",
        stage: Stage::Removed,
        default_enabled: false,
    },
    // Experimental program. Rendered in the `/experimental` menu for users.
    FeatureSpec {
        id: Feature::CodexGitCommit,
        key: "codex_git_commit",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::RuntimeMetrics,
        key: "runtime_metrics",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::Sqlite,
        key: "sqlite",
        stage: Stage::Removed,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::MemoryTool,
        key: "memories",
        stage: Stage::Experimental {
            name: "Memories",
            menu_description: "Allow Codex to create new memories from conversations and bring relevant memories into new conversations.",
            announcement: "NEW: Codex can now generate and uses memories. Try is now with `/memories`",
        },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::Chronicle,
        key: "chronicle",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ChildAgentsMd,
        key: "child_agents_md",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ApplyPatchFreeform,
        key: "apply_patch_freeform",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ApplyPatchStreamingEvents,
        key: "apply_patch_streaming_events",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ExecPermissionApprovals,
        key: "exec_permission_approvals",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::CodexHooks,
        key: "hooks",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::RequestPermissionsTool,
        key: "request_permissions_tool",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::UseLinuxSandboxBwrap,
        key: "use_linux_sandbox_bwrap",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::UseLegacyLandlock,
        key: "use_legacy_landlock",
        stage: Stage::Deprecated,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::RequestRule,
        key: "request_rule",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WindowsSandbox,
        key: "experimental_windows_sandbox",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WindowsSandboxElevated,
        key: "elevated_windows_sandbox",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::RemoteModels,
        key: "remote_models",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::EnableRequestCompression,
        key: "enable_request_compression",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Collab,
        key: "multi_agent",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::MultiAgentV2,
        key: "multi_agent_v2",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::SpawnCsv,
        key: "enable_fanout",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::Apps,
        key: "apps",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::EnableMcpApps,
        key: "enable_mcp_apps",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::AppsMcpPathOverride,
        key: "apps_mcp_path_override",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ToolSearch,
        key: "tool_search",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ToolSearchAlwaysDeferMcpTools,
        key: "tool_search_always_defer_mcp_tools",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::UnavailableDummyTools,
        key: "unavailable_dummy_tools",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ToolSuggest,
        key: "tool_suggest",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Plugins,
        key: "plugins",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::PluginHooks,
        key: "plugin_hooks",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::InAppBrowser,
        key: "in_app_browser",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::BrowserUse,
        key: "browser_use",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::BrowserUseExternal,
        key: "browser_use_external",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ComputerUse,
        key: "computer_use",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::RemotePlugin,
        key: "remote_plugin",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ExternalMigration,
        key: "external_migration",
        stage: Stage::Experimental {
            name: "External migration",
            menu_description: "Show a startup prompt when Codex detects migratable external agent config for this machine or project.",
            announcement: "",
        },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ImageGeneration,
        key: "image_generation",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::SkillMcpDependencyInstall,
        key: "skill_mcp_dependency_install",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::SkillEnvVarDependencyPrompt,
        key: "skill_env_var_dependency_prompt",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::Steer,
        key: "steer",
        stage: Stage::Removed,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::DefaultModeRequestUserInput,
        key: "default_mode_request_user_input",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::GuardianApproval,
        key: "guardian_approval",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Goals,
        key: "goals",
        stage: Stage::Experimental {
            name: "Goals",
            menu_description: "Set a persistent goal Codex can continue over time",
            announcement: "",
        },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::CollaborationModes,
        key: "collaboration_modes",
        stage: Stage::Removed,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ToolCallMcpElicitation,
        key: "tool_call_mcp_elicitation",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Personality,
        key: "personality",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Artifact,
        key: "artifact",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::FastMode,
        key: "fast_mode",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::RealtimeConversation,
        key: "realtime_conversation",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::RemoteControl,
        key: "remote_control",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ImageDetailOriginal,
        key: "image_detail_original",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::TuiAppServer,
        key: "tui_app_server",
        stage: Stage::Removed,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::PreventIdleSleep,
        key: "prevent_idle_sleep",
        stage: if cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )) {
            Stage::Experimental {
                name: "Prevent sleep while running",
                menu_description: "Keep your computer awake while Codex is running a thread.",
                announcement: "NEW: Prevent sleep while running is now available in /experimental.",
            }
        } else {
            Stage::UnderDevelopment
        },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WorkspaceOwnerUsageNudge,
        key: "workspace_owner_usage_nudge",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ResponsesWebsockets,
        key: "responses_websockets",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ResponsesWebsocketsV2,
        key: "responses_websockets_v2",
        stage: Stage::Removed,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WorkspaceDependencies,
        key: "workspace_dependencies",
        stage: Stage::Stable,
        default_enabled: true,
    },
];
