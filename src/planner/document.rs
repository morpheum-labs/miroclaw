//! Versioned global plan document (serde) — source for `plan_current.json` and `GLOBAL_PLAN.md`.

use std::fmt::Write as _;

use crate::hooks::planner_payloads::PlannerLevel;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Milestone lifecycle for execution tracking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanMilestone {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub deps: Vec<String>,
    pub status: MilestoneStatus,
    #[serde(default)]
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub workspace_paths: Vec<String>,
    #[serde(default)]
    pub requires_approval: bool,
}

/// Serializable global plan (machine-readable).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GlobalPlanDocument {
    pub plan_id: String,
    pub version: u32,
    pub level: PlannerLevel,
    pub summary: String,
    #[serde(default)]
    pub milestones: Vec<PlanMilestone>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub next_review: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub pending_human_approval: bool,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub goal: String,
}

impl GlobalPlanDocument {
    /// Human-readable markdown (dashboard + git-friendly).
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Global plan\n\n");
        let _ = writeln!(out, "**Plan ID:** `{}`  ", self.plan_id);
        let _ = writeln!(out, "**Version:** {}  ", self.version);
        let _ = writeln!(out, "**Level:** {:?}  ", self.level);
        let _ = writeln!(out, "**Session:** `{}`  ", self.session_id);
        if self.pending_human_approval {
            out.push_str("**Status:** pending human review  \n");
        }
        out.push('\n');
        out.push_str("## Summary\n\n");
        out.push_str(&self.summary);
        out.push_str("\n\n## Goal\n\n");
        out.push_str(&self.goal);
        out.push_str("\n\n## Milestones\n\n");
        for m in &self.milestones {
            let mark = match m.status {
                MilestoneStatus::Done => 'x',
                _ => ' ',
            };
            let appr = if m.requires_approval {
                " _(approval)_"
            } else {
                ""
            };
            let _ = writeln!(out, "- [{}] **{}** — {}{}", mark, m.id, m.title, appr);
            if !m.deps.is_empty() {
                let _ = writeln!(out, "  - deps: `{}`", m.deps.join("`, `"));
            }
            if !m.tool_names.is_empty() {
                let _ = writeln!(out, "  - tools: `{}`", m.tool_names.join("`, `"));
            }
        }
        if !self.risks.is_empty() {
            out.push_str("\n## Risks\n\n");
            for r in &self.risks {
                let _ = writeln!(out, "- {r}");
            }
        }
        if let Some(ref nr) = self.next_review {
            out.push_str("\n## Next review\n\n");
            out.push_str(nr);
            out.push('\n');
        }
        out.push_str("\n---\n\n_Machine-readable: `plan_current.json`_\n");
        out
    }
}
