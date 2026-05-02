use crate::legacy::LegacyFeatureToggles;
use crate::registry::FEATURES;
use crate::registry::FeatureSpec;
use codex_otel::SessionTelemetry;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::WarningEvent;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use toml::Table;
use toml::Value as TomlValue;

/// High-level lifecycle stage for a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Features that are still under development, not ready for external use
    UnderDevelopment,
    /// Experimental features made available to users through the `/experimental` menu
    Experimental {
        name: &'static str,
        menu_description: &'static str,
        announcement: &'static str,
    },
    /// Stable features. The feature flag is kept for ad-hoc enabling/disabling
    Stable,
    /// Deprecated feature that should not be used anymore.
    Deprecated,
    /// The feature flag is useless but kept for backward compatibility reason.
    Removed,
}

impl Stage {
    pub fn experimental_menu_name(self) -> Option<&'static str> {
        match self {
            Stage::Experimental { name, .. } => Some(name),
            Stage::UnderDevelopment | Stage::Stable | Stage::Deprecated | Stage::Removed => None,
        }
    }

    pub fn experimental_menu_description(self) -> Option<&'static str> {
        match self {
            Stage::Experimental {
                menu_description, ..
            } => Some(menu_description),
            Stage::UnderDevelopment | Stage::Stable | Stage::Deprecated | Stage::Removed => None,
        }
    }

    pub fn experimental_announcement(self) -> Option<&'static str> {
        match self {
            Stage::Experimental {
                announcement: "", ..
            } => None,
            Stage::Experimental { announcement, .. } => Some(announcement),
            Stage::UnderDevelopment | Stage::Stable | Stage::Deprecated | Stage::Removed => None,
        }
    }
}

/// Unique features toggled via configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    // Stable.
    /// Removed compatibility flag retained as a no-op so old configs can
    /// still parse `undo`.
    GhostCommit,
    /// Enable the default shell tool.
    ShellTool,
    /// Enable Claude-style lifecycle hooks loaded from hooks.json files.
    CodexHooks,

    // Experimental
    /// Removed compatibility flag for the deleted JavaScript REPL feature.
    JsRepl,
    /// Enable JavaScript code mode backed by the in-process V8 runtime.
    CodeMode,
    /// Restrict model-visible tools to code mode entrypoints (`exec`, `wait`).
    CodeModeOnly,
    /// Removed compatibility flag for the deleted JavaScript REPL tool-only mode.
    JsReplToolsOnly,
    /// Use the single unified PTY-backed exec tool.
    UnifiedExec,
    /// Route shell tool execution through the zsh exec bridge.
    ShellZshFork,
    /// Reflow transcript scrollback when the terminal is resized.
    TerminalResizeReflow,
    /// Include the freeform apply_patch tool.
    ApplyPatchFreeform,
    /// Stream structured progress while apply_patch input is being generated.
    ApplyPatchStreamingEvents,
    /// Allow exec tools to request additional permissions while staying sandboxed.
    ExecPermissionApprovals,
    /// Expose the built-in request_permissions tool.
    RequestPermissionsTool,
    /// Allow the model to request web searches that fetch live content.
    WebSearchRequest,
    /// Allow the model to request web searches that fetch cached content.
    /// Takes precedence over `WebSearchRequest`.
    WebSearchCached,
    /// Legacy search-tool feature flag kept for backward compatibility.
    SearchTool,
    /// Removed legacy Linux bubblewrap opt-in flag retained as a no-op so old
    /// wrappers and config can still parse it.
    UseLinuxSandboxBwrap,
    /// Use the legacy Landlock Linux sandbox fallback instead of the default
    /// bubblewrap pipeline.
    UseLegacyLandlock,
    /// Allow the model to request approval and propose exec rules.
    RequestRule,
    /// Enable Windows sandbox (restricted token) on Windows.
    WindowsSandbox,
    /// Use the elevated Windows sandbox pipeline (setup + runner).
    WindowsSandboxElevated,
    /// Legacy remote models flag kept for backward compatibility.
    RemoteModels,
    /// Experimental shell snapshotting.
    ShellSnapshot,
    /// Enable git commit attribution guidance via model instructions.
    CodexGitCommit,
    /// Enable runtime metrics snapshots via a manual reader.
    RuntimeMetrics,
    /// Persist rollout metadata to a local SQLite database.
    Sqlite,
    /// Enable startup memory extraction and file-backed memory consolidation.
    MemoryTool,
    /// Enable the Chronicle sidecar for passive screen-context memories.
    Chronicle,
    /// Append additional AGENTS.md guidance to user instructions.
    ChildAgentsMd,
    /// Compress request bodies (zstd) when sending streaming requests to codex-backend.
    EnableRequestCompression,
    /// Enable collab tools.
    Collab,
    /// Enable task-path-based multi-agent routing.
    MultiAgentV2,
    /// Enable CSV-backed agent job tools.
    SpawnCsv,
    /// Enable apps.
    Apps,
    /// Enable MCP apps.
    EnableMcpApps,
    /// Use the new path for the built-in apps MCP server.
    AppsMcpPathOverride,
    /// Enable the tool_search tool for apps.
    ToolSearch,
    /// Always defer MCP tools behind tool_search instead of exposing small sets directly.
    ToolSearchAlwaysDeferMcpTools,
    /// Expose placeholder tools for unavailable historical tool calls.
    UnavailableDummyTools,
    /// Enable discoverable tool suggestions for apps.
    ToolSuggest,
    /// Enable plugins.
    Plugins,
    /// Enable plugin-bundled lifecycle hooks.
    PluginHooks,
    /// Allow the in-app browser pane in desktop apps.
    ///
    /// Requirements-only gate: this should be set from requirements, not user config.
    InAppBrowser,
    /// Allow Browser Use agent integration in desktop apps.
    ///
    /// Requirements-only gate: this should be set from requirements, not user config.
    BrowserUse,
    /// Allow Browser Use integration with external browsers.
    ///
    /// Requirements-only gate: this should be set from requirements, not user config.
    BrowserUseExternal,
    /// Allow Codex Computer Use.
    ///
    /// Requirements-only gate: this should be set from requirements, not user config.
    ComputerUse,
    /// Temporary internal-only flag for PS-backed remote plugin catalog development.
    RemotePlugin,
    /// Show the startup prompt for migrating external agent config into Codex.
    ExternalMigration,
    /// Allow the model to invoke the built-in image generation tool.
    ImageGeneration,
    /// Allow prompting and installing missing MCP dependencies.
    SkillMcpDependencyInstall,
    /// Prompt for missing skill env var dependencies.
    SkillEnvVarDependencyPrompt,
    /// Steer feature flag - when enabled, Enter submits immediately instead of queuing.
    /// Kept for config backward compatibility; behavior is always steer-enabled.
    Steer,
    /// Allow request_user_input in Default collaboration mode.
    DefaultModeRequestUserInput,
    /// Enable automatic review for approval prompts.
    GuardianApproval,
    /// Enable persisted thread goals and automatic goal continuation.
    Goals,
    /// Enable collaboration modes (Plan, Default).
    /// Kept for config backward compatibility; behavior is always collaboration-modes-enabled.
    CollaborationModes,
    /// Route MCP tool approval prompts through the MCP elicitation request path.
    ToolCallMcpElicitation,
    /// Enable personality selection in the TUI.
    Personality,
    /// Enable native artifact tools.
    Artifact,
    /// Enable Fast mode selection in the TUI and request layer.
    FastMode,
    /// Enable experimental realtime voice conversation mode in the TUI.
    RealtimeConversation,
    /// Connect app-server to the ChatGPT remote control service.
    RemoteControl,
    /// Removed compatibility flag retained as a no-op so old wrappers can
    /// still pass `--enable image_detail_original`.
    ImageDetailOriginal,
    /// Removed compatibility flag. The TUI now always uses the app-server implementation.
    TuiAppServer,
    /// Prevent idle system sleep while a turn is actively running.
    PreventIdleSleep,
    /// Enable workspace-specific owner nudge copy and prompts in the TUI.
    WorkspaceOwnerUsageNudge,
    /// Legacy rollout flag for Responses API WebSocket transport experiments.
    ResponsesWebsockets,
    /// Legacy rollout flag for Responses API WebSocket transport v2 experiments.
    ResponsesWebsocketsV2,
    /// Enable workspace dependency support.
    WorkspaceDependencies,
}

impl Feature {
    pub fn key(self) -> &'static str {
        self.info().key
    }

    pub fn stage(self) -> Stage {
        self.info().stage
    }

    pub fn default_enabled(self) -> bool {
        self.info().default_enabled
    }

    pub(crate) fn info(self) -> &'static FeatureSpec {
        FEATURES
            .iter()
            .find(|spec| spec.id == self)
            .unwrap_or_else(|| unreachable!("missing FeatureSpec for {self:?}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LegacyFeatureUsage {
    pub alias: String,
    pub feature: Feature,
    pub summary: String,
    pub details: Option<String>,
}

/// Holds the effective set of enabled features.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Features {
    enabled: BTreeSet<Feature>,
    legacy_usages: BTreeSet<LegacyFeatureUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureOverrides {
    pub include_apply_patch_tool: Option<bool>,
    pub web_search_request: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureConfigSource<'a> {
    pub features: Option<&'a FeaturesToml>,
    pub include_apply_patch_tool: Option<bool>,
    pub experimental_use_freeform_apply_patch: Option<bool>,
    pub experimental_use_unified_exec_tool: Option<bool>,
}

impl FeatureOverrides {
    fn apply(self, features: &mut Features) {
        LegacyFeatureToggles {
            include_apply_patch_tool: self.include_apply_patch_tool,
            ..Default::default()
        }
        .apply(features);
        if let Some(enabled) = self.web_search_request {
            if enabled {
                features.enable(Feature::WebSearchRequest);
            } else {
                features.disable(Feature::WebSearchRequest);
            }
            features.record_legacy_usage("web_search_request", Feature::WebSearchRequest);
        }
    }
}

impl Features {
    /// Starts with built-in defaults.
    pub fn with_defaults() -> Self {
        let mut set = BTreeSet::new();
        for spec in FEATURES {
            if spec.default_enabled {
                set.insert(spec.id);
            }
        }
        Self {
            enabled: set,
            legacy_usages: BTreeSet::new(),
        }
    }

    pub fn enabled(&self, f: Feature) -> bool {
        self.enabled.contains(&f)
    }

    pub fn apps_enabled_for_auth(&self, has_chatgpt_auth: bool) -> bool {
        self.enabled(Feature::Apps) && has_chatgpt_auth
    }

    pub fn use_legacy_landlock(&self) -> bool {
        self.enabled(Feature::UseLegacyLandlock)
    }

    pub fn enable(&mut self, f: Feature) -> &mut Self {
        self.enabled.insert(f);
        self
    }

    pub fn disable(&mut self, f: Feature) -> &mut Self {
        self.enabled.remove(&f);
        self
    }

    pub fn set_enabled(&mut self, f: Feature, enabled: bool) -> &mut Self {
        if enabled {
            self.enable(f)
        } else {
            self.disable(f)
        }
    }

    pub fn record_legacy_usage_force(&mut self, alias: &str, feature: Feature) {
        let (summary, details) = legacy_usage_notice(alias, feature);
        self.legacy_usages.insert(LegacyFeatureUsage {
            alias: alias.to_string(),
            feature,
            summary,
            details,
        });
    }

    pub fn record_legacy_usage(&mut self, alias: &str, feature: Feature) {
        if alias == feature.key() {
            return;
        }
        self.record_legacy_usage_force(alias, feature);
    }

    pub fn legacy_feature_usages(&self) -> impl Iterator<Item = &LegacyFeatureUsage> + '_ {
        self.legacy_usages.iter()
    }

    pub fn emit_metrics(&self, otel: &SessionTelemetry) {
        for feature in FEATURES {
            if matches!(feature.stage, Stage::Removed) {
                continue;
            }
            if self.enabled(feature.id) != feature.default_enabled {
                otel.counter(
                    "codex.feature.state",
                    /*inc*/ 1,
                    &[
                        ("feature", feature.key),
                        ("value", &self.enabled(feature.id).to_string()),
                    ],
                );
            }
        }
    }

    /// Apply a table of key -> bool toggles (e.g. from TOML).
    pub fn apply_map(&mut self, entries: &BTreeMap<String, bool>) {
        for (key, enabled) in entries {
            match key.as_str() {
                "web_search_request" => {
                    self.record_legacy_usage_force(
                        "features.web_search_request",
                        Feature::WebSearchRequest,
                    );
                }
                "web_search_cached" => {
                    self.record_legacy_usage_force(
                        "features.web_search_cached",
                        Feature::WebSearchCached,
                    );
                }
                "tui_app_server"
                | "undo"
                | "js_repl"
                | "js_repl_tools_only"
                | "image_detail_original" => {
                    continue;
                }
                "use_legacy_landlock" => {
                    self.record_legacy_usage_force(
                        "features.use_legacy_landlock",
                        Feature::UseLegacyLandlock,
                    );
                }
                _ => {}
            }
            match feature_for_key(key) {
                Some(feature) => {
                    if matches!(feature, Feature::TuiAppServer) {
                        continue;
                    }
                    if key != feature.key() {
                        self.record_legacy_usage(key.as_str(), feature);
                    }
                    self.set_enabled(feature, *enabled);
                }
                None => {
                    tracing::warn!("unknown feature key in config: {key}");
                }
            }
        }
    }

    pub fn from_sources(
        base: FeatureConfigSource<'_>,
        profile: FeatureConfigSource<'_>,
        overrides: FeatureOverrides,
    ) -> Self {
        let mut features = Features::with_defaults();

        for source in [base, profile] {
            LegacyFeatureToggles {
                include_apply_patch_tool: source.include_apply_patch_tool,
                experimental_use_freeform_apply_patch: source.experimental_use_freeform_apply_patch,
                experimental_use_unified_exec_tool: source.experimental_use_unified_exec_tool,
            }
            .apply(&mut features);

            if let Some(feature_entries) = source.features {
                features.apply_toml(feature_entries);
            }
        }

        overrides.apply(&mut features);
        features.normalize_dependencies();

        features
    }

    pub fn enabled_features(&self) -> Vec<Feature> {
        self.enabled.iter().copied().collect()
    }

    pub fn normalize_dependencies(&mut self) {
        if self.enabled(Feature::SpawnCsv) && !self.enabled(Feature::Collab) {
            self.enable(Feature::Collab);
        }
        if self.enabled(Feature::CodeModeOnly) && !self.enabled(Feature::CodeMode) {
            self.enable(Feature::CodeMode);
        }
    }

    fn apply_toml(&mut self, features: &FeaturesToml) {
        let entries = features.entries();
        self.apply_map(&entries);
    }
}

fn legacy_usage_notice(alias: &str, feature: Feature) -> (String, Option<String>) {
    let canonical = feature.key();
    match feature {
        Feature::WebSearchRequest | Feature::WebSearchCached => {
            let label = match alias {
                "web_search" => "[features].web_search",
                "features.web_search_request" | "web_search_request" => {
                    "[features].web_search_request"
                }
                "features.web_search_cached" | "web_search_cached" => {
                    "[features].web_search_cached"
                }
                _ => alias,
            };
            let summary =
                format!("`{label}` is deprecated because web search is enabled by default.");
            (summary, Some(web_search_details().to_string()))
        }
        Feature::UseLegacyLandlock => {
            let label = match alias {
                "features.use_legacy_landlock" | "use_legacy_landlock" => {
                    "[features].use_legacy_landlock"
                }
                _ => alias,
            };
            let summary = format!("`{label}` is deprecated and will be removed soon.");
            let details =
                "Remove this setting to stop opting into the legacy Linux sandbox behavior."
                    .to_string();
            (summary, Some(details))
        }
        _ => {
            let label = if alias.contains('.') || alias.starts_with('[') {
                alias.to_string()
            } else {
                format!("[features].{alias}")
            };
            let summary = format!("`{label}` is deprecated. Use `[features].{canonical}` instead.");
            let details = if alias == canonical {
                None
            } else {
                Some(format!(
                    "Enable it with `--enable {canonical}` or `[features].{canonical}` in config.toml. See https://developers.openai.com/codex/config-basic#feature-flags for details."
                ))
            };
            (summary, details)
        }
    }
}

fn web_search_details() -> &'static str {
    "Set `web_search` to `\"live\"`, `\"cached\"`, or `\"disabled\"` at the top level (or under a profile) in config.toml if you want to override it."
}

/// Keys accepted in `[features]` tables.
pub fn feature_for_key(key: &str) -> Option<Feature> {
    for spec in FEATURES {
        if spec.key == key {
            return Some(spec.id);
        }
    }
    crate::legacy::feature_for_key(key)
}

pub fn canonical_feature_for_key(key: &str) -> Option<Feature> {
    FEATURES
        .iter()
        .find(|spec| spec.key == key)
        .map(|spec| spec.id)
}

/// Returns `true` if the provided string matches a known feature toggle key.
pub fn is_known_feature_key(key: &str) -> bool {
    feature_for_key(key).is_some()
}

/// Deserializable features table for TOML.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct FeaturesToml {
    #[serde(flatten)]
    entries: BTreeMap<String, FeatureToml>,
}

impl FeaturesToml {
    #[cfg(test)]
    pub(crate) fn from_entries(entries: BTreeMap<String, FeatureToml>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> BTreeMap<String, bool> {
        self.entries
            .iter()
            .filter_map(|(key, feature)| {
                feature_enabled_in_config(key, feature).map(|enabled| (key.clone(), enabled))
            })
            .collect()
    }

    pub fn get(&self, key: &str) -> Option<&FeatureToml> {
        self.entries.get(key)
    }

    pub fn typed_config<T>(
        &self,
        key: &str,
    ) -> Option<Result<FeatureConfigTable<T>, toml::de::Error>>
    where
        T: DeserializeOwned,
    {
        self.get(key).and_then(FeatureToml::typed_config::<T>)
    }

    pub fn hint(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(FeatureToml::hint)
    }

    pub fn insert(&mut self, key: String, feature: FeatureToml) {
        self.entries.insert(key, feature);
    }

    pub fn materialize_resolved_enabled(&mut self, features: &Features) {
        for key in crate::legacy::legacy_feature_keys() {
            self.entries.remove(key);
        }
        for spec in FEATURES {
            let enabled = features.enabled(spec.id);
            materialize_resolved_feature_enabled(&mut self.entries, spec.key, enabled);
        }
    }

    pub fn materialize_resolved_config<T>(
        &mut self,
        feature: Feature,
        enabled: bool,
        extra: T,
    ) -> Result<(), toml::ser::Error>
    where
        T: Serialize,
    {
        let key = feature.key().to_string();
        let hint = self.hint(&key).map(ToOwned::to_owned);
        let feature = FeatureToml::Config(
            FeatureConfigTable {
                common: CommonFeatureConfigToml {
                    enabled: Some(enabled),
                    hint,
                },
                extra,
            }
            .into_raw()?,
        );
        self.insert(key, feature);
        Ok(())
    }
}

fn materialize_resolved_feature_enabled(
    features: &mut BTreeMap<String, FeatureToml>,
    key: &str,
    enabled: bool,
) {
    match features.get_mut(key) {
        Some(feature) => feature.set_enabled(enabled),
        None => {
            features.insert(key.to_string(), FeatureToml::Enabled(enabled));
        }
    }
}

fn feature_enabled_in_config(key: &str, feature: &FeatureToml) -> Option<bool> {
    match feature {
        FeatureToml::Enabled(enabled) => Some(*enabled),
        FeatureToml::Config(config) => config.common.enabled.or_else(|| {
            if key == Feature::AppsMcpPathOverride.key() {
                config.extra.contains_key("path").then_some(true)
            } else {
                None
            }
        }),
    }
}

impl From<BTreeMap<String, bool>> for FeaturesToml {
    fn from(entries: BTreeMap<String, bool>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(key, enabled)| (key, FeatureToml::Enabled(enabled)))
                .collect(),
        }
    }
}

pub type RawFeatureConfigExtras = BTreeMap<String, TomlValue>;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommonFeatureConfigToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoExtraFeatureConfigToml {}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
pub struct FeatureConfigTable<T = RawFeatureConfigExtras> {
    #[serde(flatten)]
    pub common: CommonFeatureConfigToml,
    #[serde(flatten)]
    pub extra: T,
}

// To be used for feature entries under `[features]` that can be either a bare
// boolean toggle or a table with shared fields plus feature-specific extras.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(untagged)]
pub enum FeatureToml<T = RawFeatureConfigExtras> {
    Enabled(bool),
    Config(FeatureConfigTable<T>),
}

impl<T> FeatureToml<T> {
    pub fn enabled(&self) -> Option<bool> {
        match self {
            Self::Enabled(enabled) => Some(*enabled),
            Self::Config(config) => config.common.enabled,
        }
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Enabled(_) => None,
            Self::Config(config) => config.common.hint.as_deref(),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        match self {
            Self::Enabled(value) => *value = enabled,
            Self::Config(config) => config.common.enabled = Some(enabled),
        }
    }
}

impl FeatureToml<RawFeatureConfigExtras> {
    pub fn typed_config<T>(&self) -> Option<Result<FeatureConfigTable<T>, toml::de::Error>>
    where
        T: DeserializeOwned,
    {
        match self {
            Self::Enabled(_) => None,
            Self::Config(config) => Some(config.clone().typed()),
        }
    }
}

impl<T> FeatureConfigTable<T> {
    pub fn into_raw(self) -> Result<FeatureConfigTable<RawFeatureConfigExtras>, toml::ser::Error>
    where
        T: Serialize,
    {
        let extra = match TomlValue::try_from(self.extra)? {
            TomlValue::Table(table) => table.into_iter().collect(),
            other => {
                unreachable!("feature config extras must serialize as a TOML table: {other:?}")
            }
        };
        Ok(FeatureConfigTable {
            common: self.common,
            extra,
        })
    }
}

impl FeatureConfigTable<RawFeatureConfigExtras> {
    pub fn typed<T>(self) -> Result<FeatureConfigTable<T>, toml::de::Error>
    where
        T: DeserializeOwned,
    {
        Ok(FeatureConfigTable {
            common: self.common,
            extra: TomlValue::Table(self.extra.into_iter().collect()).try_into()?,
        })
    }
}

pub fn unstable_features_warning_event(
    effective_features: Option<&Table>,
    suppress_unstable_features_warning: bool,
    features: &Features,
    config_path: &str,
) -> Option<Event> {
    if suppress_unstable_features_warning {
        return None;
    }

    let mut under_development_feature_keys = Vec::new();
    if let Some(table) = effective_features {
        for (key, value) in table {
            if configured_feature_enabled_in_effective_table(key, value) != Some(true) {
                continue;
            }
            let Some(spec) = FEATURES.iter().find(|spec| spec.key == key.as_str()) else {
                continue;
            };
            if !features.enabled(spec.id) {
                continue;
            }
            if matches!(spec.stage, Stage::UnderDevelopment) {
                under_development_feature_keys.push(spec.key.to_string());
            }
        }
    }

    if under_development_feature_keys.is_empty() {
        return None;
    }

    let under_development_feature_keys = under_development_feature_keys.join(", ");
    let message = format!(
        "Under-development features enabled: {under_development_feature_keys}. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in {config_path}."
    );
    Some(Event {
        id: String::new(),
        msg: EventMsg::Warning(WarningEvent { message }),
    })
}

fn configured_feature_enabled_in_effective_table(key: &str, value: &TomlValue) -> Option<bool> {
    let feature: FeatureToml = value.clone().try_into().ok()?;
    feature_enabled_in_config(key, &feature)
}
