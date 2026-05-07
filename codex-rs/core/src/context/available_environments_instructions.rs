//! Renders the `<environments>` developer-section block listing each
//! execution environment available for this turn. Modeled after
//! `available_skills_instructions.rs`.
//!
//! Spec reference: `2026-05-05-codex-app-gateway-and-exec-gateway-design.md`
//! § Subsystem 1, P4. Body advertises the env-aware tool family added in
//! Pa.1–Pa.6 (per the P4 update following Pa.7's `env_count >= 2` gate).

use codex_protocol::protocol::ENVIRONMENTS_CLOSE_TAG;
use codex_protocol::protocol::ENVIRONMENTS_OPEN_TAG;

use super::ContextualUserFragment;

/// One row in the `<environments>` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvironmentRow {
    pub(crate) environment_id: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AvailableEnvironmentsInstructions {
    rows: Vec<EnvironmentRow>,
}

impl AvailableEnvironmentsInstructions {
    /// Builds the fragment from the turn's environments.
    ///
    /// Returns `None` when fewer than 2 environments are present — the block
    /// has no value when there is nothing for the LLM to choose between (per
    /// spec § P4 "absent / single-env turns omit the block"). The gate
    /// matches Pa.7's env-aware tool registration.
    pub(crate) fn from_turn_environments(
        environments: &[crate::session::turn_context::TurnEnvironment],
        descriptions: &std::collections::HashMap<String, Option<String>>,
        default_environment_id: Option<&str>,
    ) -> Option<Self> {
        if environments.len() < 2 {
            return None;
        }
        let rows = environments
            .iter()
            .map(|env| EnvironmentRow {
                environment_id: env.environment_id.clone(),
                description: descriptions
                    .get(&env.environment_id)
                    .and_then(|d| d.as_deref())
                    .unwrap_or("(no description)")
                    .to_string(),
                is_default: default_environment_id == Some(env.environment_id.as_str()),
            })
            .collect();
        Some(Self { rows })
    }
}

impl ContextualUserFragment for AvailableEnvironmentsInstructions {
    const ROLE: &'static str = "developer";
    const START_MARKER: &'static str = ENVIRONMENTS_OPEN_TAG;
    const END_MARKER: &'static str = ENVIRONMENTS_CLOSE_TAG;

    fn body(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "\nYou have access to the following execution environments. Operations on the\n\
             default environment use the standard tools (`shell`, `apply_patch`,\n\
             `exec_command`, etc.).\n\n\
             For operations on **non-default environments**, use the env-aware tool family\n\
             and pass `environment_id` explicitly. Available env-aware tools:\n\n\
             - `exec_command_in_environment(environment_id, cmd, ...)` \
             — run a command on the named environment\n\
             - `apply_patch_in_environment(environment_id, input)` \
             — apply a patch on the named environment's filesystem\n\
             - `list_dir_in_environment(environment_id, path)` \
             — list a directory on the named environment\n\
             - `read_file_in_environment(environment_id, path)` \
             — read a file on the named environment\n\
             - `write_file_in_environment(environment_id, path, content)` \
             — write a file on the named environment\n\
             - `view_image_in_environment(environment_id, path)` \
             — view an image file on the named environment\n\
             - `list_environments()` — refresh the catalog below\n\n\
             Available environments:\n\n",
        );
        out.push_str("| id | description | default |\n");
        out.push_str("| --- | --- | --- |\n");
        for row in &self.rows {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                escape_table_cell(&row.environment_id),
                escape_table_cell(&row.description),
                if row.is_default { "yes" } else { "no" },
            ));
        }
        out
    }
}

/// Escapes pipe and newline characters so a malicious / quirky description
/// cannot break the markdown table rendering. (Per spec § P4 tests.)
fn escape_table_cell(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
        .replace('\r', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_for(ids_and_defaults: &[(&str, bool)]) -> Vec<EnvironmentRow> {
        ids_and_defaults
            .iter()
            .map(|(id, def)| EnvironmentRow {
                environment_id: (*id).to_string(),
                description: format!("desc for {id}"),
                is_default: *def,
            })
            .collect()
    }

    #[test]
    fn renders_table_for_multiple_environments() {
        let frag = AvailableEnvironmentsInstructions {
            rows: rows_for(&[("exe_a", true), ("exe_b", false), ("exe_c", false)]),
        };
        let body = frag.body();
        assert!(body.contains("| exe_a | desc for exe_a | yes |"));
        assert!(body.contains("| exe_b | desc for exe_b | no |"));
        assert!(body.contains("| exe_c | desc for exe_c | no |"));
    }

    #[tokio::test]
    async fn from_turn_environments_returns_none_for_single_env() {
        let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
            std::env::current_dir().expect("cwd").as_path(),
        )
        .expect("abs");
        let env = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
        let environments = vec![crate::session::turn_context::TurnEnvironment {
            environment_id: "only".into(),
            environment: env,
            cwd,
            shell: "/bin/sh".into(),
        }];
        let descriptions = std::collections::HashMap::new();
        assert!(
            AvailableEnvironmentsInstructions::from_turn_environments(
                &environments,
                &descriptions,
                Some("only"),
            )
            .is_none()
        );
    }

    #[test]
    fn escapes_pipe_and_newline_in_descriptions() {
        let frag = AvailableEnvironmentsInstructions {
            rows: vec![
                EnvironmentRow {
                    environment_id: "x".into(),
                    description: "evil | desc\nwith newline".into(),
                    is_default: true,
                },
                EnvironmentRow {
                    environment_id: "y".into(),
                    description: "ok".into(),
                    is_default: false,
                },
            ],
        };
        let body = frag.body();
        assert!(body.contains("| evil \\| desc with newline |"));
        assert!(!body.contains("evil | desc"));
    }

    #[tokio::test]
    async fn default_flag_matches_default_environment_id() {
        let cwd = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(
            std::env::current_dir().expect("cwd").as_path(),
        )
        .expect("abs");
        let env = std::sync::Arc::new(codex_exec_server::Environment::default_for_tests());
        let environments = vec![
            crate::session::turn_context::TurnEnvironment {
                environment_id: "a".into(),
                environment: env.clone(),
                cwd: cwd.clone(),
                shell: "/bin/sh".into(),
            },
            crate::session::turn_context::TurnEnvironment {
                environment_id: "b".into(),
                environment: env,
                cwd,
                shell: "/bin/sh".into(),
            },
        ];
        let mut descriptions = std::collections::HashMap::new();
        descriptions.insert("a".to_string(), Some("Alpha".to_string()));
        descriptions.insert("b".to_string(), Some("Beta".to_string()));
        let frag = AvailableEnvironmentsInstructions::from_turn_environments(
            &environments,
            &descriptions,
            Some("b"),
        )
        .expect("two envs");
        let body = frag.body();
        assert!(body.contains("| a | Alpha | no |"));
        assert!(body.contains("| b | Beta | yes |"));
    }

    #[test]
    fn body_mentions_env_aware_tool_family() {
        let frag = AvailableEnvironmentsInstructions {
            rows: rows_for(&[("a", true), ("b", false)]),
        };
        let body = frag.body();
        assert!(
            body.contains("exec_command_in_environment"),
            "body should mention exec_command_in_environment, got: {body}"
        );
        assert!(
            body.contains("apply_patch_in_environment"),
            "body should mention apply_patch_in_environment, got: {body}"
        );
        assert!(
            body.contains("list_environments"),
            "body should mention list_environments, got: {body}"
        );
    }
}
