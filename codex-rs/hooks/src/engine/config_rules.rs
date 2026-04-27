use std::collections::HashMap;
use std::path::Path;

use codex_config::ConfigLayerSource;
use codex_config::ConfigLayerStack;
use codex_config::ConfigLayerStackOrdering;
use codex_config::HookConfig;
use codex_config::HookConfigSelector;
use codex_config::HookConfigSource;
use codex_config::HookEventsToml;
use codex_protocol::protocol::HookEventName;
use codex_protocol::protocol::HookSource;
use codex_utils_absolute_path::AbsolutePathBuf;

#[derive(Default)]
pub(crate) struct HookConfigRules {
    entries: HashMap<HookConfigSelector, bool>,
}

impl HookConfigRules {
    pub(crate) fn from_stack(
        config_layer_stack: &ConfigLayerStack,
        warnings: &mut Vec<String>,
    ) -> Self {
        let mut rules = Self::default();
        for layer in config_layer_stack.get_layers(
            ConfigLayerStackOrdering::LowestPrecedenceFirst,
            /*include_disabled*/ true,
        ) {
            if !matches!(
                layer.name,
                ConfigLayerSource::User { .. } | ConfigLayerSource::SessionFlags
            ) {
                continue;
            }

            let Some(hooks_value) = layer.config.get("hooks") else {
                continue;
            };
            let hooks: HookEventsToml = match hooks_value.clone().try_into() {
                Ok(hooks) => hooks,
                Err(err) => {
                    warnings.push(format!("failed to parse TOML hooks config: {err}"));
                    continue;
                }
            };

            for entry in hooks.config {
                rules.append(entry, warnings);
            }
        }

        rules
    }

    pub(crate) fn enabled_for_hook(
        &self,
        source: HookSource,
        plugin_id: Option<&str>,
        source_path: &AbsolutePathBuf,
        key: &str,
    ) -> bool {
        hook_config_selector_for_hook(source, plugin_id, source_path, key)
            .map(|selector| self.enabled_for_selector(selector))
            .unwrap_or_else(default_hook_enabled)
    }

    fn enabled_for_selector(&self, selector: HookConfigSelector) -> bool {
        self.entries
            .get(&normalize_hook_config_selector(selector))
            .copied()
            .unwrap_or_else(default_hook_enabled)
    }

    fn append(&mut self, entry: HookConfig, warnings: &mut Vec<String>) {
        let enabled = entry.enabled;
        let Some(selector) = hook_config_selector_for_config(entry, warnings) else {
            return;
        };
        self.entries
            .insert(normalize_hook_config_selector(selector), enabled);
    }
}

fn default_hook_enabled() -> bool {
    // TODO(abhinav): Default-enabled hooks are temporary until hook trust is added.
    true
}

fn hook_config_source_for_hook_source(source: HookSource) -> Option<HookConfigSource> {
    match source {
        HookSource::User => Some(HookConfigSource::User),
        HookSource::Project => Some(HookConfigSource::Project),
        HookSource::System
        | HookSource::Mdm
        | HookSource::SessionFlags
        | HookSource::Plugin
        | HookSource::LegacyManagedConfigFile
        | HookSource::LegacyManagedConfigMdm
        | HookSource::Unknown => None,
    }
}

fn hook_config_selector_for_hook(
    source: HookSource,
    plugin_id: Option<&str>,
    source_path: &AbsolutePathBuf,
    key: &str,
) -> Option<HookConfigSelector> {
    match source {
        HookSource::Plugin => plugin_id.map(|plugin_id| HookConfigSelector::Plugin {
            plugin_id: plugin_id.to_string(),
            key: key.to_string(),
        }),
        HookSource::User | HookSource::Project => {
            hook_config_source_for_hook_source(source).map(|source| match source {
                HookConfigSource::User => HookConfigSelector::User {
                    source_path: source_path.as_path().to_path_buf(),
                    key: key.to_string(),
                },
                HookConfigSource::Project => HookConfigSelector::Project {
                    source_path: source_path.as_path().to_path_buf(),
                    key: key.to_string(),
                },
                HookConfigSource::Plugin => unreachable!("plugin hook source handled above"),
            })
        }
        HookSource::System
        | HookSource::Mdm
        | HookSource::SessionFlags
        | HookSource::LegacyManagedConfigFile
        | HookSource::LegacyManagedConfigMdm
        | HookSource::Unknown => None,
    }
}

fn hook_config_selector_for_config(
    entry: HookConfig,
    warnings: &mut Vec<String>,
) -> Option<HookConfigSelector> {
    if entry.key.trim().is_empty() {
        warnings.push("ignoring hooks.config entry with empty key".to_string());
        return None;
    }
    match entry.source {
        HookConfigSource::Plugin => {
            let Some(plugin_id) = entry.plugin_id else {
                warnings.push(
                    "ignoring plugin hooks.config entry without a plugin_id selector".to_string(),
                );
                return None;
            };
            if plugin_id.trim().is_empty() {
                warnings
                    .push("ignoring plugin hooks.config entry with empty plugin_id".to_string());
                return None;
            }
            Some(HookConfigSelector::Plugin {
                plugin_id,
                key: entry.key,
            })
        }
        HookConfigSource::User | HookConfigSource::Project => {
            let Some(source_path) = entry.source_path else {
                warnings.push(format!(
                    "ignoring {} hooks.config entry without a source_path selector",
                    hook_config_source_label(entry.source)
                ));
                return None;
            };
            if source_path.as_os_str().is_empty() {
                warnings.push(format!(
                    "ignoring {} hooks.config entry with empty source_path",
                    hook_config_source_label(entry.source)
                ));
                return None;
            }
            match entry.source {
                HookConfigSource::User => Some(HookConfigSelector::User {
                    source_path,
                    key: entry.key,
                }),
                HookConfigSource::Project => Some(HookConfigSelector::Project {
                    source_path,
                    key: entry.key,
                }),
                HookConfigSource::Plugin => unreachable!("plugin config source handled above"),
            }
        }
    }
}

fn hook_config_source_label(source: HookConfigSource) -> &'static str {
    match source {
        HookConfigSource::Plugin => "plugin",
        HookConfigSource::User => "user",
        HookConfigSource::Project => "project",
    }
}

fn normalize_hook_config_selector(selector: HookConfigSelector) -> HookConfigSelector {
    match selector {
        HookConfigSelector::Plugin { plugin_id, key } => HookConfigSelector::Plugin {
            plugin_id: plugin_id.trim().to_string(),
            key: key.trim().to_string(),
        },
        HookConfigSelector::User { source_path, key } => HookConfigSelector::User {
            source_path: normalize_source_path(&source_path),
            key: key.trim().to_string(),
        },
        HookConfigSelector::Project { source_path, key } => HookConfigSelector::Project {
            source_path: normalize_source_path(&source_path),
            key: key.trim().to_string(),
        },
    }
}

fn normalize_source_path(path: &Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn hook_config_key(
    source_relative_path: &str,
    event_name: HookEventName,
    group_index: usize,
    handler_index: usize,
) -> String {
    format!(
        "{}:{}:{}:{}",
        source_relative_path,
        hook_event_name_config_label(event_name),
        group_index,
        handler_index
    )
}

pub(crate) fn local_hook_config_key(
    event_name: HookEventName,
    group_index: usize,
    handler_index: usize,
) -> String {
    format!(
        "{}:{}:{}",
        hook_event_name_config_label(event_name),
        group_index,
        handler_index
    )
}

fn hook_event_name_config_label(event_name: HookEventName) -> &'static str {
    match event_name {
        HookEventName::PreToolUse => "PreToolUse",
        HookEventName::PermissionRequest => "PermissionRequest",
        HookEventName::PostToolUse => "PostToolUse",
        HookEventName::SessionStart => "SessionStart",
        HookEventName::UserPromptSubmit => "UserPromptSubmit",
        HookEventName::Stop => "Stop",
    }
}
