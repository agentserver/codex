use crate::config_api::ConfigApi;
use crate::error_code::invalid_request;
use async_trait::async_trait;
use codex_app_server_protocol::ConfigBatchWriteParams;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::ConfigValueWriteParams;
use codex_app_server_protocol::ConfigWriteResponse;
use codex_app_server_protocol::JSONRPCErrorError;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Applies remote plugin enablement changes whose current UI entry point is a
/// config-shaped RPC.
#[async_trait]
pub(crate) trait RemotePluginEnablementWriter: Send + Sync {
    async fn set_remote_plugin_enabled(
        &self,
        plugin_id: String,
        enabled: bool,
    ) -> Result<(), JSONRPCErrorError>;
}

#[derive(Clone)]
pub(crate) struct ConfigWriteRouter {
    config_api: ConfigApi,
    remote_plugin_enablement_writer: Arc<dyn RemotePluginEnablementWriter>,
}

impl ConfigWriteRouter {
    pub(crate) fn new(
        config_api: ConfigApi,
        remote_plugin_enablement_writer: Arc<dyn RemotePluginEnablementWriter>,
    ) -> Self {
        Self {
            config_api,
            remote_plugin_enablement_writer,
        }
    }

    pub(crate) async fn write_value(
        &self,
        params: ConfigValueWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        if let Some(remote_plugin_edit) =
            remote_plugin_enabled_config_edit(&params.key_path, &params.value)
        {
            let response = self
                .config_api
                .batch_write(ConfigBatchWriteParams {
                    edits: Vec::new(),
                    file_path: params.file_path,
                    expected_version: params.expected_version,
                    reload_user_config: false,
                })
                .await?;
            self.remote_plugin_enablement_writer
                .set_remote_plugin_enabled(remote_plugin_edit.plugin_id, remote_plugin_edit.enabled)
                .await?;
            return Ok(response);
        }

        self.config_api.write_value(params).await
    }

    pub(crate) async fn batch_write(
        &self,
        params: ConfigBatchWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let ConfigBatchWriteParams {
            edits,
            file_path,
            expected_version,
            reload_user_config,
        } = params;
        let mut local_edits = Vec::<ConfigEdit>::new();
        let mut remote_plugin_toggles = BTreeMap::<String, bool>::new();

        for edit in edits {
            if let Some(remote_plugin_edit) =
                remote_plugin_enabled_config_edit(&edit.key_path, &edit.value)
            {
                remote_plugin_toggles
                    .insert(remote_plugin_edit.plugin_id, remote_plugin_edit.enabled);
            } else {
                local_edits.push(edit);
            }
        }

        if !remote_plugin_toggles.is_empty() && !local_edits.is_empty() {
            return Err(invalid_request(
                "remote plugin enablement edits cannot be batched with local config edits",
            ));
        }

        if remote_plugin_toggles.is_empty() {
            return self
                .config_api
                .batch_write(ConfigBatchWriteParams {
                    edits: local_edits,
                    file_path,
                    expected_version,
                    reload_user_config,
                })
                .await;
        }

        let response = self
            .config_api
            .batch_write(ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: file_path.clone(),
                expected_version,
                reload_user_config: false,
            })
            .await?;

        for (plugin_id, enabled) in remote_plugin_toggles {
            self.remote_plugin_enablement_writer
                .set_remote_plugin_enabled(plugin_id, enabled)
                .await?;
        }

        if reload_user_config {
            self.config_api
                .batch_write(ConfigBatchWriteParams {
                    edits: Vec::new(),
                    file_path,
                    expected_version: None,
                    reload_user_config: true,
                })
                .await?;
        }

        Ok(response)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemotePluginEnabledConfigEdit {
    plugin_id: String,
    enabled: bool,
}

fn remote_plugin_enabled_config_edit(
    key_path: &str,
    value: &JsonValue,
) -> Option<RemotePluginEnabledConfigEdit> {
    let enabled = value.as_bool()?;
    let mut segments = key_path.split('.');
    let table = segments.next()?;
    let plugin_id = segments.next()?;
    let field = segments.next()?;
    if table == "plugins"
        && field == "enabled"
        && segments.next().is_none()
        && codex_core_plugins::remote::is_supported_remote_plugin_id(plugin_id)
    {
        return Some(RemotePluginEnabledConfigEdit {
            plugin_id: plugin_id.to_string(),
            enabled,
        });
    }
    None
}
