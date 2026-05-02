use std::collections::BTreeMap;

use codex_features::FEATURES;
use codex_protocol::models::ResponseItem;
use codex_utils_template::Template;
use codex_utils_template::TemplateError;
use codex_utils_template::TemplateRenderError;
use toml::Value as TomlValue;
use tracing::warn;

use crate::context_manager::updates::build_developer_update_item;

use super::session::SessionConfiguration;
use super::turn_context::TurnContext;

pub(super) fn render_feature_hint_messages(
    session_configuration: &SessionConfiguration,
    turn_context: &TurnContext,
) -> Vec<ResponseItem> {
    let lockfile = match session_configuration.to_config_lockfile_toml() {
        Ok(lockfile) => lockfile,
        Err(err) => {
            warn!(error = %err, "failed to build config lock for feature hint rendering");
            return Vec::new();
        }
    };
    let Some(features_toml) = lockfile.config.features.as_ref() else {
        return Vec::new();
    };
    let template_variables = match build_template_variables(&lockfile.config) {
        Ok(variables) => variables,
        Err(err) => {
            warn!(error = %err, "failed to serialize resolved config for feature hint rendering");
            return Vec::new();
        }
    };

    FEATURES
        .iter()
        .filter(|spec| turn_context.features.enabled(spec.id))
        .filter_map(|spec| {
            let hint = features_toml.hint(spec.key)?;
            let rendered = match render_feature_hint(hint, &template_variables) {
                Ok(rendered) => rendered,
                Err(err) => {
                    warn!(
                        feature = spec.key,
                        error = %err,
                        "failed to render feature hint"
                    );
                    return None;
                }
            };
            if rendered.is_empty() {
                return None;
            }
            build_developer_update_item(vec![rendered])
        })
        .collect()
}

fn render_feature_hint(
    source: &str,
    template_variables: &BTreeMap<String, String>,
) -> Result<String, TemplateError> {
    let template = Template::parse(source)?;
    let variables = template
        .placeholders()
        .map(|placeholder| {
            template_variables
                .get(placeholder)
                .map(|value| (placeholder, value.as_str()))
                .ok_or_else(|| TemplateRenderError::MissingValue {
                    name: placeholder.to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    template.render(variables).map_err(Into::into)
}

fn build_template_variables(
    config: &codex_config::config_toml::ConfigToml,
) -> Result<BTreeMap<String, String>, toml::ser::Error> {
    let value = TomlValue::try_from(config)?;
    let mut variables = BTreeMap::new();
    flatten_toml_value(/*prefix*/ None, &value, &mut variables);
    Ok(variables)
}

fn flatten_toml_value(
    prefix: Option<&str>,
    value: &TomlValue,
    variables: &mut BTreeMap<String, String>,
) {
    match value {
        TomlValue::Table(table) => {
            for (key, value) in table {
                let key = match prefix {
                    Some(prefix) => format!("{prefix}.{key}"),
                    None => key.clone(),
                };
                flatten_toml_value(Some(&key), value, variables);
            }
        }
        TomlValue::String(value) => insert_template_variable(prefix, value.clone(), variables),
        TomlValue::Integer(value) => {
            insert_template_variable(prefix, value.to_string(), variables);
        }
        TomlValue::Float(value) => {
            insert_template_variable(prefix, value.to_string(), variables);
        }
        TomlValue::Boolean(value) => {
            insert_template_variable(prefix, value.to_string(), variables);
        }
        TomlValue::Datetime(value) => {
            insert_template_variable(prefix, value.to_string(), variables);
        }
        TomlValue::Array(value) => {
            insert_template_variable(
                prefix,
                TomlValue::Array(value.clone()).to_string(),
                variables,
            );
        }
    }
}

fn insert_template_variable(
    key: Option<&str>,
    value: String,
    variables: &mut BTreeMap<String, String>,
) {
    if let Some(key) = key {
        variables.insert(key.to_string(), value);
    }
}
