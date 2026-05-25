//! Active agent marker (`active_agent.toml`) and legacy `active_workspace.toml` support.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ACTIVE_AGENT_STATE_FILE: &str = "active_agent.toml";
pub const LEGACY_ACTIVE_WORKSPACE_STATE_FILE: &str = "active_workspace.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveAgentState {
    /// Agent name (registry key).
    #[serde(default)]
    pub agent: Option<String>,
    /// Profile config directory (contains config.toml + workspace/).
    pub config_dir: String,
}

fn active_agent_state_path(default_dir: &Path) -> PathBuf {
    default_dir.join(ACTIVE_AGENT_STATE_FILE)
}

fn legacy_active_workspace_state_path(default_dir: &Path) -> PathBuf {
    default_dir.join(LEGACY_ACTIVE_WORKSPACE_STATE_FILE)
}

/// Load persisted active agent config dir from `active_agent.toml` or legacy marker.
pub async fn load_persisted_active_agent_config_dir(
    default_config_dir: &Path,
    expand_tilde: fn(&str) -> PathBuf,
) -> Result<Option<PathBuf>> {
    if let Some(dir) = load_marker_file(
        &active_agent_state_path(default_config_dir),
        default_config_dir,
        expand_tilde,
    )
    .await?
    {
        return Ok(Some(dir));
    }
    load_marker_file(
        &legacy_active_workspace_state_path(default_config_dir),
        default_config_dir,
        expand_tilde,
    )
    .await
}

async fn load_marker_file(
    state_path: &Path,
    default_config_dir: &Path,
    expand_tilde: fn(&str) -> PathBuf,
) -> Result<Option<PathBuf>> {
    if !state_path.exists() {
        return Ok(None);
    }
    let contents = match tokio::fs::read_to_string(state_path).await {
        Ok(c) => c,
        Err(error) => {
            tracing::warn!(
                "Failed to read active agent marker {}: {error}",
                state_path.display()
            );
            return Ok(None);
        }
    };
    let state: ActiveAgentState = match toml::from_str(&contents) {
        Ok(s) => s,
        Err(error) => {
            tracing::warn!(
                "Failed to parse active agent marker {}: {error}",
                state_path.display()
            );
            return Ok(None);
        }
    };
    let raw = state.config_dir.trim();
    if raw.is_empty() {
        tracing::warn!(
            "Ignoring active agent marker {} because config_dir is empty",
            state_path.display()
        );
        return Ok(None);
    }
    let parsed = expand_tilde(raw);
    let config_dir = if parsed.is_absolute() {
        parsed
    } else {
        default_config_dir.join(parsed)
    };
    Ok(Some(config_dir))
}

pub async fn persist_active_agent_config_dir(
    config_dir: &Path,
    agent_name: Option<&str>,
    default_config_dir: &Path,
    is_temp_directory: fn(&Path) -> bool,
) -> Result<()> {
    let state_path = active_agent_state_path(default_config_dir);

    if is_temp_directory(config_dir) && !is_temp_directory(default_config_dir) {
        tracing::warn!(
            path = %config_dir.display(),
            "Refusing to persist temp directory as active agent marker"
        );
        return Ok(());
    }

    if config_dir == default_config_dir.join("profiles").join("main")
        || config_dir == default_config_dir
    {
        // Default layout — clear marker when pointing at canonical default profile root.
        let legacy = legacy_active_workspace_state_path(default_config_dir);
        if state_path.exists() {
            let _ = tokio::fs::remove_file(&state_path).await;
        }
        if legacy.exists() {
            let _ = tokio::fs::remove_file(&legacy).await;
        }
        return Ok(());
    }

    tokio::fs::create_dir_all(default_config_dir)
        .await
        .with_context(|| format!("creating {}", default_config_dir.display()))?;

    let state = ActiveAgentState {
        agent: agent_name.map(str::to_string),
        config_dir: config_dir.to_string_lossy().into_owned(),
    };
    let serialized = toml::to_string_pretty(&state).context("serializing active agent marker")?;
    let temp_path = default_config_dir.join(format!(
        ".{ACTIVE_AGENT_STATE_FILE}.tmp-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::write(&temp_path, &serialized)
        .await
        .with_context(|| format!("writing {}", temp_path.display()))?;
    tokio::fs::rename(&temp_path, &state_path)
        .await
        .with_context(|| format!("persisting {}", state_path.display()))?;

    // Remove legacy marker when writing new format.
    let legacy = legacy_active_workspace_state_path(default_config_dir);
    if legacy.exists() {
        let _ = tokio::fs::remove_file(&legacy).await;
    }
    Ok(())
}
