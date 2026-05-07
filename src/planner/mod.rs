//! Unified hierarchical planner: triggers, persistence, optional LLM generation, Tier-1 validation,
//! bounded Tier-2 critique, and workspace artifacts (`GLOBAL_PLAN.md`, `plan_current.json`).

mod document;
mod llm;
mod merge;
mod state;
mod trigger;
mod validate;

pub use merge::apply_turn_completion_heuristic;

pub use crate::hooks::planner_payloads::{
    PlanRequestedPayload, PlanStatus, PlanUpdatedPayload, PlannerLevel,
};
pub use document::{GlobalPlanDocument, MilestoneStatus};
pub type PlanMilestone = document::PlanMilestone;
pub use llm::CritiqueReport;

use crate::config::{PlannerConfig, PlannerTriggerMode};
use crate::hooks::HookRunner;
use crate::providers::Provider;
use anyhow::Context;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Minimum non-zero per-call planner LLM outer timeout (seconds).
pub const PLANNER_LLM_TIMEOUT_MIN_SECS: u64 = 5;
/// Maximum per-call planner LLM outer timeout (seconds).
pub const PLANNER_LLM_TIMEOUT_MAX_SECS: u64 = 3600;

/// Resolve `[planner].llm_call_timeout_secs`: inherit `provider_timeout_secs`, clamp, or `0` = disable outer timeout.
#[must_use]
pub fn resolve_planner_llm_call_timeout_secs(
    planner_override: Option<u64>,
    provider_timeout_secs: u64,
) -> u64 {
    let raw = planner_override.unwrap_or(provider_timeout_secs);
    if raw == 0 {
        return 0;
    }
    raw.clamp(PLANNER_LLM_TIMEOUT_MIN_SECS, PLANNER_LLM_TIMEOUT_MAX_SECS)
}

async fn run_with_planner_llm_timeout<T, F>(
    timeout_secs: u64,
    label: &'static str,
    fut: F,
) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>> + Send,
    T: Send,
{
    if timeout_secs == 0 {
        return fut.await;
    }
    match tokio::time::timeout(Duration::from_secs(timeout_secs), fut).await {
        Ok(inner) => inner,
        Err(_) => {
            tracing::warn!(timeout_secs, label, "planner LLM call timed out");
            anyhow::bail!("planner LLM {label} timed out after {timeout_secs}s")
        }
    }
}

/// Single path segment derived from `plan_id` (UUIDs pass through; strips separators).
#[must_use]
pub fn fs_safe_plan_segment(plan_id: &str) -> String {
    const MAX: usize = 96;
    let s: String = plan_id
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '/' | '\\' | ':' | '\0' | '<' | '>' | '|' | '"' | '*' | '?'
            )
        })
        .take(MAX)
        .collect();
    let t = s.trim();
    if t.is_empty() {
        "plan".to_string()
    } else {
        t.to_string()
    }
}

/// References for optional LLM planner calls (same process provider as the tool loop).
#[derive(Clone, Copy)]
pub struct PlannerLlmRefs<'a> {
    pub provider: &'a dyn Provider,
    pub provider_name: &'a str,
    pub model: &'a str,
    pub temperature: f64,
    pub model_routes: &'a [crate::config::ModelRouteConfig],
}

/// Finite-state planner workflow (audit-oriented tracing labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

fn log_transition(state: PlannerState, detail: Option<&str>) {
    match detail {
        Some(d) => tracing::info!(planner_state = ?state, "{d}"),
        None => tracing::info!(planner_state = ?state, "planner transition"),
    }
}

#[must_use]
pub fn resolve_planner_level(cfg: &PlannerConfig) -> PlannerLevel {
    if let Some(ref s) = cfg.forced_level {
        match s.to_ascii_lowercase().as_str() {
            "strategic" => PlannerLevel::Strategic,
            "tactical" => PlannerLevel::Tactical,
            _ => PlannerLevel::Operational,
        }
    } else {
        PlannerLevel::Operational
    }
}

async fn load_current_document(workspace_dir: &Path) -> Option<GlobalPlanDocument> {
    let p = workspace_dir.join("plan_current.json");
    let raw = tokio::fs::read_to_string(&p).await.ok()?;
    serde_json::from_str(&raw).ok()
}

#[allow(clippy::too_many_arguments)]
async fn persist_plan_workspace(
    workspace_dir: &Path,
    doc: &GlobalPlanDocument,
    hooks: Option<&HookRunner>,
    triggered_by: &str,
) -> anyhow::Result<PathBuf> {
    let plan_dir = workspace_dir.join("plan_history");
    tokio::fs::create_dir_all(&plan_dir)
        .await
        .with_context(|| format!("mkdir {}", plan_dir.display()))?;

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let safe_id = fs_safe_plan_segment(&doc.plan_id);
    let hist_path = plan_dir.join(format!("{stamp}_{safe_id}.json"));
    let json = serde_json::to_string_pretty(doc)?;
    tokio::fs::write(&hist_path, &json)
        .await
        .with_context(|| format!("write {}", hist_path.display()))?;

    let current = workspace_dir.join("plan_current.json");
    tokio::fs::write(&current, json)
        .await
        .with_context(|| format!("write {}", current.display()))?;

    let global_path = workspace_dir.join("GLOBAL_PLAN.md");
    tokio::fs::write(&global_path, doc.to_markdown())
        .await
        .with_context(|| format!("write {}", global_path.display()))?;

    let milestones: Vec<String> = doc.milestones.iter().map(|m| m.id.clone()).collect();
    let updated = PlanUpdatedPayload {
        plan_id: doc.plan_id.clone(),
        version: doc.version,
        updated_milestones: milestones,
        status: PlanStatus::Active,
        triggered_by: triggered_by.to_string(),
    };
    if let Some(h) = hooks {
        h.fire_planner_plan_updated(&updated).await;
    }

    Ok(hist_path)
}

async fn append_critique_log(
    workspace_dir: &Path,
    plan_id: &str,
    report: &CritiqueReport,
) -> anyhow::Result<()> {
    let dir = workspace_dir.join("plan_critiques");
    tokio::fs::create_dir_all(&dir).await?;
    let day = chrono::Local::now().format("%Y-%m-%d");
    let safe_id = fs_safe_plan_segment(plan_id);
    let path = dir.join(format!("{day}_{safe_id}.jsonl"));
    let line = serde_json::to_string(report)?;
    let mut opts = tokio::fs::OpenOptions::new();
    opts.create(true).append(true);
    let mut f = opts.open(&path).await?;
    use tokio::io::AsyncWriteExt;
    f.write_all(format!("{line}\n").as_bytes()).await?;
    f.flush().await?;
    Ok(())
}

fn build_stub_document(
    plan_id: &str,
    version: u32,
    level: PlannerLevel,
    goal: &str,
    session_id: &str,
) -> GlobalPlanDocument {
    GlobalPlanDocument {
        plan_id: plan_id.to_string(),
        version,
        level,
        summary: "Stub plan (LLM disabled or unavailable).".into(),
        milestones: vec![document::PlanMilestone {
            id: format!("m-{plan_id}"),
            title: "Complete the stated goal using available tools.".into(),
            deps: Vec::new(),
            status: MilestoneStatus::Pending,
            tool_names: Vec::new(),
            workspace_paths: Vec::new(),
            requires_approval: false,
        }],
        risks: Vec::new(),
        next_review: None,
        confidence: None,
        pending_human_approval: false,
        session_id: session_id.to_string(),
        goal: goal.to_string(),
    }
}

fn inject_plan_user_message(
    history: &mut Vec<crate::providers::ChatMessage>,
    doc: &GlobalPlanDocument,
) {
    let block = format!(
        "### Unified planner\n\nPlan `{}` v{} — {}\n\n_(Structured plan persisted to workspace.)_",
        doc.plan_id, doc.version, doc.summary
    );
    if history.len() > 1 {
        history.insert(1, crate::providers::ChatMessage::user(block));
    } else {
        history.push(crate::providers::ChatMessage::user(block));
    }
}

/// Run planner pipeline when triggers and config permit (iteration 0 caller).
#[allow(clippy::too_many_arguments)]
pub async fn run_planner_turn_if_eligible(
    cfg: &PlannerConfig,
    workspace_dir: &Path,
    hooks: Option<&HookRunner>,
    turn_user_message: Option<&str>,
    session_id: &str,
    autonomy: crate::security::AutonomyLevel,
    parent_plan_id: Option<&str>,
    llm: Option<PlannerLlmRefs<'_>>,
    llm_call_timeout_secs: u64,
    cancel: Option<&CancellationToken>,
    effective_tools: &[String],
    history: &mut Vec<crate::providers::ChatMessage>,
) {
    log_transition(PlannerState::Idle, None);
    if !cfg.enabled {
        return;
    }
    if cancel.is_some_and(|c| c.is_cancelled()) {
        tracing::trace!("planner skipped: cancelled before start");
        return;
    }
    let Some(goal_raw) = turn_user_message.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };

    log_transition(
        PlannerState::ContextLoading,
        Some("loading planner context"),
    );
    let goal = goal_raw.to_string();
    let trigger_ok = match crate::planner::trigger::should_fire_planner_trigger(
        cfg,
        &goal,
        session_id,
        workspace_dir,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "planner trigger check failed");
            return;
        }
    };
    if !trigger_ok {
        return;
    }

    let level = resolve_planner_level(cfg);

    let mut pointer = match state::load_active_pointer(workspace_dir).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "planner active pointer load failed");
            state::PlannerActivePointer::default()
        }
    };

    let new_goal = crate::planner::trigger::message_starts_new_goal(cfg, &goal);
    let plan_id = if pointer.plan_id.is_empty() || new_goal {
        Uuid::new_v4().to_string()
    } else {
        pointer.plan_id.clone()
    };
    let version = if pointer.plan_id.is_empty() || new_goal || plan_id != pointer.plan_id {
        1
    } else {
        pointer.version.saturating_add(1)
    };

    log_transition(PlannerState::PlanGeneration, Some("plan generation"));

    let req = PlanRequestedPayload {
        goal: goal.clone(),
        level,
        session_id: session_id.to_string(),
        autonomy_level: autonomy,
        parent_plan_id: parent_plan_id.map(String::from),
    };
    if let Some(h) = hooks {
        h.fire_planner_plan_requested(&req).await;
    }

    let planner_model = llm.as_ref().map(|l| {
        llm::resolve_planner_model_id(
            cfg.model_hint.as_deref(),
            l.model_routes,
            l.provider_name,
            l.model,
        )
    });

    let mut doc = if cfg.llm_enabled {
        match (&llm, planner_model.as_ref()) {
            (Some(llmrefs), Some(pm)) => {
                let prior = load_current_document(workspace_dir)
                    .await
                    .map(|d| d.summary)
                    .unwrap_or_default();
                let prior_ref = (!prior.trim().is_empty()).then_some(prior.as_str());
                match run_with_planner_llm_timeout(
                    llm_call_timeout_secs,
                    "plan_generation",
                    llm::generate_plan_document(
                        llmrefs.provider,
                        pm.as_str(),
                        llmrefs.temperature,
                        cfg,
                        level,
                        &goal,
                        session_id,
                        &plan_id,
                        version,
                        prior_ref,
                        effective_tools,
                        None,
                    ),
                )
                .await
                {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!(error = %e, "planner LLM generation failed; using stub");
                        build_stub_document(&plan_id, version, level, &goal, session_id)
                    }
                }
            }
            _ => {
                if cfg.llm_enabled {
                    tracing::warn!(
                        "planner llm_enabled but no provider context in loop; using stub"
                    );
                }
                build_stub_document(&plan_id, version, level, &goal, session_id)
            }
        }
    } else {
        build_stub_document(&plan_id, version, level, &goal, session_id)
    };

    llm::coerce_document_metadata(&mut doc, &plan_id, version, session_id, &goal, level);

    let v1 = validate::validate_document(
        &doc,
        workspace_dir,
        effective_tools,
        cfg.operational_max_steps,
    );
    if !v1.ok {
        tracing::warn!(reasons = ?v1.reasons, "planner Tier-1 validation failed");
        return;
    }

    if cfg.llm_enabled && cfg.critique_enabled {
        if let (Some(llmrefs), Some(pm)) = (&llm, planner_model.as_ref()) {
            log_transition(PlannerState::CritiqueRevision, Some("critique"));
            let mut revision_round: u32 = 0;
            loop {
                if cancel.is_some_and(|c| c.is_cancelled()) {
                    tracing::trace!("planner critique loop stopped: cancelled");
                    break;
                }
                let plan_json = match serde_json::to_string(&doc) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(error = %e, "planner serialize for critique failed");
                        break;
                    }
                };
                let report = match run_with_planner_llm_timeout(
                    llm_call_timeout_secs,
                    "critique",
                    llm::run_critique(
                        llmrefs.provider,
                        pm.as_str(),
                        llmrefs.temperature,
                        cfg,
                        &plan_json,
                    ),
                )
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(error = %e, "planner critique failed");
                        break;
                    }
                };
                if cfg.persist_critiques {
                    let _ = append_critique_log(workspace_dir, &doc.plan_id, &report).await;
                }
                if !llm::critique_requires_revision(&report) {
                    break;
                }
                if revision_round >= cfg.max_critique_revisions {
                    doc.pending_human_approval = true;
                    log_transition(
                        PlannerState::ApprovalCheck,
                        Some("critique escalated — pending human approval"),
                    );
                    break;
                }
                revision_round += 1;
                if cancel.is_some_and(|c| c.is_cancelled()) {
                    tracing::trace!("planner revision skipped: cancelled");
                    break;
                }
                match run_with_planner_llm_timeout(
                    llm_call_timeout_secs,
                    "plan_revision",
                    llm::generate_plan_document(
                        llmrefs.provider,
                        pm.as_str(),
                        llmrefs.temperature,
                        cfg,
                        level,
                        &goal,
                        session_id,
                        &plan_id,
                        version,
                        Some(doc.summary.as_str()),
                        effective_tools,
                        Some(report.revision_suggestions.as_str()),
                    ),
                )
                .await
                {
                    Ok(mut d) => {
                        llm::coerce_document_metadata(
                            &mut d, &plan_id, version, session_id, &goal, level,
                        );
                        let v2 = validate::validate_document(
                            &d,
                            workspace_dir,
                            effective_tools,
                            cfg.operational_max_steps,
                        );
                        if !v2.ok {
                            tracing::warn!(reasons = ?v2.reasons, "planner revision failed Tier-1");
                            break;
                        }
                        doc = d;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "planner revision generation failed");
                        break;
                    }
                }
            }
        }
    }

    log_transition(PlannerState::GlobalPlanUpdate, Some("persist"));
    let triggered_by = "user";
    if let Err(e) = persist_plan_workspace(workspace_dir, &doc, hooks, triggered_by).await {
        tracing::warn!(error = %e, "planner persist failed");
        return;
    }

    pointer.plan_id = plan_id;
    pointer.version = version;
    if let Err(e) = state::save_active_pointer(workspace_dir, &pointer).await {
        tracing::warn!(error = %e, "planner active pointer save failed");
    }

    if cfg.trigger_mode == PlannerTriggerMode::FirstSessionTurn {
        if let Err(e) = state::mark_session_invoked(workspace_dir, session_id).await {
            tracing::warn!(error = %e, "planner session index update failed");
        }
    }

    if cfg.inject_into_history {
        inject_plan_user_message(history, &doc);
    }

    log_transition(PlannerState::Idle, Some("complete"));
}

/// Workspace-relative safety: reject traversal outside `workspace_root`.
#[must_use]
pub fn plan_path_within_workspace(workspace_root: &Path, candidate: &Path) -> Option<PathBuf> {
    let root = workspace_root.canonicalize().ok()?;
    let joined = workspace_root.join(candidate);
    let cand = joined.canonicalize().ok()?;
    cand.starts_with(&root).then_some(cand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PlannerTriggerMode;

    #[test]
    fn resolve_planner_timeout_inherits_clamps_and_zero_disables() {
        assert_eq!(resolve_planner_llm_call_timeout_secs(None, 120), 120);
        assert_eq!(resolve_planner_llm_call_timeout_secs(Some(0), 999), 0);
        assert_eq!(
            resolve_planner_llm_call_timeout_secs(Some(3), 120),
            PLANNER_LLM_TIMEOUT_MIN_SECS
        );
        assert_eq!(
            resolve_planner_llm_call_timeout_secs(Some(5000), 120),
            PLANNER_LLM_TIMEOUT_MAX_SECS
        );
    }

    #[tokio::test]
    async fn planner_llm_timeout_wrapper_errors_fast() {
        let fut = async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok::<String, anyhow::Error>("x".into())
        };
        let r = super::run_with_planner_llm_timeout(1, "test", fut).await;
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn planner_llm_timeout_zero_passes_through() {
        let r =
            super::run_with_planner_llm_timeout(0, "noop", async { Ok::<(), anyhow::Error>(()) })
                .await;
        assert!(r.is_ok());
    }

    #[test]
    fn workspace_relative_ok_rejects_dotdot() {
        let tmp = PathBuf::from("/tmp/workspace");
        assert!(
            !validate::validate_document(
                &GlobalPlanDocument {
                    plan_id: "p".into(),
                    version: 1,
                    level: PlannerLevel::Operational,
                    summary: "s".into(),
                    milestones: vec![document::PlanMilestone {
                        id: "a".into(),
                        title: "t".into(),
                        deps: vec![],
                        status: MilestoneStatus::Pending,
                        tool_names: vec![],
                        workspace_paths: vec!["../etc/passwd".into()],
                        requires_approval: false,
                    }],
                    risks: vec![],
                    next_review: None,
                    confidence: None,
                    pending_human_approval: false,
                    session_id: "x".into(),
                    goal: "g".into(),
                },
                &tmp,
                &[],
                5,
            )
            .ok
        );
    }

    #[tokio::test]
    async fn first_session_trigger_requires_mark() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = PlannerConfig {
            enabled: true,
            trigger_mode: PlannerTriggerMode::FirstSessionTurn,
            ..PlannerConfig::default()
        };
        assert!(
            trigger::should_fire_planner_trigger(&cfg, "hello", "s1", dir.path())
                .await
                .unwrap()
        );
        state::mark_session_invoked(dir.path(), "s1").await.unwrap();
        assert!(
            !trigger::should_fire_planner_trigger(&cfg, "hello", "s1", dir.path())
                .await
                .unwrap()
        );
        cfg.trigger_mode = PlannerTriggerMode::EveryMessage;
        assert!(
            trigger::should_fire_planner_trigger(&cfg, "hello", "s1", dir.path())
                .await
                .unwrap()
        );
    }

    #[test]
    fn tier1_rejects_dependency_cycle() {
        let tmp = PathBuf::from("/tmp/workspace");
        let doc = GlobalPlanDocument {
            plan_id: "p".into(),
            version: 1,
            level: PlannerLevel::Operational,
            summary: "s".into(),
            milestones: vec![
                document::PlanMilestone {
                    id: "a".into(),
                    title: "a".into(),
                    deps: vec!["b".into()],
                    status: MilestoneStatus::Pending,
                    tool_names: vec![],
                    workspace_paths: vec![],
                    requires_approval: false,
                },
                document::PlanMilestone {
                    id: "b".into(),
                    title: "b".into(),
                    deps: vec!["a".into()],
                    status: MilestoneStatus::Pending,
                    tool_names: vec![],
                    workspace_paths: vec![],
                    requires_approval: false,
                },
            ],
            risks: vec![],
            next_review: None,
            confidence: None,
            pending_human_approval: false,
            session_id: "x".into(),
            goal: "g".into(),
        };
        let v = validate::validate_document(&doc, &tmp, &[], 5);
        assert!(!v.ok);
        assert!(v.reasons.iter().any(|r| r.contains("cycle")));
    }

    #[test]
    fn fs_safe_plan_segment_strips_path_chars() {
        assert_eq!(fs_safe_plan_segment("abc/def:g\\h"), "abcdefgh");
        assert_eq!(fs_safe_plan_segment("   "), "plan");
    }

    #[test]
    fn merge_heuristic_appends_risk_on_failure_signal() {
        let mut doc = GlobalPlanDocument {
            plan_id: "p".into(),
            version: 1,
            level: PlannerLevel::Operational,
            summary: "s".into(),
            milestones: vec![],
            risks: vec![],
            next_review: None,
            confidence: None,
            pending_human_approval: false,
            session_id: "x".into(),
            goal: "g".into(),
        };
        merge::apply_turn_completion_heuristic(&mut doc, "The shell command failed with error 1");
        assert_eq!(doc.risks.len(), 1);
        assert!(doc.risks[0].contains("Execution signal"));
    }
}
