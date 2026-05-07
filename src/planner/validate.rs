//! Tier-1 deterministic validation for planner output.

use crate::hooks::planner_payloads::PlannerLevel;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

use super::document::{GlobalPlanDocument, PlanMilestone};

#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    pub ok: bool,
    pub reasons: Vec<String>,
}

impl ValidationOutcome {
    fn fail(reason: impl Into<String>) -> Self {
        Self {
            ok: false,
            reasons: vec![reason.into()],
        }
    }

    fn merge(mut self, other: Self) -> Self {
        if !other.ok {
            self.ok = false;
            self.reasons.extend(other.reasons);
        }
        self
    }

    #[must_use]
    pub fn ok() -> Self {
        Self {
            ok: true,
            reasons: Vec::new(),
        }
    }
}

/// Validate tools, workspace paths, DAG, and operational step counts.
#[must_use]
pub fn validate_document(
    doc: &GlobalPlanDocument,
    workspace_dir: &Path,
    effective_tools: &[String],
    operational_max_steps: usize,
) -> ValidationOutcome {
    let mut out = ValidationOutcome::ok();
    let allowed: HashSet<&str> = effective_tools.iter().map(String::as_str).collect();

    if doc.plan_id.trim().is_empty() {
        out = out.merge(ValidationOutcome::fail("plan_id must be non-empty"));
    }

    if doc.level == PlannerLevel::Operational && doc.milestones.len() > operational_max_steps {
        out = out.merge(ValidationOutcome::fail(format!(
            "operational plan has {} milestones (max {operational_max_steps})",
            doc.milestones.len()
        )));
    }

    // Tool allowlist
    for m in &doc.milestones {
        for t in &m.tool_names {
            if t.trim().is_empty() {
                continue;
            }
            if !allowed.contains(t.as_str()) {
                out = out.merge(ValidationOutcome::fail(format!(
                    "unknown or disallowed tool `{t}` in milestone {}",
                    m.id
                )));
            }
        }
        for p in &m.workspace_paths {
            if !workspace_relative_ok(workspace_dir, p) {
                out = out.merge(ValidationOutcome::fail(format!(
                    "invalid workspace-relative path in milestone {}: {p}",
                    m.id
                )));
            }
        }
    }

    // DAG cycle check on milestone ids
    let ids: HashSet<&str> = doc.milestones.iter().map(|m| m.id.as_str()).collect();
    for m in &doc.milestones {
        for d in &m.deps {
            if !ids.contains(d.as_str()) {
                out = out.merge(ValidationOutcome::fail(format!(
                    "milestone {} depends on missing id `{d}`",
                    m.id
                )));
            }
        }
    }
    if has_cycle_milestones(&doc.milestones) {
        out = out.merge(ValidationOutcome::fail(
            "milestone dependency graph has a cycle",
        ));
    }

    out
}

fn workspace_relative_ok(workspace_dir: &Path, raw: &str) -> bool {
    let raw = raw.trim();
    if raw.is_empty() {
        return true;
    }
    let p = Path::new(raw);
    if p.is_absolute() {
        return false;
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let joined = workspace_dir.join(p);
    joined.starts_with(workspace_dir)
}

fn has_cycle_milestones(milestones: &[PlanMilestone]) -> bool {
    let mut g: HashMap<String, Vec<String>> = HashMap::new();
    for m in milestones {
        g.insert(m.id.clone(), m.deps.clone());
    }
    let mut visiting: HashSet<String> = HashSet::new();
    let mut done: HashSet<String> = HashSet::new();
    for id in g.keys().cloned().collect::<Vec<_>>() {
        if dfs(&g, &id, &mut visiting, &mut done) {
            return true;
        }
    }
    false
}

fn dfs(
    g: &HashMap<String, Vec<String>>,
    id: &str,
    visiting: &mut HashSet<String>,
    done: &mut HashSet<String>,
) -> bool {
    if done.contains(id) {
        return false;
    }
    if visiting.contains(id) {
        return true;
    }
    visiting.insert(id.to_string());
    if let Some(deps) = g.get(id) {
        for d in deps {
            if dfs(g, d.as_str(), visiting, done) {
                return true;
            }
        }
    }
    visiting.remove(id);
    done.insert(id.to_string());
    false
}
