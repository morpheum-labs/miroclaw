//! Domain entities used by sync and reconciliation.

use serde::{Deserialize, Serialize};

/// Minimal tool metadata mirrored from Clawgotcha for delegate tool wiring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: Option<String>,
}

/// Delegate-oriented agent definition aligned with host `DelegateAgentConfig` mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub api_key: Option<String>,
    pub temperature: Option<f64>,
    pub max_depth: u32,
    pub agentic: bool,
    pub allowed_tools: Vec<String>,
    pub max_iterations: usize,
    pub timeout_secs: Option<u64>,
    pub agentic_timeout_secs: Option<u64>,
    pub skills_directory: Option<String>,
    pub memory_namespace: Option<String>,
    pub tools: Vec<ToolMetadata>,
    pub current_revision: u64,
}

/// Cron job definition aligned with host cron persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CronJobDefinition {
    pub id: String,
    pub expression: String,
    pub target_agent_name: Option<String>,
    pub prompt: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub enabled: bool,
    pub current_revision: u64,
}

/// Singleton swarm-level defaults (maps to host default provider/model).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmDefaults {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub current_revision: u64,
}

/// Cursor for delta APIs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RevisionSummary {
    pub global_max_revision: u64,
    pub agents_revision_at: std::collections::HashMap<String, u64>,
    pub cron_revision_at: std::collections::HashMap<String, u64>,
    pub config_revision_at: Option<u64>,
}

/// Runtime instance registration payload (metadata only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawgotchaInstance {
    pub instance_name: String,
    pub callback_url: Option<String>,
    /// Required by agentbook `POST /api/v1/instances/register`.
    pub hostname: String,
    /// Required by agentbook registration (e.g. Miroclaw package version).
    pub version: String,
}

/// Snapshot persisted when Clawgotcha is unreachable (`OfflineCache`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineSnapshot {
    pub revision: RevisionSummary,
    pub agents: Vec<AgentDefinition>,
    pub cron_jobs: Vec<CronJobDefinition>,
    pub swarm_defaults: Option<SwarmDefaults>,
}
