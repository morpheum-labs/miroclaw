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
        crate::cron::retire_job_for_clawgotcha_remove(&cfg, job_id)
            .map_err(|e| ClawgotchaError::Validation(format!("cron retire: {e}")))?;
        tracing::info!(%job_id, "clawgotcha: retired cron job (soft-remove)");
        Ok(())
    }

    async fn reconcile_jobs_present(
        &self,
        remote_job_ids: &[String],
    ) -> Result<(), ClawgotchaError> {
        let cfg = self.config.lock().clone();
        crate::cron::retire_clawgotcha_jobs_not_in_remote(&cfg, remote_job_ids)
            .map_err(|e| ClawgotchaError::Validation(format!("cron reconcile snapshot: {e}")))?;
        Ok(())
    }
}

/// Webhook batch reconcile + swarm defaults application during polling.
pub struct HostReconciler {
    pub config: Arc<Mutex<Config>>,
    pub agents: Arc<dyn AgentRuntimeUpdater>,
    pub cron: Arc<dyn CronSchedulerUpdater>,
}

#[async_trait]
impl ConfigReconciler for HostReconciler {
    async fn apply_batch(&self, events: Vec<ChangeEvent>) -> Result<(), ClawgotchaError> {
        if events.is_empty() {
            return Ok(());
        }
        tracing::debug!(
            count = events.len(),
            "clawgotcha: applying webhook event batch"
        );
        for ev in events {
            match ev {
                ChangeEvent::CronDeleted { job_id, .. } => {
                    self.cron.remove_job(&job_id).await?;
                }
                ChangeEvent::AgentDeleted { name, .. } => {
                    self.agents.remove_agent(&name).await?;
                }
                ChangeEvent::CronUpdated { job_id, revision } => {
                    tracing::debug!(
                        job_id = %job_id,
                        revision,
                        "clawgotcha webhook: cron updated (poll will upsert full row)"
                    );
                }
                ChangeEvent::AgentUpdated { name, revision } => {
                    tracing::debug!(
                        agent = %name,
                        revision,
                        "clawgotcha webhook: agent updated (poll will upsert full row)"
                    );
                }
                ChangeEvent::ConfigUpdated { revision } => {
                    tracing::debug!(
                        revision,
                        "clawgotcha webhook: config updated (poll will refresh swarm defaults)"
                    );
                }
                ChangeEvent::NotifySync { reason } => {
                    tracing::debug!(%reason, "clawgotcha webhook: notify sync");
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    #[tokio::test]
    async fn apply_batch_cron_deleted_retires_job() {
        let tmp = TempDir::new().unwrap();
        let cfg = Config {
            workspace_dir: tmp.path().join("workspace"),
            config_path: tmp.path().join("config.toml"),
            ..Config::default()
        };
        std::fs::create_dir_all(&cfg.workspace_dir).unwrap();

        crate::cron::upsert_clawgotcha_agent_job(&cfg, "w1", "*/5 * * * *", "prompt", true)
            .unwrap();

        let shared = Arc::new(Mutex::new(cfg.clone()));
        let agents: Arc<dyn AgentRuntimeUpdater> = Arc::new(HostAgents {
            config: Arc::clone(&shared),
            delegate_agents: None,
        });
        let cron: Arc<dyn CronSchedulerUpdater> = Arc::new(HostCron {
            config: Arc::clone(&shared),
        });
        let reconciler = HostReconciler {
            config: Arc::clone(&shared),
            agents,
            cron,
        };

        reconciler
            .apply_batch(vec![ChangeEvent::CronDeleted {
                job_id: "w1".into(),
                revision: 9,
            }])
            .await
            .unwrap();

        let j = crate::cron::get_job(&cfg, "w1").unwrap();
        assert!(j.retired_at.is_some());
        assert_eq!(j.retired_reason.as_deref(), Some("clawgotcha"));
    }
}
