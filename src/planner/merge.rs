//! Deterministic merge / feedback hooks on [`GlobalPlanDocument`].

use super::document::GlobalPlanDocument;

/// Lightweight heuristic: append risk notes when the assistant summary signals failure.
pub fn apply_turn_completion_heuristic(doc: &mut GlobalPlanDocument, assistant_summary: &str) {
    let low = assistant_summary.to_ascii_lowercase();
    if low.contains("failed")
        || low.contains("error")
        || low.contains("permission denied")
        || low.contains("blocked")
    {
        let snippet: String = assistant_summary.chars().take(200).collect();
        doc.risks
            .push(format!("Execution signal (heuristic): {snippet}"));
    }
}
