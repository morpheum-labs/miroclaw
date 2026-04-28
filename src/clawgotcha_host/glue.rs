//! Stub trait implementations until gateway `AppState` hot-reload is wired.

use async_trait::async_trait;
use clawgotcha::models::domain::{AgentDefinition, CronJobDefinition};
use clawgotcha::traits::{AgentRuntimeUpdater, ConfigReconciler, CronSchedulerUpdater};
use clawgotcha::{ChangeEvent, ClawgotchaError};

/// Logs-only delegate agent updates (replace with config hot-reload).
pub struct StubAgents;

#[async_trait]
impl AgentRuntimeUpdater for StubAgents {
    async fn upsert_agent(&self, def: &AgentDefinition) -> Result<(), ClawgotchaError> {
        tracing::debug!(
            agent = %def.name,
            revision = def.current_revision,
            "clawgotcha stub: upsert delegate agent"
        );
        Ok(())
    }

    async fn remove_agent(&self, name: &str) -> Result<(), ClawgotchaError> {
        tracing::debug!(%name, "clawgotcha stub: remove delegate agent");
        Ok(())
    }
}

/// Logs-only cron updates (replace with cron store writes shared with the gateway API).
pub struct StubCron;

#[async_trait]
impl CronSchedulerUpdater for StubCron {
    async fn upsert_job(&self, job: &CronJobDefinition) -> Result<(), ClawgotchaError> {
        tracing::debug!(job_id = %job.id, "clawgotcha stub: upsert cron job");
        Ok(())
    }

    async fn remove_job(&self, job_id: &str) -> Result<(), ClawgotchaError> {
        tracing::debug!(%job_id, "clawgotcha stub: remove cron job");
        Ok(())
    }
}

/// No-op batch reconcile (extend when events carry enough payload to patch `Config` atomically).
pub struct StubReconciler;

#[async_trait]
impl ConfigReconciler for StubReconciler {
    async fn apply_batch(&self, events: Vec<ChangeEvent>) -> Result<(), ClawgotchaError> {
        tracing::debug!(count = events.len(), "clawgotcha stub: reconcile batch");
        Ok(())
    }
}
