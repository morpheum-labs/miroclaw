//! Post-turn heuristic merge into `plan_current.json` when `[planner].feedback_hook_enabled`.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::hooks::traits::HookHandler;
use crate::planner::apply_turn_completion_heuristic;
use crate::planner::GlobalPlanDocument;

pub struct PlannerFeedbackHook {
    workspace_dir: PathBuf,
}

impl PlannerFeedbackHook {
    #[must_use]
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }

    fn plan_path(&self) -> PathBuf {
        self.workspace_dir.join("plan_current.json")
    }
}

#[async_trait]
impl HookHandler for PlannerFeedbackHook {
    fn name(&self) -> &str {
        "planner_feedback"
    }

    async fn on_after_turn_completed(
        &self,
        _channel: &str,
        _user_message: &str,
        assistant_summary: &str,
    ) {
        let path = self.plan_path();
        if !path.exists() {
            return;
        }
        let Ok(raw) = tokio::fs::read_to_string(&path).await else {
            return;
        };
        let Ok(mut doc) = serde_json::from_str::<GlobalPlanDocument>(&raw) else {
            return;
        };
        apply_turn_completion_heuristic(&mut doc, assistant_summary);
        let Ok(json) = serde_json::to_string_pretty(&doc) else {
            return;
        };
        let _ = tokio::fs::write(&path, json).await;
        let md_path = self.workspace_dir.join("GLOBAL_PLAN.md");
        let _ = tokio::fs::write(&md_path, doc.to_markdown()).await;
    }
}
