//! Trigger gating for planner runs (`[planner].trigger_mode`).

use crate::config::{PlannerConfig, PlannerTriggerMode};

/// Returns true when this turn should run the planner pipeline (before session bookkeeping).
pub async fn should_fire_planner_trigger(
    cfg: &PlannerConfig,
    goal: &str,
    session_id: &str,
    workspace_dir: &std::path::Path,
) -> anyhow::Result<bool> {
    if !cfg.enabled {
        return Ok(false);
    }
    let goal_trim = goal.trim();
    if goal_trim.is_empty() {
        return Ok(false);
    }

    Ok(match cfg.trigger_mode {
        PlannerTriggerMode::EveryMessage => true,
        PlannerTriggerMode::FirstSessionTurn => {
            !super::state::session_already_invoked(workspace_dir, session_id).await?
        }
        PlannerTriggerMode::Keyword => cfg.trigger_keywords.iter().any(|k| {
            let k = k.trim();
            !k.is_empty()
                && goal_trim
                    .to_ascii_lowercase()
                    .contains(&k.to_ascii_lowercase())
        }),
        PlannerTriggerMode::Directive => cfg.directive_prefixes.iter().any(|prefix| {
            let p = prefix.trim();
            if p.is_empty() {
                return false;
            }
            let g = goal_trim;
            if p.starts_with('/') {
                let gl = g.to_ascii_lowercase();
                let pl = p.to_ascii_lowercase();
                gl.starts_with(&pl)
            } else {
                g.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase())
            }
        }),
    })
}

/// User message starts a brand-new plan id when it matches any configured prefix (trimmed).
#[must_use]
pub fn message_starts_new_goal(cfg: &PlannerConfig, goal: &str) -> bool {
    let t = goal.trim();
    cfg.new_goal_prefixes.iter().any(|p| {
        let p = p.trim();
        !p.is_empty() && t.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase())
    })
}
