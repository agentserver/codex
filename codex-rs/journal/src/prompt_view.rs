/// View configuration used when rendering or forking a journal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptView {
    pub(crate) is_root: bool,
    pub(crate) agent_path: Option<String>,
    pub(crate) agent_role: Option<String>,
}

impl PromptView {
    /// Returns the view for the root agent.
    pub fn root() -> Self {
        Self {
            is_root: true,
            agent_path: None,
            agent_role: None,
        }
    }

    /// Returns the view for a spawned subagent.
    pub fn subagent(agent_path: impl Into<String>, agent_role: Option<String>) -> Self {
        Self {
            is_root: false,
            agent_path: Some(agent_path.into()),
            agent_role,
        }
    }

    /// Sets the agent role used for audience matching.
    pub fn with_agent_role(mut self, agent_role: impl Into<String>) -> Self {
        self.agent_role = Some(agent_role.into());
        self
    }
}
