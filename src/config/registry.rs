//! Agent profile registry (`registry.toml`) for multi-agent supervisor layout.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROFILES_DIR: &str = "profiles";
pub const DEFAULT_AGENT_NAME: &str = "main";
pub const DEFAULT_INTERNAL_PORT_BASE: u16 = 18_080;
pub const REGISTRY_FILENAME: &str = "registry.toml";

/// One registered agent profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRegistryEntry {
    pub name: String,
    /// Path to profile root (contains `config.toml` + `workspace/`), relative to home or absolute.
    pub config_dir: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Localhost-only internal gateway port for this agent worker.
    pub internal_port: u16,
}

fn default_true() -> bool {
    true
}

/// Top-level agent registry persisted at `~/.miroclaw/registry.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRegistry {
    #[serde(default = "default_registry_version")]
    pub version: u32,
    #[serde(default = "default_profiles_dir")]
    pub profiles_dir: String,
    #[serde(default = "default_agent_name")]
    pub default_agent: String,
    #[serde(default)]
    pub agents: Vec<AgentRegistryEntry>,
}

fn default_registry_version() -> u32 {
    1
}

fn default_profiles_dir() -> String {
    DEFAULT_PROFILES_DIR.to_string()
}

fn default_agent_name() -> String {
    DEFAULT_AGENT_NAME.to_string()
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            version: default_registry_version(),
            profiles_dir: default_profiles_dir(),
            default_agent: default_agent_name(),
            agents: Vec::new(),
        }
    }
}

impl AgentRegistry {
    /// Validate agent name charset (alphanumeric, hyphens, underscores).
    pub fn validate_agent_name(name: &str) -> Result<()> {
        if name.is_empty() {
            bail!("agent name must not be empty");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!(
                "agent name must contain only alphanumeric characters, hyphens, or underscores"
            );
        }
        Ok(())
    }

    pub fn path_in_home(home_config_dir: &Path) -> PathBuf {
        home_config_dir.join(REGISTRY_FILENAME)
    }

    pub async fn load_from(home_config_dir: &Path) -> Result<Self> {
        let path = Self::path_in_home(home_config_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("reading registry {}", path.display()))?;
        let registry: Self = toml::from_str(&contents)
            .with_context(|| format!("parsing registry {}", path.display()))?;
        registry.validate()?;
        Ok(registry)
    }

    pub async fn save_to(&self, home_config_dir: &Path) -> Result<()> {
        self.validate()?;
        let path = Self::path_in_home(home_config_dir);
        tokio::fs::create_dir_all(home_config_dir)
            .await
            .with_context(|| format!("creating {}", home_config_dir.display()))?;
        let serialized = toml::to_string_pretty(self).context("serializing registry")?;
        let temp = home_config_dir.join(format!(".{REGISTRY_FILENAME}.tmp-{}", uuid::Uuid::new_v4()));
        tokio::fs::write(&temp, serialized)
            .await
            .with_context(|| format!("writing {}", temp.display()))?;
        tokio::fs::rename(&temp, &path)
            .await
            .with_context(|| format!("persisting registry {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        Self::validate_agent_name(&self.default_agent)?;
        let mut names = HashSet::new();
        let mut ports = HashSet::new();
        for entry in &self.agents {
            Self::validate_agent_name(&entry.name)?;
            if !names.insert(entry.name.clone()) {
                bail!("duplicate agent name in registry: {}", entry.name);
            }
            if entry.internal_port == 0 {
                bail!("internal_port must be non-zero for agent '{}'", entry.name);
            }
            if !ports.insert(entry.internal_port) {
                bail!(
                    "duplicate internal_port {} in registry",
                    entry.internal_port
                );
            }
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&AgentRegistryEntry> {
        self.agents.iter().find(|a| a.name == name)
    }

    pub fn resolve_config_dir(&self, home_config_dir: &Path, name: &str) -> Result<PathBuf> {
        let entry = self
            .get(name)
            .with_context(|| format!("agent '{name}' not found in registry"))?;
        Ok(resolve_profile_path(home_config_dir, &entry.config_dir))
    }

    pub fn enabled_agents(&self) -> impl Iterator<Item = &AgentRegistryEntry> {
        self.agents.iter().filter(|a| a.enabled)
    }

    /// Allocate the next free internal port starting at `DEFAULT_INTERNAL_PORT_BASE`.
    pub fn next_internal_port(&self) -> u16 {
        let mut port = DEFAULT_INTERNAL_PORT_BASE;
        let used: HashSet<u16> = self.agents.iter().map(|a| a.internal_port).collect();
        while used.contains(&port) {
            port = port.saturating_add(1);
        }
        port
    }

    pub fn default_config_dir(&self, home_config_dir: &Path) -> PathBuf {
        self.resolve_config_dir(home_config_dir, &self.default_agent)
            .unwrap_or_else(|_| {
                home_config_dir
                    .join(&self.profiles_dir)
                    .join(&self.default_agent)
            })
    }

    pub fn profile_config_dir(&self, home_config_dir: &Path, name: &str) -> PathBuf {
        home_config_dir.join(&self.profiles_dir).join(name)
    }

    /// Load a registered profile as a delegate sub-agent config + workspace directory.
    pub async fn load_profile_delegate(
        home_dir: &Path,
        agent_name: &str,
    ) -> Result<(crate::config::DelegateAgentConfig, PathBuf)> {
        let registry = Self::load_from(home_dir).await?;
        let config_dir = registry.resolve_config_dir(home_dir, agent_name)?;
        let prev = std::env::var("MIROCLAW_CONFIG_DIR").ok();
        std::env::set_var("MIROCLAW_CONFIG_DIR", &config_dir);
        let config = crate::config::Config::load_or_init().await?;
        if let Some(p) = prev.as_ref() {
            std::env::set_var("MIROCLAW_CONFIG_DIR", p);
        } else {
            std::env::remove_var("MIROCLAW_CONFIG_DIR");
        }
        let workspace_dir = config.workspace_dir.clone();
        let delegate_cfg = config
            .agents
            .get(agent_name)
            .cloned()
            .unwrap_or_else(|| delegate_config_from_profile(&config));
        Ok((delegate_cfg, workspace_dir))
    }
}

/// Build delegate settings from a profile's primary runtime config.
#[must_use]
pub fn delegate_config_from_profile(config: &crate::config::Config) -> crate::config::DelegateAgentConfig {
    crate::config::DelegateAgentConfig {
        provider: config
            .default_provider
            .clone()
            .unwrap_or_else(|| "openrouter".to_string()),
        model: config
            .default_model
            .clone()
            .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string()),
        system_prompt: None,
        api_key: config.api_key.clone(),
        temperature: Some(config.default_temperature),
        max_depth: 3,
        agentic: false,
        allowed_tools: Vec::new(),
        max_iterations: 10,
        timeout_secs: Some(config.delegate.timeout_secs),
        agentic_timeout_secs: Some(config.delegate.agentic_timeout_secs),
        skills_directory: None,
        memory_namespace: None,
    }
}

/// Expand tilde and resolve relative paths against `home_config_dir`.
#[must_use]
pub fn resolve_profile_path(home_config_dir: &Path, raw: &str) -> PathBuf {
    let expanded = shellexpand::tilde(raw.trim());
    let path = PathBuf::from(expanded.as_ref());
    if path.is_absolute() {
        path
    } else {
        home_config_dir.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validate_rejects_duplicate_names() {
        let reg = AgentRegistry {
            agents: vec![
                AgentRegistryEntry {
                    name: "a".into(),
                    config_dir: "profiles/a".into(),
                    enabled: true,
                    internal_port: 18080,
                },
                AgentRegistryEntry {
                    name: "a".into(),
                    config_dir: "profiles/b".into(),
                    enabled: true,
                    internal_port: 18081,
                },
            ],
            ..AgentRegistry::default()
        };
        assert!(reg.validate().is_err());
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut reg = AgentRegistry::default();
        reg.agents.push(AgentRegistryEntry {
            name: "main".into(),
            config_dir: "profiles/main".into(),
            enabled: true,
            internal_port: 18080,
        });
        reg.save_to(tmp.path()).await.unwrap();
        let loaded = AgentRegistry::load_from(tmp.path()).await.unwrap();
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.agents[0].name, "main");
    }
}
