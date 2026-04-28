use codex_protocol::account::PlanType;
use codex_protocol::protocol::CurrentUsageLimitNudgeState;
use codex_protocol::protocol::UsageLimitNudge;
use codex_protocol::protocol::UsageLimitNudgeAction;

pub(super) const CURRENT_USAGE_LIMIT_NUDGE_URL: &str = "https://chatgpt.com/codex/settings/usage";
pub(super) const WORKSPACE_OWNER_USAGE_LIMIT_NUDGE_URL: &str = "https://chatgpt.com/admin/billing";
pub(super) const UPGRADE_USAGE_LIMIT_NUDGE_URL: &str = "https://chatgpt.com/explore/pro";

#[derive(Default)]
pub(super) struct CurrentUsageLimitNudgePromptState {
    current: Option<UsageLimitNudge>,
    pending: Option<UsageLimitNudge>,
    last_shown_key: Option<String>,
}

impl CurrentUsageLimitNudgePromptState {
    pub(super) fn update(&mut self, state: CurrentUsageLimitNudgeState) {
        match state {
            CurrentUsageLimitNudgeState::Unknown => {}
            CurrentUsageLimitNudgeState::Inactive => {
                self.current = None;
                self.pending = None;
                self.last_shown_key = None;
            }
            CurrentUsageLimitNudgeState::Active(nudge) => {
                let already_shown = self.last_shown_key.as_deref() == Some(nudge.key.as_str());
                self.current = Some(nudge.clone());
                self.pending = (!already_shown).then_some(nudge);
            }
        }
    }

    pub(super) fn take_pending(&mut self) -> Option<UsageLimitNudge> {
        let nudge = self.pending.take()?;
        self.last_shown_key = Some(nudge.key.clone());
        Some(nudge)
    }

    pub(super) fn is_active(&self) -> bool {
        self.current.is_some()
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

pub(super) fn prompt_subtitle(nudge: &UsageLimitNudge) -> String {
    let action = match nudge.action {
        UsageLimitNudgeAction::AddCredits => "Add credits",
        UsageLimitNudgeAction::Upgrade => "Upgrade",
    };
    format!(
        "You're at {}% of your Codex usage limit. {action} now to keep going?",
        nudge.threshold.as_percent()
    )
}

pub(super) fn prompt_url(nudge: &UsageLimitNudge, plan_type: Option<PlanType>) -> &'static str {
    match nudge.action {
        UsageLimitNudgeAction::Upgrade => UPGRADE_USAGE_LIMIT_NUDGE_URL,
        UsageLimitNudgeAction::AddCredits
            if plan_type.is_some_and(PlanType::is_workspace_account) =>
        {
            WORKSPACE_OWNER_USAGE_LIMIT_NUDGE_URL
        }
        UsageLimitNudgeAction::AddCredits => CURRENT_USAGE_LIMIT_NUDGE_URL,
    }
}
