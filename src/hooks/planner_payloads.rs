//! Payload types for unified planner lifecycle hooks (`planner.plan_requested` / `planner.plan_updated`).

use crate::security::AutonomyLevel;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Agent planning level (strategic / tactical / operational).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlannerLevel {
    Strategic,
    Tactical,
    Operational,
}

/// Coarse lifecycle state for a stored plan version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Active,
    Superseded,
    Failed,
}

/// Emitted before a planning attempt (void hook).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRequestedPayload {
    pub goal: String,
    pub level: PlannerLevel,
    pub session_id: String,
    pub autonomy_level: AutonomyLevel,
    pub parent_plan_id: Option<String>,
}

/// Emitted after a plan artifact is written (void hook).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanUpdatedPayload {
    pub plan_id: String,
    pub version: u32,
    pub updated_milestones: Vec<String>,
    pub status: PlanStatus,
    pub triggered_by: String,
}
