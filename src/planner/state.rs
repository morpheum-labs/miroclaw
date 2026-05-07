//! Workspace-backed planner state: active plan pointer and per-session invocation index.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const ACTIVE_FILE: &str = "planner_active.json";
const SESSION_FILE: &str = "planner_sessions.json";

fn state_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("state")
}

fn active_path(workspace_dir: &Path) -> PathBuf {
    state_dir(workspace_dir).join(ACTIVE_FILE)
}

fn sessions_path(workspace_dir: &Path) -> PathBuf {
    state_dir(workspace_dir).join(SESSION_FILE)
}

/// Rolling active plan identity (monotonic `version` until `plan_id` changes).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlannerActivePointer {
    #[serde(default)]
    pub plan_id: String,
    #[serde(default)]
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionRecord {
    #[serde(default)]
    pub planner_invoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionsFile {
    #[serde(default)]
    sessions: HashMap<String, SessionRecord>,
}

/// Load active pointer; empty `plan_id` means uninitialized.
pub async fn load_active_pointer(workspace_dir: &Path) -> anyhow::Result<PlannerActivePointer> {
    let p = active_path(workspace_dir);
    if !p.exists() {
        return Ok(PlannerActivePointer::default());
    }
    let raw = tokio::fs::read_to_string(&p)
        .await
        .with_context(|| format!("read {}", p.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", p.display()))
}

pub async fn save_active_pointer(
    workspace_dir: &Path,
    pointer: &PlannerActivePointer,
) -> anyhow::Result<()> {
    let dir = state_dir(workspace_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("mkdir {}", dir.display()))?;
    let p = active_path(workspace_dir);
    let json = serde_json::to_string_pretty(pointer)?;
    tokio::fs::write(&p, json)
        .await
        .with_context(|| format!("write {}", p.display()))?;
    Ok(())
}

/// Whether this session has already run the planner (for `FirstSessionTurn` mode).
pub async fn session_already_invoked(
    workspace_dir: &Path,
    session_id: &str,
) -> anyhow::Result<bool> {
    let p = sessions_path(workspace_dir);
    if !p.exists() {
        return Ok(false);
    }
    let raw = tokio::fs::read_to_string(&p).await.unwrap_or_default();
    let file: SessionsFile = serde_json::from_str(&raw).unwrap_or_default();
    Ok(file
        .sessions
        .get(session_id)
        .is_some_and(|r| r.planner_invoked))
}

pub async fn mark_session_invoked(workspace_dir: &Path, session_id: &str) -> anyhow::Result<()> {
    let dir = state_dir(workspace_dir);
    tokio::fs::create_dir_all(&dir).await?;
    let p = sessions_path(workspace_dir);
    let mut file: SessionsFile = if p.exists() {
        let raw = tokio::fs::read_to_string(&p).await.unwrap_or_default();
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        SessionsFile::default()
    };
    file.sessions.insert(
        session_id.to_string(),
        SessionRecord {
            planner_invoked: true,
        },
    );
    let json = serde_json::to_string_pretty(&file)?;
    tokio::fs::write(&p, json).await?;
    Ok(())
}
