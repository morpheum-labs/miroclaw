//! Wire DTOs for the Clawgotcha HTTP API (serde).

use serde::{Deserialize, Serialize};

use super::domain::{
    AgentDefinition, ClawgotchaInstance, CronJobDefinition, SwarmDefaults, ToolMetadata,
};

/// Errors converting wire payloads into domain entities.
#[derive(Debug, thiserror::Error)]
pub enum WireParseError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid value for {0}: {1}")]
    Invalid(&'static str, String),
}

/// Mirrors `swarm_runtime_instances` registration response bodies (subset).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireInstanceRecord {
    pub instance_name: String,
    #[serde(default)]
    pub callback_url: Option<String>,
}

impl TryFrom<WireInstanceRecord> for ClawgotchaInstance {
    type Error = WireParseError;

    fn try_from(value: WireInstanceRecord) -> Result<Self, Self::Error> {
        let instance_name = value.instance_name.trim().to_string();
        if instance_name.is_empty() {
            return Err(WireParseError::MissingField("instance_name"));
        }
        Ok(Self {
            instance_name,
            callback_url: value.callback_url,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireToolMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAgent {
    pub name: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    #[serde(default)]
    pub agentic: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub agentic_timeout_secs: Option<u64>,
    #[serde(default)]
    pub skills_directory: Option<String>,
    #[serde(default)]
    pub memory_namespace: Option<String>,
    #[serde(default)]
    pub tools: Vec<WireToolMeta>,
    #[serde(default)]
    pub current_revision: u64,
}

fn default_max_depth() -> u32 {
    8
}

fn default_max_iterations() -> usize {
    10
}

impl TryFrom<WireAgent> for AgentDefinition {
    type Error = WireParseError;

    fn try_from(value: WireAgent) -> Result<Self, Self::Error> {
        let name = value.name.trim().to_string();
        if name.is_empty() {
            return Err(WireParseError::MissingField("name"));
        }
        Ok(Self {
            name,
            provider: value.provider,
            model: value.model,
            system_prompt: value.system_prompt,
            api_key: value.api_key,
            temperature: value.temperature,
            max_depth: value.max_depth,
            agentic: value.agentic,
            allowed_tools: value.allowed_tools,
            max_iterations: value.max_iterations,
            timeout_secs: value.timeout_secs,
            agentic_timeout_secs: value.agentic_timeout_secs,
            skills_directory: value.skills_directory,
            memory_namespace: value.memory_namespace,
            tools: value
                .tools
                .into_iter()
                .map(|t| ToolMetadata {
                    name: t.name,
                    description: t.description,
                })
                .collect(),
            current_revision: value.current_revision,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireCronJob {
    pub id: String,
    pub expression: String,
    #[serde(default)]
    pub target_agent_name: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub current_revision: u64,
}

fn default_true() -> bool {
    true
}

impl TryFrom<WireCronJob> for CronJobDefinition {
    type Error = WireParseError;

    fn try_from(value: WireCronJob) -> Result<Self, Self::Error> {
        let id = value.id.trim().to_string();
        if id.is_empty() {
            return Err(WireParseError::MissingField("id"));
        }
        Ok(Self {
            id,
            expression: value.expression,
            target_agent_name: value.target_agent_name,
            prompt: value.prompt,
            timeout_seconds: value.timeout_seconds,
            enabled: value.enabled,
            current_revision: value.current_revision,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSwarmDefaults {
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub current_revision: u64,
}

impl From<WireSwarmDefaults> for SwarmDefaults {
    fn from(value: WireSwarmDefaults) -> Self {
        Self {
            default_provider: value.default_provider,
            default_model: value.default_model,
            current_revision: value.current_revision,
        }
    }
}
