use std::collections::HashSet;

use codex_config::ConfigLayerSource;
use codex_config::ConfigLayerStack;
use codex_config::ConfigLayerStackOrdering;
use codex_config::HookConfig;
use codex_config::HookEventsToml;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HookConfigRules {
    disabled_keys: HashSet<String>,
}

impl HookConfigRules {
    pub(crate) fn is_enabled(&self, key: &str) -> bool {
        !self.disabled_keys.contains(key)
    }
}

pub(crate) fn hook_config_rules_from_stack(
    config_layer_stack: Option<&ConfigLayerStack>,
) -> HookConfigRules {
    let Some(config_layer_stack) = config_layer_stack else {
        return HookConfigRules::default();
    };

    let mut disabled_keys = HashSet::new();
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
            Err(_) => {
                continue;
            }
        };

        for entry in hooks.config {
            let Some(key) = hook_config_key(&entry) else {
                continue;
            };
            if entry.enabled {
                disabled_keys.remove(&key);
            } else {
                disabled_keys.insert(key);
            }
        }
    }

    HookConfigRules { disabled_keys }
}

fn hook_config_key(entry: &HookConfig) -> Option<String> {
    let key = entry.key.as_deref().map(str::trim).unwrap_or_default();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}
