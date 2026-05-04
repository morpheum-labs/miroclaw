//! Wire DTOs for the Clawgotcha HTTP API (serde).

use serde::{Deserialize, Serialize};

use super::domain::{AgentDefinition, CronJobDefinition, SwarmDefaults, ToolMetadata};

/// Errors converting wire payloads into domain entities.
#[derive(Debug, thiserror::Error)]
pub enum WireParseError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid value for {0}: {1}")]
    Invalid(&'static str, String),
}

/// `POST /api/v1/instances/register` body (agentbook OpenAPI `RegisterInstanceRequest`).
#[derive(Debug, Clone, Serialize)]
pub struct RegisterInstanceBody {
    pub instance_name: String,
    pub hostname: String,
    pub version: String,
    pub callback_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_type: Option<String>,
}

/// Agentbook `SwarmAgent` list element (mixed PascalCase / snake_case JSON).
#[derive(Debug, Deserialize)]
pub(crate) struct AgentbookSwarmAgent {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "SystemPrompt")]
    pub system_prompt: Option<String>,
    #[serde(rename = "Tools", default)]
    pub tools: Vec<String>,
    #[serde(rename = "Provider")]
    pub provider: Option<String>,
    #[serde(rename = "Model")]
    pub model: Option<String>,
    #[serde(rename = "TimeoutSeconds")]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub current_revision: u64,
    #[serde(default)]
    pub deleted: bool,
}

/// Agentbook `SwarmCronJob` list element.
#[derive(Debug, Deserialize)]
pub(crate) struct AgentbookCronJob {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "AgentName")]
    pub agent_name: String,
    #[serde(rename = "Schedule")]
    pub schedule: String,
    #[serde(rename = "Prompt")]
    pub prompt: Option<String>,
    #[serde(rename = "TimeoutSeconds")]
    pub timeout_seconds: Option<u64>,
    #[serde(rename = "Active", default = "default_true")]
    pub active: bool,
    #[serde(rename = "CurrentRevision", default)]
    pub current_revision: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentbookAgentListEnvelope {
    pub agents: Vec<AgentbookSwarmAgent>,
    #[serde(default)]
    pub revision_summary: Option<AgentbookRevisionSummary>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AgentbookRevisionSummary {
    /// Present in API payloads; sync uses `agents_max_revision` / `cron_jobs_max_revision` watermarks.
    #[serde(default)]
    #[allow(dead_code)]
    pub config_revision: u64,
    #[serde(default)]
    pub agents_max_revision: u64,
    #[serde(default)]
    pub cron_jobs_max_revision: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentbookCronListEnvelope {
    pub cron_jobs: Vec<AgentbookCronJob>,
    #[serde(default)]
    pub revision_summary: Option<AgentbookRevisionSummary>,
}

impl TryFrom<AgentbookSwarmAgent> for AgentDefinition {
    type Error = WireParseError;

    fn try_from(value: AgentbookSwarmAgent) -> Result<Self, Self::Error> {
        let name = value.name.trim().to_string();
        if name.is_empty() {
            return Err(WireParseError::MissingField("name"));
        }
        let provider = value.provider.unwrap_or_default().trim().to_string();
        let model = value.model.unwrap_or_default().trim().to_string();
        let tool_names = value.tools;
        Ok(Self {
            name,
            provider,
            model,
            system_prompt: value.system_prompt.filter(|s| !s.trim().is_empty()),
            api_key: None,
            temperature: None,
            max_depth: default_max_depth(),
            agentic: false,
            allowed_tools: tool_names.clone(),
            max_iterations: default_max_iterations(),
            timeout_secs: value.timeout_seconds,
            agentic_timeout_secs: None,
            skills_directory: None,
            memory_namespace: None,
            tools: tool_names
                .into_iter()
                .map(|name| ToolMetadata {
                    name,
                    description: None,
                })
                .collect(),
            current_revision: value.current_revision,
        })
    }
}

impl TryFrom<AgentbookCronJob> for CronJobDefinition {
    type Error = WireParseError;

    fn try_from(value: AgentbookCronJob) -> Result<Self, Self::Error> {
        let id = value.id.trim().to_string();
        if id.is_empty() {
            return Err(WireParseError::MissingField("id"));
        }
        let agent_name = value.agent_name.trim().to_string();
        if agent_name.is_empty() {
            return Err(WireParseError::MissingField("agent_name"));
        }
        Ok(Self {
            id,
            expression: value.schedule,
            target_agent_name: Some(agent_name),
            prompt: value.prompt,
            timeout_seconds: value.timeout_seconds,
            enabled: value.active,
            current_revision: value.current_revision,
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

/// Response from `GET /v1/instances/.../mcp-credentials` (decrypted MCP bindings).
#[derive(Debug, Clone, Deserialize)]
pub struct McpCredentialsRevealResponse {
    pub agent_name: String,
    pub mcp_bindings: Vec<McpBindingReveal>,
    pub revision: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpBindingReveal {
    pub mcp_server_name: String,
    pub material_kind: String,
    pub payload: serde_json::Value,
}
