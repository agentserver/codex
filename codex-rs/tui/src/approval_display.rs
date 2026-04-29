//! Display-level approval decisions used by the TUI approval overlay.

use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_protocol::approvals::ExecPolicyAmendment;
use codex_protocol::approvals::NetworkPolicyAmendment;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewDecision {
    Approved,
    ApprovedExecpolicyAmendment {
        proposed_execpolicy_amendment: ExecPolicyAmendment,
    },
    ApprovedForSession,
    NetworkPolicyAmendment {
        network_policy_amendment: NetworkPolicyAmendment,
    },
    #[default]
    Denied,
    TimedOut,
    Abort,
}

impl From<ReviewDecision> for CommandExecutionApprovalDecision {
    fn from(value: ReviewDecision) -> Self {
        match value {
            ReviewDecision::Approved => Self::Accept,
            ReviewDecision::ApprovedExecpolicyAmendment {
                proposed_execpolicy_amendment,
            } => Self::AcceptWithExecpolicyAmendment {
                execpolicy_amendment: proposed_execpolicy_amendment.into(),
            },
            ReviewDecision::ApprovedForSession => Self::AcceptForSession,
            ReviewDecision::NetworkPolicyAmendment {
                network_policy_amendment,
            } => Self::ApplyNetworkPolicyAmendment {
                network_policy_amendment: network_policy_amendment.into(),
            },
            ReviewDecision::Denied | ReviewDecision::TimedOut => Self::Decline,
            ReviewDecision::Abort => Self::Cancel,
        }
    }
}
