//! LLM-backed plan generation and Tier-2 critique (bounded).

use crate::config::{ModelRouteConfig, PlannerConfig};
use crate::hooks::planner_payloads::PlannerLevel;
use crate::providers::Provider;

use super::document::GlobalPlanDocument;

/// Force authoritative metadata after model output (IDs, session, level).
pub fn coerce_document_metadata(
    doc: &mut GlobalPlanDocument,
    plan_id: &str,
    version: u32,
    session_id: &str,
    goal: &str,
    level: PlannerLevel,
) {
    doc.plan_id = plan_id.to_string();
    doc.version = version;
    doc.session_id = session_id.to_string();
    doc.goal = goal.to_string();
    doc.level = level;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CritiqueReport {
    #[serde(default)]
    pub passes: Vec<String>,
    #[serde(default)]
    pub fails: Vec<String>,
    #[serde(default)]
    pub revision_suggestions: String,
    #[serde(default)]
    pub overall_risk: String,
}

#[must_use]
pub fn resolve_planner_model_id(
    planner_hint: Option<&str>,
    model_routes: &[ModelRouteConfig],
    loop_provider: &str,
    loop_model: &str,
) -> String {
    let Some(raw) = planner_hint.map(str::trim).filter(|s| !s.is_empty()) else {
        return loop_model.to_string();
    };
    let hint = raw.strip_prefix("hint:").unwrap_or(raw).trim();
    if hint.is_empty() {
        return loop_model.to_string();
    }
    if let Some(r) = model_routes
        .iter()
        .find(|r| r.hint.eq_ignore_ascii_case(hint))
    {
        if r.provider.eq_ignore_ascii_case(loop_provider) {
            return r.model.clone();
        }
        tracing::warn!(
            planner_hint = %hint,
            route_provider = %r.provider,
            loop_provider,
            "planner model route skipped — provider differs from loop provider"
        );
    }
    loop_model.to_string()
}

fn truncate_ctx(s: &str, max_chars: usize) -> String {
    if max_chars == 0 || s.len() <= max_chars {
        return s.to_string();
    }
    format!("{}…", &s[..max_chars])
}

/// Ask the model for a JSON [`GlobalPlanDocument`] only (no markdown fences required but tolerated).
#[allow(clippy::too_many_arguments)]
pub async fn generate_plan_document(
    provider: &dyn Provider,
    model: &str,
    temperature: f64,
    cfg: &PlannerConfig,
    level: PlannerLevel,
    goal: &str,
    session_id: &str,
    plan_id: &str,
    version: u32,
    prior_summary: Option<&str>,
    effective_tools: &[String],
    revision_hints: Option<&str>,
) -> anyhow::Result<GlobalPlanDocument> {
    let tools_line = effective_tools.join(", ");
    let prior = prior_summary.unwrap_or("(none)");
    let schema_hint = r#"Return ONLY a single JSON object (no markdown) with keys:
plan_id (string), version (number), level ("STRATEGIC"|"TACTICAL"|"OPERATIONAL"),
summary (string), milestones (array of {id, title, deps[], status ("pending"|"in_progress"|"done"|"blocked"), tool_names[], workspace_paths[], requires_approval (bool)}),
risks (string array), next_review (string or null), confidence (number or null),
pending_human_approval (bool), session_id (string), goal (string).
Use plan_id, version, session_id, goal exactly as given below."#;

    let rev = revision_hints
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("\n\nCritique revision hints (must address):\n{s}\n"))
        .unwrap_or_default();
    let user = format!(
        "{schema_hint}\n\nFixed fields:\nplan_id={plan_id}\nversion={version}\nsession_id={session_id}\ngoal={goal}\nlevel={level:?}\n\nPrior plan summary:\n{prior}\n\nAvailable tools (names only): {tools_line}\n{rev}\nProduce an executable plan aligned with autonomy and workspace safety.",
    );
    let system = format!(
        "You are ZeroClaw's unified planner. Output compact valid JSON only. Max milestones for OPERATIONAL level: {}.",
        cfg.operational_max_steps
    );
    let mut text = provider
        .chat_with_system(Some(system.as_str()), &user, model, temperature)
        .await?;
    text = truncate_ctx(&text, cfg.max_plan_context_chars.max(16_000));
    parse_document_json(&extract_json_payload(&text)?)
}

fn extract_json_payload(raw: &str) -> anyhow::Result<String> {
    let t = raw.trim();
    if let Some(i) = t.find("```json") {
        let rest = &t[i + 7..];
        if let Some(end) = rest.find("```") {
            return Ok(rest[..end].trim().to_string());
        }
    }
    if let Some(i) = t.find("```") {
        let rest = &t[i + 3..];
        if let Some(end) = rest.find("```") {
            return Ok(rest[..end].trim().to_string());
        }
    }
    Ok(t.to_string())
}

pub fn parse_document_json(json: &str) -> anyhow::Result<GlobalPlanDocument> {
    let v: GlobalPlanDocument = serde_json::from_str(json)?;
    Ok(v)
}

/// Tier-2 critique; returns report. Parsing failures yield empty fails + medium risk.
pub async fn run_critique(
    provider: &dyn Provider,
    model: &str,
    temperature: f64,
    cfg: &PlannerConfig,
    plan_json: &str,
) -> anyhow::Result<CritiqueReport> {
    let max = cfg.max_plan_context_chars;
    let plan_snippet: String = if max > 0 && plan_json.chars().count() > max {
        plan_json.chars().take(max).collect::<String>() + "\n… [truncated for critique budget]"
    } else {
        plan_json.to_string()
    };
    let user = format!(
        "You are the Critique Engine. Review ONLY the plan JSON below using modules: Error Monitoring, State Prediction, Feasibility, Alignment, Efficiency.\nReturn ONLY JSON: {{\"passes\":[\"...\"],\"fails\":[\"...\"],\"revision_suggestions\":\"...\",\"overall_risk\":\"low|medium|high\"}}\n\nPlan:\n{plan_snippet}",
    );
    let system = "Output valid JSON only.";
    let text = provider
        .chat_with_system(Some(system), &user, model, temperature.clamp(0.0, 0.3))
        .await?;
    let payload = extract_json_payload(&text)?;
    match serde_json::from_str::<CritiqueReport>(&payload) {
        Ok(r) => Ok(r),
        Err(e) => {
            tracing::warn!(error = %e, "planner critique JSON parse failed");
            Ok(CritiqueReport {
                passes: Vec::new(),
                fails: vec!["CritiqueParse".into()],
                revision_suggestions: text.chars().take(500).collect(),
                overall_risk: "medium".into(),
            })
        }
    }
}

#[must_use]
pub fn critique_requires_revision(r: &CritiqueReport) -> bool {
    !r.fails.is_empty() || r.overall_risk.eq_ignore_ascii_case("high")
}
