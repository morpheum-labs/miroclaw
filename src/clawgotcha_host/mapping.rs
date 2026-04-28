//! Map Clawgotcha domain agents into host [`DelegateAgentConfig`](crate::config::DelegateAgentConfig).

use clawgotcha::models::domain::AgentDefinition;

use crate::config::DelegateAgentConfig;

impl From<&AgentDefinition> for DelegateAgentConfig {
    fn from(def: &AgentDefinition) -> Self {
        Self {
            provider: def.provider.clone(),
            model: def.model.clone(),
            system_prompt: def.system_prompt.clone(),
            api_key: def.api_key.clone(),
            temperature: def.temperature,
            max_depth: def.max_depth,
            agentic: def.agentic,
            allowed_tools: def.allowed_tools.clone(),
            max_iterations: def.max_iterations,
            timeout_secs: def.timeout_secs,
            agentic_timeout_secs: def.agentic_timeout_secs,
            skills_directory: def.skills_directory.clone(),
            memory_namespace: def.memory_namespace.clone(),
        }
    }
}
