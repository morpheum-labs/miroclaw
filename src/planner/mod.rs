//! Unified hierarchical planner (phase-1 skeleton).
//!
//! When `[planner].enabled = true`, the tool-call loop runs a stub planning pass after the
//! first system-prompt assembly each turn: workspace persistence, optional history injection,
//! and planner lifecycle hooks.

pub use crate::hooks::planner_payloads::{
    PlanRequestedPayload, PlanStatus, PlanUpdatedPayload, PlannerLevel,
};

use crate::hooks::HookRunner;
use crate::providers::ChatMessage;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Finite-state planner workflow (audit-oriented; phase-1 transitions are minimal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlannerState {
    Idle,
    ContextLoading,
    ComplexityAssessment,
    PlanGeneration,
    CritiqueRevision,
    ApprovalCheck,
    OutputDelegation,
    FeedbackProcessing,
    GlobalPlanUpdate,
}

/// Structured snapshot persisted under `workspace/plan_history/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSnapshot {
    pub plan_id: String,
    pub version: u32,
    pub level: PlannerLevel,
    pub status: PlanStatus,
    pub goal: String,
    pub session_id: String,
    pub triggered_by: String,
}

/// Phase-1 stub — no LLM calls; produces a versioned workspace artifact.
pub struct UnifiedPlanner;

impl UnifiedPlanner {
    /// Run tier-0 planning: persist files, fire hooks, optional user message for the model.
    #[allow(clippy::too_many_arguments)]
    pub async fn plan_turn_stub(
        workspace_dir: &Path,
        hooks: Option<&HookRunner>,
        goal: &str,
        session_id: &str,
        autonomy: crate::security::AutonomyLevel,
        level: PlannerLevel,
        parent_plan_id: Option<&str>,
        history: &mut Vec<ChatMessage>,
    ) {
        let plan_id = Uuid::new_v4().to_string();
        let version = 1_u32;
        let triggered_by = "user".to_string();

        let req = PlanRequestedPayload {
            goal: goal.to_string(),
            level,
            session_id: session_id.to_string(),
            autonomy_level: autonomy,
            parent_plan_id: parent_plan_id.map(String::from),
        };
        if let Some(h) = hooks {
            h.fire_planner_plan_requested(&req).await;
        }

        if let Err(e) = persist_plan_artifacts(
            workspace_dir,
            &plan_id,
            version,
            level,
            goal,
            session_id,
            &triggered_by,
        )
        .await
        {
            tracing::warn!(error = %e, "planner: failed to persist plan artifacts");
            return;
        }

        let updated = PlanUpdatedPayload {
            plan_id: plan_id.clone(),
            version,
            updated_milestones: vec![format!("m-{plan_id}")],
            status: PlanStatus::Draft,
            triggered_by: triggered_by.clone(),
        };
        if let Some(h) = hooks {
            h.fire_planner_plan_updated(&updated).await;
        }

        inject_plan_user_message(history, &plan_id, version, goal);
    }
}

/// If `[planner].enabled` and there is a non-empty user turn message, run the stub planner once.
#[allow(clippy::too_many_arguments)]
pub async fn maybe_run_stub_turn(
    planner_cfg: &crate::config::PlannerConfig,
    workspace_dir: &Path,
    hooks: Option<&HookRunner>,
    turn_user_message: Option<&str>,
    session_id: &str,
    autonomy: crate::security::AutonomyLevel,
    history: &mut Vec<ChatMessage>,
) {
    if !planner_cfg.enabled {
        return;
    }
    let Some(goal) = turn_user_message.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    UnifiedPlanner::plan_turn_stub(
        workspace_dir,
        hooks,
        goal,
        session_id,
        autonomy,
        PlannerLevel::Operational,
        None,
        history,
    )
    .await;
}

fn inject_plan_user_message(
    history: &mut Vec<ChatMessage>,
    plan_id: &str,
    version: u32,
    goal: &str,
) {
    let block = format!(
        "### Unified planner (phase-1 stub)\n\
         \n\
         Plan ID: `{plan_id}` (v{version})\n\
         Goal: {goal}\n\
         \n\
         _(Structured planning pass — execution continues with normal tools.)_"
    );
    if history.len() > 1 {
        history.insert(1, ChatMessage::user(block));
    } else {
        history.push(ChatMessage::user(block));
    }
}

async fn persist_plan_artifacts(
    workspace_dir: &Path,
    plan_id: &str,
    version: u32,
    level: PlannerLevel,
    goal: &str,
    session_id: &str,
    triggered_by: &str,
) -> anyhow::Result<()> {
    let plan_dir = workspace_dir.join("plan_history");
    tokio::fs::create_dir_all(&plan_dir)
        .await
        .with_context(|| format!("create planner history dir {}", plan_dir.display()))?;

    let snapshot = PlanSnapshot {
        plan_id: plan_id.to_string(),
        version,
        level,
        status: PlanStatus::Draft,
        goal: goal.to_string(),
        session_id: session_id.to_string(),
        triggered_by: triggered_by.to_string(),
    };
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let hist_path = plan_dir.join(format!("{stamp}_{plan_id}.json"));
    let json = serde_json::to_string_pretty(&snapshot)?;
    tokio::fs::write(&hist_path, json)
        .await
        .with_context(|| format!("write {}", hist_path.display()))?;

    let global_path = workspace_dir.join("GLOBAL_PLAN.md");
    let md = format!(
        "# Global plan\n\n\
         **Plan ID:** `{plan_id}`  \n\
         **Version:** {version}  \n\
         **Session:** `{session_id}`  \n\
         **Trigger:** {triggered_by}  \n\
         \n\
         ## Active goal\n\
         \n\
         {goal}\n\
         \n\
         ## Milestones\n\
         \n\
         - [ ] Stub milestone `m-{plan_id}` (phase-1 placeholder)\n\
         \n\
         _Machine-readable snapshot:_ `{}`\n",
        hist_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("snapshot.json")
    );
    tokio::fs::write(&global_path, md)
        .await
        .with_context(|| format!("write {}", global_path.display()))?;

    Ok(())
}

/// Workspace-relative safety: reject traversal outside `workspace_root`.
#[must_use]
pub fn plan_path_within_workspace(workspace_root: &Path, candidate: &Path) -> Option<PathBuf> {
    let root = workspace_root.canonicalize().ok()?;
    let joined = workspace_root.join(candidate);
    let cand = joined.canonicalize().ok()?;
    cand.starts_with(&root).then_some(cand)
}
