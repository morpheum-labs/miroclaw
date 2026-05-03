//! Core task identity, status, and kind types for the task runtime.

use serde::{Deserialize, Serialize};

/// Opaque task identifier (UUID v4 string).
pub type TaskId = String;

/// Lifecycle state for a tracked work unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

impl TaskStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Killed => "killed",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "killed" => Ok(Self::Killed),
            other => Err(format!("unknown task status: {other}")),
        }
    }
}

/// High-level classification for observability and routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// A cron scheduler invocation (shell, agent, or hand).
    CronJobRun,
    /// Explicit operator / API spawned work.
    Manual,
    /// Bypasses full task recording (logged only when enabled).
    Bypass { reason: String },
    /// Parent row for an SOP run (see `sop_run_id`).
    SopChain,
    /// One step within an SOP chain.
    SopStep,
}

impl TaskKind {
    #[must_use]
    pub fn as_db_str(&self) -> String {
        match self {
            Self::CronJobRun => "cron_job".into(),
            Self::Manual => "manual".into(),
            Self::Bypass { .. } => "bypass".into(),
            Self::SopChain => "sop_chain".into(),
            Self::SopStep => "sop_step".into(),
        }
    }

    #[must_use]
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "cron_job" => Self::CronJobRun,
            "manual" => Self::Manual,
            "bypass" => Self::Bypass {
                reason: String::new(),
            },
            "sop_chain" => Self::SopChain,
            "sop_step" => Self::SopStep,
            _ => Self::Manual,
        }
    }
}

/// Serializable row for API / CLI / SSE consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: TaskId,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sop_run_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Outcome of executing a task body (distinct from `TaskStatus` for killed races).
#[derive(Debug, Clone)]
pub enum TaskOutcome {
    Success { summary: String },
    Failed { message: String },
}
