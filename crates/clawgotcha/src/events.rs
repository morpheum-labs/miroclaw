//! Typed change notifications fed into the reconciler (polling + webhooks).

use serde::{Deserialize, Serialize};

/// Fan-in event for agent, cron, and swarm-default updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeEvent {
    AgentUpdated {
        name: String,
        revision: u64,
    },
    AgentDeleted {
        name: String,
        revision: u64,
    },
    CronUpdated {
        job_id: String,
        revision: u64,
    },
    CronDeleted {
        job_id: String,
        revision: u64,
    },
    ConfigUpdated {
        revision: u64,
    },
    /// Synthetic heartbeat from webhook transport (coalesced by sync layer).
    NotifySync {
        reason: String,
    },
}

impl ChangeEvent {
    /// Stable idempotency key: entity plus revision where applicable.
    #[must_use]
    pub fn dedupe_key(&self) -> String {
        match self {
            ChangeEvent::AgentUpdated { name, revision } => {
                format!("agent:{name}:{revision}")
            }
            ChangeEvent::AgentDeleted { name, revision } => {
                format!("agent_del:{name}:{revision}")
            }
            ChangeEvent::CronUpdated { job_id, revision } => {
                format!("cron:{job_id}:{revision}")
            }
            ChangeEvent::CronDeleted { job_id, revision } => {
                format!("cron_del:{job_id}:{revision}")
            }
            ChangeEvent::ConfigUpdated { revision } => {
                format!("cfg:{revision}")
            }
            ChangeEvent::NotifySync { reason } => format!("notify:{reason}"),
        }
    }
}
