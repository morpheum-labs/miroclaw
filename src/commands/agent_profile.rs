//! Agent profile management CLI (`miroclaw agents …`).

use crate::config::registry::{AgentRegistry, AgentRegistryEntry, DEFAULT_AGENT_NAME};
use crate::config::schema::{default_config_dir, ensure_bootstrap_files, Config};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Subcommands for agent profile management.
#[derive(Debug, clap::Subcommand)]
pub enum AgentProfileCommands {
    /// List registered agent profiles
    List,
    /// Create a new agent profile directory and registry entry
    Create {
        /// Agent name (alphanumeric, hyphens, underscores)
        name: String,
        /// Copy defaults from an existing profile
        #[arg(long)]
        from: Option<String>,
    },
    /// Set the active agent for CLI commands
    Use {
        /// Agent name
        name: String,
    },
    /// Show details for one agent profile
    Show {
        /// Agent name
        name: String,
    },
    /// Run a single agent worker (internal gateway + channels)
    Worker {
        /// Agent profile name from registry
        #[arg(long)]
        profile: String,
        /// Override internal gateway port
        #[arg(long)]
        port: Option<u16>,
    },
}

pub async fn handle_command(command: AgentProfileCommands) -> Result<()> {
    let home_dir = default_config_dir()?;
    match command {
        AgentProfileCommands::List => cmd_list(&home_dir).await,
        AgentProfileCommands::Create { name, from } => {
            cmd_create(&home_dir, &name, from.as_deref()).await
        }
        AgentProfileCommands::Use { name } => cmd_use(&home_dir, &name).await,
        AgentProfileCommands::Show { name } => cmd_show(&home_dir, &name).await,
        AgentProfileCommands::Worker { profile, port } => {
            cmd_worker(&home_dir, &profile, port).await
        }
    }
}

async fn load_registry(home_dir: &Path) -> Result<AgentRegistry> {
    AgentRegistry::load_from(home_dir).await
}

fn is_temp_path(p: &Path) -> bool {
    let temp = std::env::temp_dir();
    let canon_temp = temp.canonicalize().unwrap_or(temp);
    let canon_path = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon_path.starts_with(&canon_temp)
}

async fn cmd_list(home_dir: &Path) -> Result<()> {
    let registry = load_registry(home_dir).await?;
    if registry.agents.is_empty() {
        println!("No agent profiles registered.");
        println!("Create one with: miroclaw agents create main");
        return Ok(());
    }
    let active =
        crate::config::active_agent::load_persisted_active_agent_config_dir(home_dir, |s| {
            PathBuf::from(shellexpand::tilde(s).as_ref())
        })
        .await?
        .map(|p| p.to_string_lossy().into_owned());

    println!(
        "{:<16} {:<8} {:<8} {}",
        "NAME", "ENABLED", "PORT", "CONFIG_DIR"
    );
    for entry in &registry.agents {
        let dir = crate::config::registry::resolve_profile_path(home_dir, &entry.config_dir);
        let marker = active
            .as_ref()
            .is_some_and(|a| a == &dir.to_string_lossy())
            .then_some("*")
            .unwrap_or("");
        println!(
            "{:<16} {:<8} {:<8} {} {}",
            entry.name, entry.enabled, entry.internal_port, entry.config_dir, marker
        );
    }
    Ok(())
}

async fn cmd_create(home_dir: &Path, name: &str, from: Option<&str>) -> Result<()> {
    AgentRegistry::validate_agent_name(name)?;
    let mut registry = load_registry(home_dir).await?;
    if registry.get(name).is_some() {
        bail!("agent '{name}' already exists in registry");
    }

    let profile_dir = registry.profile_config_dir(home_dir, name);
    if profile_dir.exists() {
        bail!(
            "profile directory already exists: {}",
            profile_dir.display()
        );
    }

    tokio::fs::create_dir_all(profile_dir.join("workspace")).await?;

    if let Some(src_name) = from {
        let src_dir = registry
            .resolve_config_dir(home_dir, src_name)
            .with_context(|| format!("source agent '{src_name}' not found"))?;
        if src_dir.join("config.toml").exists() {
            tokio::fs::copy(src_dir.join("config.toml"), profile_dir.join("config.toml")).await?;
        }
    } else if name == DEFAULT_AGENT_NAME {
        let default_cfg = Config::default();
        let toml_str = toml::to_string_pretty(&strip_runtime_paths(&default_cfg))?;
        tokio::fs::write(profile_dir.join("config.toml"), toml_str).await?;
        ensure_bootstrap_files(&profile_dir.join("workspace")).await?;
    } else {
        let toml_str = format!(
            "# Agent profile config\n# Configure with: miroclaw onboard --config-dir {}\n",
            profile_dir.display()
        );
        tokio::fs::write(profile_dir.join("config.toml"), toml_str).await?;
    }

    let rel_config_dir = format!("{}/{name}", registry.profiles_dir);
    let port = registry.next_internal_port();
    registry.agents.push(AgentRegistryEntry {
        name: name.to_string(),
        config_dir: rel_config_dir,
        enabled: true,
        internal_port: port,
    });
    if registry.agents.len() == 1 {
        registry.default_agent = name.to_string();
    }
    registry.save_to(home_dir).await?;

    println!(
        "Created agent profile '{name}' at {}",
        profile_dir.display()
    );
    println!("Internal port: {port}");
    println!("Activate with: miroclaw agents use {name}");
    Ok(())
}

async fn cmd_use(home_dir: &Path, name: &str) -> Result<()> {
    let registry = load_registry(home_dir).await?;
    let config_dir = registry.resolve_config_dir(home_dir, name)?;
    if !config_dir.join("config.toml").exists() {
        bail!(
            "profile config not found at {}",
            config_dir.join("config.toml").display()
        );
    }
    crate::config::active_agent::persist_active_agent_config_dir(
        &config_dir,
        Some(name),
        home_dir,
        is_temp_path,
    )
    .await?;
    println!("Active agent set to '{name}' ({})", config_dir.display());
    Ok(())
}

async fn cmd_show(home_dir: &Path, name: &str) -> Result<()> {
    let registry = load_registry(home_dir).await?;
    let entry = registry
        .get(name)
        .with_context(|| format!("agent '{name}' not found"))?;
    let config_dir = registry.resolve_config_dir(home_dir, name)?;
    println!("Agent: {}", entry.name);
    println!("  enabled: {}", entry.enabled);
    println!("  internal_port: {}", entry.internal_port);
    println!("  config_dir: {}", config_dir.display());
    println!("  workspace: {}", config_dir.join("workspace").display());
    Ok(())
}

async fn cmd_worker(home_dir: &Path, profile: &str, port: Option<u16>) -> Result<()> {
    let registry = load_registry(home_dir).await?;
    let entry = registry
        .get(profile)
        .with_context(|| format!("agent '{profile}' not found"))?;
    let config_dir = registry.resolve_config_dir(home_dir, profile)?;
    std::env::set_var("MIROCLAW_CONFIG_DIR", &config_dir);
    let mut config = Config::load_or_init().await?;
    let internal_port = port.unwrap_or(entry.internal_port);
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = internal_port;
    crate::agent_worker::run(config, internal_port).await
}

fn strip_runtime_paths(config: &Config) -> Config {
    let mut c = config.clone();
    c.workspace_dir = PathBuf::new();
    c.config_path = PathBuf::new();
    c
}

pub async fn ensure_default_main_profile(home_dir: &Path) -> Result<()> {
    let registry = load_registry(home_dir).await?;
    if registry.get(DEFAULT_AGENT_NAME).is_some() {
        return Ok(());
    }
    cmd_create(home_dir, DEFAULT_AGENT_NAME, None).await
}
