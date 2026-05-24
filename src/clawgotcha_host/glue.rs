//! Clawgotcha host adapters: persist agent profiles in registry + cron rows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use clawgotcha::models::domain::{AgentDefinition, CronJobDefinition, SwarmDefaults};
use clawgotcha::traits::{AgentRuntimeUpdater, ConfigReconciler, CronSchedulerUpdater};
use clawgotcha::{ChangeEvent, ClawgotchaError};
use parking_lot::{Mutex, RwLock};

use crate::config::registry::AgentRegistry;
use crate::config::{Config, DelegateAgentConfig};

async fn write_profile_config(profile_dir: &std::path::Path, agent: &DelegateAgentConfig) -> Result<(), ClawgotchaError> {
    let mut cfg = Config::default();
    cfg.default_provider = Some(agent.provider.clone());
    cfg.default_model = Some(agent.model.clone());
    cfg.api_key = agent.api_key.clone();
    cfg.default_temperature = agent.temperature.unwrap_or(cfg.default_temperature);
    let mut stripped = cfg;
    stripped.workspace_dir = PathBuf::new();
    stripped.config_path = PathBuf::new();
    let toml_str = toml::to_string_pretty(&stripped)
        .map_err(|e| ClawgotchaError::Validation(format!("serialize profile: {e}")))?;
    tokio::fs::create_dir_all(profile_dir.join("workspace"))
        .await
        .map_err(|e| ClawgotchaError::Validation(format!("create workspace: {e}")))?;
    tokio::fs::write(profile_dir.join("config.toml"), toml_str)
        .await
        .map_err(|e| ClawgotchaError::Validation(format!("write profile config: {e}")))?;
    Ok(())
}

/// Updates agent profiles in registry and keeps the delegate tool snapshot in sync when provided.
pub struct HostAgents {
    pub config: Arc<Mutex<Config>>,
    pub delegate_agents: Option<Arc<RwLock<HashMap<String, DelegateAgentConfig>>>>,
}

#[async_trait]
impl AgentRuntimeUpdater for HostAgents {
    async fn upsert_agent(&self, def: &AgentDefinition) -> Result<(), ClawgotchaError> {
        let agent_cfg = DelegateAgentConfig::from(def);
        let home_dir = {
            let cfg = self.config.lock();
            cfg.config_path
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| ClawgotchaError::Validation("config path has no parent".into()))?
        };
        let mut registry = AgentRegistry::load_from(&home_dir)
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("load registry: {e}")))?;
        let profile_dir = registry.profile_config_dir(&home_dir, &def.name);
        write_profile_config(&profile_dir, &agent_cfg).await?;
        if registry.get(&def.name).is_none() {
            registry.agents.push(crate::config::registry::AgentRegistryEntry {
                name: def.name.clone(),
                config_dir: format!("{}/{}", registry.profiles_dir, def.name),
                enabled: true,
                internal_port: registry.next_internal_port(),
            });
        }
        registry
            .save_to(&home_dir)
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("save registry: {e}")))?;
        if let Some(cell) = &self.delegate_agents {
            let mut map = cell.write();
            map.insert(def.name.clone(), agent_cfg);
        }
        crate::tools::mcp_vault::invalidate_mcp_scoped_state();
        tracing::info!(agent = %def.name, revision = def.current_revision, "clawgotcha: upserted agent profile");
        Ok(())
    }

    async fn remove_agent(&self, name: &str) -> Result<(), ClawgotchaError> {
        let home_dir = {
            let cfg = self.config.lock();
            cfg.config_path
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| ClawgotchaError::Validation("config path has no parent".into()))?
        };
        let mut registry = AgentRegistry::load_from(&home_dir)
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("load registry: {e}")))?;
        registry.agents.retain(|a| a.name != name);
        registry
            .save_to(&home_dir)
            .await
            .map_err(|e| ClawgotchaError::Validation(format!("save registry: {e}")))?;
        if let Some(cell) = &self.delegate_agents {
            cell.write().remove(name);
        }
        crate::tools::mcp_vault::invalidate_mcp_scoped_state();
        tracing::info!(%name, "clawgotcha: removed agent profile from registry");
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
