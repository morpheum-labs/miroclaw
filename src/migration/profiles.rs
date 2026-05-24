//! Migrate legacy flat layout to profile-per-agent structure.

use crate::config::registry::{AgentRegistry, AgentRegistryEntry, DEFAULT_AGENT_NAME, DEFAULT_INTERNAL_PORT_BASE};
use crate::config::schema::{default_config_dir, DelegateAgentConfig, Config};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub async fn migrate_profiles(home_dir: &Path, dry_run: bool) -> Result<()> {
    let legacy_workspace = home_dir.join("workspace");
    let legacy_config = home_dir.join("config.toml");
    let profile_main = home_dir.join("profiles").join(DEFAULT_AGENT_NAME);

    if profile_main.join("config.toml").exists() {
        bail!(
            "profiles/{DEFAULT_AGENT_NAME}/ already exists; migration not needed or already done"
        );
    }

    if !legacy_config.exists() && !legacy_workspace.exists() {
        bail!("No legacy layout found at {}", home_dir.display());
    }

    println!("Migration preview (dry_run={dry_run}):");
    println!("  Home: {}", home_dir.display());
    println!("  Legacy config: {}", legacy_config.display());
    println!("  Legacy workspace: {}", legacy_workspace.display());
    println!("  Target profile: {}", profile_main.display());

    let mut agents_to_split: HashMap<String, DelegateAgentConfig> = HashMap::new();
    if legacy_config.exists() {
        let contents = tokio::fs::read_to_string(&legacy_config).await?;
        if let Ok(parsed) = toml::from_str::<Config>(&contents) {
            agents_to_split = parsed.agents;
        }
    }

    if dry_run {
        println!("  Delegate agents to split: {}", agents_to_split.len());
        for name in agents_to_split.keys() {
            println!("    -> profiles/{name}/");
        }
        println!("Run without --dry-run to apply.");
        return Ok(());
    }

    tokio::fs::create_dir_all(&profile_main).await?;

    if legacy_config.exists() {
        move_or_copy(&legacy_config, &profile_main.join("config.toml")).await?;
        strip_hub_sections_from_profile(&profile_main.join("config.toml")).await?;
    }

    if legacy_workspace.exists() {
        move_dir_contents(&legacy_workspace, &profile_main.join("workspace")).await?;
    } else {
        tokio::fs::create_dir_all(profile_main.join("workspace")).await?;
    }

    let mut registry = AgentRegistry::load_from(home_dir).await.unwrap_or_default();
    registry.default_agent = DEFAULT_AGENT_NAME.to_string();
    if registry.get(DEFAULT_AGENT_NAME).is_none() {
        registry.agents.push(AgentRegistryEntry {
            name: DEFAULT_AGENT_NAME.to_string(),
            config_dir: format!("profiles/{DEFAULT_AGENT_NAME}"),
            enabled: true,
            internal_port: DEFAULT_INTERNAL_PORT_BASE,
        });
    }

    let mut port = DEFAULT_INTERNAL_PORT_BASE + 1;
    for (name, agent_cfg) in agents_to_split {
        if name == DEFAULT_AGENT_NAME {
            continue;
        }
        let profile_dir = home_dir.join("profiles").join(&name);
        tokio::fs::create_dir_all(profile_dir.join("workspace")).await?;
        write_delegate_profile(&profile_dir, &agent_cfg).await?;
        if registry.get(&name).is_none() {
            registry.agents.push(AgentRegistryEntry {
                name: name.clone(),
                config_dir: format!("profiles/{name}"),
                enabled: true,
                internal_port: port,
            });
            port += 1;
        }
    }

    registry.save_to(home_dir).await?;

    crate::config::active_agent::persist_active_agent_config_dir(
        &profile_main,
        Some(DEFAULT_AGENT_NAME),
        home_dir,
        is_temp_path,
    )
    .await?;

    write_hub_config(home_dir).await?;

    println!("Migration complete.");
    println!("  Main profile: {}", profile_main.display());
    println!("  Registry: {}", home_dir.join("registry.toml").display());
    println!("Enable hub mode with [hub] enabled = true in {}", home_dir.join("config.toml").display());
    Ok(())
}

async fn write_delegate_profile(profile_dir: &Path, agent: &DelegateAgentConfig) -> Result<()> {
    let mut cfg = Config::default();
    cfg.default_provider = Some(agent.provider.clone());
    cfg.default_model = Some(agent.model.clone());
    cfg.api_key = agent.api_key.clone();
    cfg.default_temperature = agent.temperature.unwrap_or(cfg.default_temperature);
    let mut stripped = cfg.clone();
    stripped.workspace_dir = PathBuf::new();
    stripped.config_path = PathBuf::new();
    let toml_str = toml::to_string_pretty(&stripped)?;
    tokio::fs::write(profile_dir.join("config.toml"), toml_str).await?;
    tokio::fs::create_dir_all(profile_dir.join("workspace")).await?;
    Ok(())
}

async fn write_hub_config(home_dir: &Path) -> Result<()> {
    let hub_path = home_dir.join("config.toml");
    if hub_path.exists() {
        let mut cfg: toml::Value = toml::from_str(&tokio::fs::read_to_string(&hub_path).await?)?;
        if let Some(table) = cfg.as_table_mut() {
            table.remove("agents");
            table.remove("agents_list_path");
            table.remove("agents_list_url");
            table.insert(
                "hub".to_string(),
                toml::Value::Table(toml::map::Map::from_iter([(
                    "enabled".to_string(),
                    toml::Value::Boolean(true),
                )])),
            );
            if !table.contains_key("gateway") {
                table.insert(
                    "gateway".to_string(),
                    toml::Value::Table(toml::map::Map::from_iter([
                        ("host".to_string(), toml::Value::String("127.0.0.1".into())),
                        ("port".to_string(), toml::Value::Integer(8080)),
                    ])),
                );
            }
        }
        tokio::fs::write(hub_path, toml::to_string_pretty(&cfg)?).await?;
    }
    Ok(())
}

async fn strip_hub_sections_from_profile(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let contents = tokio::fs::read_to_string(path).await?;
    let mut cfg: toml::Value = toml::from_str(&contents)?;
    if let Some(table) = cfg.as_table_mut() {
        table.remove("hub");
    }
    tokio::fs::write(path, toml::to_string_pretty(&cfg)?).await?;
    Ok(())
}

async fn move_or_copy(from: &Path, to: &Path) -> Result<()> {
    if tokio::fs::rename(from, to).await.is_err() {
        tokio::fs::copy(from, to).await?;
        tokio::fs::remove_file(from).await.ok();
    }
    Ok(())
}

async fn move_dir_contents(from: &Path, to: &Path) -> Result<()> {
    tokio::fs::create_dir_all(to).await?;
    let mut entries = tokio::fs::read_dir(from).await?;
    while let Some(entry) = entries.next_entry().await? {
        let dest = to.join(entry.file_name());
        if tokio::fs::rename(entry.path(), &dest).await.is_err() {
            if entry.path().is_dir() {
                copy_dir_recursive(&entry.path(), &dest).await?;
            } else {
                tokio::fs::copy(entry.path(), &dest).await?;
            }
        }
    }
    Ok(())
}

async fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    tokio::fs::create_dir_all(to).await?;
    let mut entries = tokio::fs::read_dir(from).await?;
    while let Some(entry) = entries.next_entry().await? {
        let dest = to.join(entry.file_name());
        if entry.file_type().await?.is_dir() {
            Box::pin(copy_dir_recursive(&entry.path(), &dest)).await?;
        } else {
            tokio::fs::copy(entry.path(), &dest).await?;
        }
    }
    Ok(())
}

fn is_temp_path(path: &Path) -> bool {
    let temp = std::env::temp_dir();
    let canon_temp = temp.canonicalize().unwrap_or(temp);
    let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canon_path.starts_with(&canon_temp)
}

pub async fn handle_migrate_profiles(dry_run: bool) -> Result<()> {
    let home_dir = default_config_dir()?;
    migrate_profiles(&home_dir, dry_run).await
}
