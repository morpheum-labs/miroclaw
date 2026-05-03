//! Mandatory guardrail gate invoked before a task enters [`super::TaskStatus::Running`].
//!
//! Phase 1 wires the default gate to existing [`SecurityPolicy`] and autonomy checks used
//! elsewhere for cron jobs. Finance-specific rules (LP Guardian / drawdown breakers, etc.)
//! belong in a future `TaskGuardrail` implementation behind `[tasks].lp_guardian_enabled`.

use std::fmt;

use crate::config::Config;
use crate::cron::{validate_shell_command_with_security, CronJob, JobType};
use crate::security::SecurityPolicy;

/// Structured rejection from the guardrail gate (deterministic; no LLM).
#[derive(Debug, Clone)]
pub struct GuardrailError(String);

impl GuardrailError {
    #[must_use]
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GuardrailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GuardrailError {}

/// Pluggable gate evaluated **before** a task is allowed to run.
///
/// Implementations must be deterministic and side-effect free except for policy counters
/// owned by [`SecurityPolicy`] (e.g. rate limits) where those already apply to cron today.
pub trait TaskGuardrail: Send + Sync {
    /// Validate a cron-backed task before execution.
    fn evaluate_cron_job(
        &self,
        security: &SecurityPolicy,
        config: &Config,
        job: &CronJob,
    ) -> Result<(), GuardrailError>;
}

/// Default gate: shell commands use `validate_shell_command_with_security`; agent/hand jobs
/// reuse the same autonomy / rate / budget checks as the scheduler paths.
pub struct DefaultTaskGuardrail;

impl TaskGuardrail for DefaultTaskGuardrail {
    fn evaluate_cron_job(
        &self,
        security: &SecurityPolicy,
        config: &Config,
        job: &CronJob,
    ) -> Result<(), GuardrailError> {
        if config.tasks.lp_guardian_enabled {
            // Extension point: mount LP-specific deterministic checks here when specified.
            let _ = config;
        }

        match job.job_type {
            JobType::Shell => validate_shell_command_with_security(security, &job.command, false)
                .map_err(|e| GuardrailError::new(e.to_string())),
            JobType::Agent => {
                if !security.can_act() {
                    return Err(GuardrailError::new(
                        "blocked by security policy: autonomy is read-only",
                    ));
                }
                if security.is_rate_limited() {
                    return Err(GuardrailError::new(
                        "blocked by security policy: rate limit exceeded",
                    ));
                }
                Ok(())
            }
            JobType::Hand => {
                if !security.can_act() {
                    return Err(GuardrailError::new(
                        "blocked by security policy: autonomy is read-only",
                    ));
                }
                if security.is_rate_limited() {
                    return Err(GuardrailError::new(
                        "blocked by security policy: rate limit exceeded",
                    ));
                }
                if job.command.trim().is_empty() {
                    return Err(GuardrailError::new(
                        "hand cron job has empty command (expected hand name)",
                    ));
                }
                Ok(())
            }
        }
    }
}
