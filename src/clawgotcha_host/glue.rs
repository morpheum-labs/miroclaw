//! Clawgotcha host adapters: persist delegate agents + cron rows and merge swarm defaults.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use clawgotcha::models::domain::{AgentDefinition, CronJobDefinition, SwarmDefaults};
use clawgotcha::traits::{AgentRuntimeUpdater, ConfigReconciler, CronSchedulerUpdater};
use clawgotcha::{ChangeEvent, ClawgotchaError};
use parking_lot::{Mutex, RwLock};

use crate::config::{Config, DelegateAgentConfig};

/// Updates `[agents]` on disk and keeps the delegate tool snapshot in sync when provided.
pub struct HostAgents {
    pub config: Arc<Mutex<Config>>,
    pub delegate_agents: Option<Arc<RwLock<HashMap<String, DelegateAgentConfig>>>>,
}

#[async_trait]
impl AgentRuntimeUpdater for HostAgents {
    async fn upsert_agent(&self, def: &AgentDefinition) -> Result<(), ClawgotchaError> {
        let snapshot = {
            let mut cfg = self.config.lock();
            cfg.agents
                .insert(def.name.clone(), DelegateAgentConfig::from(def));
            cfg.clone()
        };
        snapshot
            .save()
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("save config: {e}")))?;
        if let Some(cell) = &self.delegate_agents {
            *cell.write() = snapshot.agents.clone();
        }
        crate::tools::mcp_vault::invalidate_mcp_scoped_state();
        tracing::info!(agent = %def.name, revision = def.current_revision, "clawgotcha: upserted delegate agent");
        Ok(())
    }

    async fn remove_agent(&self, name: &str) -> Result<(), ClawgotchaError> {
        let snapshot = {
            let mut cfg = self.config.lock();
            cfg.agents.remove(name);
            cfg.clone()
        };
        snapshot
            .save()
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("save config: {e}")))?;
        if let Some(cell) = &self.delegate_agents {
            *cell.write() = snapshot.agents.clone();
        }
        crate::tools::mcp_vault::invalidate_mcp_scoped_state();
        tracing::info!(%name, "clawgotcha: removed delegate agent");
        Ok(())
    }
}

/// Upserts agent-type cron rows via `cron::store` (stable job id).
pub struct HostCron {
    pub config: Arc<Mutex<Config>>,
}

#[async_trait]
impl CronSchedulerUpdater for HostCron {
    async fn upsert_job(&self, job: &CronJobDefinition) -> Result<(), ClawgotchaError> {
        let cfg = self.config.lock().clone();
        let prompt = job.prompt.clone().unwrap_or_default();
        crate::cron::upsert_clawgotcha_agent_job(
            &cfg,
            &job.id,
            &job.expression,
            &prompt,
            job.enabled,
        )
        .map_err(|e| ClawgotchaError::Validation(format!("cron upsert: {e}")))?;
        tracing::info!(job_id = %job.id, "clawgotcha: upserted cron job");
        Ok(())
    }

    async fn remove_job(&self, job_id: &str) -> Result<(), ClawgotchaError> {
        let cfg = self.config.lock().clone();
        crate::cron::remove_job(&cfg, job_id)
            .map_err(|e| ClawgotchaError::Validation(format!("cron remove: {e}")))?;
        tracing::info!(%job_id, "clawgotcha: removed cron job");
        Ok(())
    }
}

/// Webhook batch reconcile + swarm defaults application during polling.
pub struct HostReconciler {
    pub config: Arc<Mutex<Config>>,
}

#[async_trait]
impl ConfigReconciler for HostReconciler {
    async fn apply_batch(&self, events: Vec<ChangeEvent>) -> Result<(), ClawgotchaError> {
        if events.is_empty() {
            return Ok(());
        }
        tracing::debug!(
            count = events.len(),
            "clawgotcha: reconcile webhook batch (polling also applies deltas)"
        );
        Ok(())
    }

    async fn apply_swarm_defaults(&self, defs: &SwarmDefaults) -> Result<(), ClawgotchaError> {
        let snapshot = {
            let mut cfg = self.config.lock();
            if let Some(ref p) = defs.default_provider {
                cfg.default_provider = Some(p.clone());
            }
            if let Some(ref m) = defs.default_model {
                cfg.default_model = Some(m.clone());
            }
            cfg.clone()
        };
        snapshot
            .save()
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("save config: {e}")))?;
        tracing::info!(
            revision = defs.current_revision,
            "clawgotcha: applied swarm defaults"
        );
        Ok(())
    }
}
