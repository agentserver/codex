use codex_config::ConfigLayerStack;
use codex_config::ConfigLayerStackOrdering;
use codex_config::HookEventsToml;
use codex_config::HookHandlerConfig;
use codex_config::MatcherGroup;
use codex_plugin::PluginHookSource;
use codex_protocol::protocol::HookEventName;
use codex_protocol::protocol::HookHandlerType;
use codex_protocol::protocol::HookSource;
use codex_utils_absolute_path::AbsolutePathBuf;

use super::config_rules::HookConfigRules;
use super::config_rules::hook_config_key;
use super::config_rules::local_hook_config_key;
use super::discovery::hook_source_for_config_layer_source;
use super::discovery::hook_source_for_requirement_source;
use super::discovery::load_hooks_json;
use super::discovery::load_toml_hooks_from_layer;
use super::discovery::managed_hooks_source_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookInventoryEntry {
    pub source: HookSource,
    pub plugin_id: Option<String>,
    pub key: String,
    pub event_name: HookEventName,
    pub matcher: Option<String>,
    pub handler_type: HookHandlerType,
    pub command: Option<String>,
    pub timeout_sec: Option<u64>,
    pub status_message: Option<String>,
    pub source_path: AbsolutePathBuf,
    pub source_relative_path: Option<String>,
    pub enabled: bool,
}

#[derive(Clone)]
struct InventoryHookSource {
    source: HookSource,
    plugin_id: Option<String>,
    source_path: AbsolutePathBuf,
    source_relative_path: Option<String>,
}

pub fn list_hooks(
    config_layer_stack: Option<&ConfigLayerStack>,
    plugin_hook_sources: &[PluginHookSource],
) -> Vec<HookInventoryEntry> {
    let mut warnings = Vec::new();
    let hook_config_rules = config_layer_stack
        .map(|config_layer_stack| HookConfigRules::from_stack(config_layer_stack, &mut warnings))
        .unwrap_or_default();
    let mut entries = Vec::new();

    if let Some(config_layer_stack) = config_layer_stack {
        if let Some(managed_hooks) = config_layer_stack.requirements().managed_hooks.as_ref()
            && let Some(source_path) = managed_hooks_source_path(
                managed_hooks.get(),
                managed_hooks.source.as_ref(),
                &mut warnings,
            )
        {
            append_hook_events(
                &mut entries,
                InventoryHookSource {
                    source: hook_source_for_requirement_source(managed_hooks.source.as_ref()),
                    plugin_id: None,
                    source_path,
                    source_relative_path: None,
                },
                managed_hooks.get().hooks.clone(),
                &hook_config_rules,
            );
        }

        for layer in config_layer_stack.get_layers(
            ConfigLayerStackOrdering::LowestPrecedenceFirst,
            /*include_disabled*/ false,
        ) {
            let hook_source = hook_source_for_config_layer_source(&layer.name);
            if let Some((source_path, hook_events)) =
                load_hooks_json(layer.config_folder().as_deref(), &mut warnings)
            {
                append_hook_events(
                    &mut entries,
                    InventoryHookSource {
                        source: hook_source,
                        plugin_id: None,
                        source_path,
                        source_relative_path: None,
                    },
                    hook_events,
                    &hook_config_rules,
                );
            }
            if let Some((source_path, hook_events)) =
                load_toml_hooks_from_layer(layer, &mut warnings)
            {
                append_hook_events(
                    &mut entries,
                    InventoryHookSource {
                        source: hook_source,
                        plugin_id: None,
                        source_path,
                        source_relative_path: None,
                    },
                    hook_events,
                    &hook_config_rules,
                );
            }
        }
    }

    for source in plugin_hook_sources {
        let plugin_id = source.plugin_id.as_key();
        append_hook_events(
            &mut entries,
            InventoryHookSource {
                source: HookSource::Plugin,
                plugin_id: Some(plugin_id),
                source_path: source.source_path.clone(),
                source_relative_path: Some(source.source_relative_path.clone()),
            },
            source.hooks.clone(),
            &hook_config_rules,
        );
    }

    entries
}

pub fn list_plugin_hooks(
    config_layer_stack: Option<&ConfigLayerStack>,
    plugin_hook_sources: &[PluginHookSource],
) -> Vec<HookInventoryEntry> {
    let mut warnings = Vec::new();
    let hook_config_rules = config_layer_stack
        .map(|config_layer_stack| HookConfigRules::from_stack(config_layer_stack, &mut warnings))
        .unwrap_or_default();
    let mut entries = Vec::new();

    for source in plugin_hook_sources {
        append_hook_events(
            &mut entries,
            InventoryHookSource {
                source: HookSource::Plugin,
                plugin_id: Some(source.plugin_id.as_key()),
                source_path: source.source_path.clone(),
                source_relative_path: Some(source.source_relative_path.clone()),
            },
            source.hooks.clone(),
            &hook_config_rules,
        );
    }

    entries
}

fn append_hook_events(
    entries: &mut Vec<HookInventoryEntry>,
    source: InventoryHookSource,
    hook_events: HookEventsToml,
    hook_config_rules: &HookConfigRules,
) {
    for (event_name, groups) in hook_events.into_matcher_groups() {
        append_matcher_groups(
            entries,
            source.clone(),
            event_name,
            groups,
            hook_config_rules,
        );
    }
}

fn append_matcher_groups(
    entries: &mut Vec<HookInventoryEntry>,
    source: InventoryHookSource,
    event_name: HookEventName,
    groups: Vec<MatcherGroup>,
    hook_config_rules: &HookConfigRules,
) {
    for (group_index, group) in groups.into_iter().enumerate() {
        for (handler_index, handler) in group.hooks.into_iter().enumerate() {
            let key = match (source.source, source.source_relative_path.as_deref()) {
                (HookSource::Plugin, Some(source_relative_path)) => {
                    hook_config_key(source_relative_path, event_name, group_index, handler_index)
                }
                _ => local_hook_config_key(event_name, group_index, handler_index),
            };
            let enabled = hook_config_rules.enabled_for_hook(
                source.source,
                source.plugin_id.as_deref(),
                &source.source_path,
                &key,
            );
            let (handler_type, command, timeout_sec, status_message) =
                hook_inventory_handler_fields(handler);
            entries.push(HookInventoryEntry {
                source: source.source,
                plugin_id: source.plugin_id.clone(),
                key,
                event_name,
                matcher: group.matcher.clone(),
                handler_type,
                command,
                timeout_sec,
                status_message,
                source_path: source.source_path.clone(),
                source_relative_path: source.source_relative_path.clone(),
                enabled,
            });
        }
    }
}

fn hook_inventory_handler_fields(
    handler: HookHandlerConfig,
) -> (HookHandlerType, Option<String>, Option<u64>, Option<String>) {
    match handler {
        HookHandlerConfig::Command {
            command,
            timeout_sec,
            r#async: _,
            status_message,
        } => (
            HookHandlerType::Command,
            Some(command),
            timeout_sec,
            status_message,
        ),
        HookHandlerConfig::Prompt {} => (HookHandlerType::Prompt, None, None, None),
        HookHandlerConfig::Agent {} => (HookHandlerType::Agent, None, None, None),
    }
}
