//! Deterministic merge / feedback hooks on [`GlobalPlanDocument`].

use super::document::GlobalPlanDocument;

/// Cap appended feedback rows so post-turn hooks cannot grow the JSON without bound.
const MAX_PLANNER_FEEDBACK_RISKS: usize = 64;

/// Lightweight heuristic: append risk notes when the assistant summary signals failure.
pub fn apply_turn_completion_heuristic(doc: &mut GlobalPlanDocument, assistant_summary: &str) {
    let low = assistant_summary.to_ascii_lowercase();
    if low.contains("failed")
        || low.contains("error")
        || low.contains("permission denied")
        || low.contains("blocked")
    {
        if doc.risks.len() >= MAX_PLANNER_FEEDBACK_RISKS {
            return;
        }
        let snippet: String = assistant_summary.chars().take(200).collect();
        doc.risks
            .push(format!("Execution signal (heuristic): {snippet}"));
    }
}
