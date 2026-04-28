//! Orchestrates registration, heartbeat, delta fetch, and fan-in from webhooks.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::config::{ClawgotchaRuntimeConfig, SyncMode};
use crate::error::ClawgotchaError;
use crate::events::ChangeEvent;
use crate::models::domain::{OfflineSnapshot, RevisionSummary};
use crate::sync::strategy::HybridParts;
use crate::traits::{
    AgentRuntimeUpdater, ChangeEventSink, ClawgotchaClient, ConfigReconciler, CronSchedulerUpdater,
    FetchDelta, HeartbeatPayload, InstanceRegistration, OfflineCache, RevisionStore,
};

/// Long-lived sync orchestration (depends only on traits).
pub struct SyncService<R, O, C>
where
    R: RevisionStore,
    O: OfflineCache,
    C: ClawgotchaClient,
{
    pub client: Arc<C>,
    pub revisions: Arc<R>,
    pub offline: Arc<O>,
    pub reconciler: Arc<dyn ConfigReconciler>,
    pub agents: Arc<dyn AgentRuntimeUpdater>,
    pub cron: Arc<dyn CronSchedulerUpdater>,
    pub sink: Arc<dyn ChangeEventSink>,
    /// Host fills counts + instance id each heartbeat tick.
    heartbeat: Arc<dyn Fn() -> HeartbeatPayload + Send + Sync>,
    etag_agents: Arc<Mutex<Option<String>>>,
    etag_cron: Arc<Mutex<Option<String>>>,
    etag_swarm: Arc<Mutex<Option<String>>>,
}

impl<R, O, C> SyncService<R, O, C>
where
    R: RevisionStore + 'static,
    O: OfflineCache + 'static,
    C: ClawgotchaClient + 'static,
{
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        client: Arc<C>,
        revisions: Arc<R>,
        offline: Arc<O>,
        reconciler: Arc<dyn ConfigReconciler>,
        agents: Arc<dyn AgentRuntimeUpdater>,
        cron: Arc<dyn CronSchedulerUpdater>,
        sink: Arc<dyn ChangeEventSink>,
        heartbeat: Arc<dyn Fn() -> HeartbeatPayload + Send + Sync>,
    ) -> Self {
        Self {
            client,
            revisions,
            offline,
            reconciler,
            agents,
            cron,
            sink,
            heartbeat,
            etag_agents: Arc::new(Mutex::new(None)),
            etag_cron: Arc::new(Mutex::new(None)),
            etag_swarm: Arc::new(Mutex::new(None)),
        }
    }

    /// Initial registration + full pull + optional webhook registration.
    pub async fn bootstrap(
        &self,
        cfg: &ClawgotchaRuntimeConfig,
        instance_callback_url: Option<String>,
    ) -> Result<(), ClawgotchaError> {
        self.client
            .register_instance(&InstanceRegistration {
                instance: crate::models::domain::ClawgotchaInstance {
                    instance_name: cfg.instance_name.clone(),
                    callback_url: instance_callback_url,
                },
            })
            .await?;

        let mut summary = self.revisions.load().await?;
        self.pull_agents_delta(&mut summary, None).await?;
        self.pull_cron_delta(&mut summary, None).await?;
        self.pull_swarm(&mut summary).await?;

        self.persist_offline(&summary).await?;
        self.revisions.save(&summary).await?;

        if matches!(cfg.sync_mode, SyncMode::Webhook | SyncMode::Hybrid) {
            if let Some(ref base) = cfg.callback_public_base_url {
                let url = format!("{}/webhook/clawgotcha", base.trim_end_matches('/'));
                self.client
                    .register_webhook(&url, &["agent", "cron", "config"])
                    .await?;
            } else {
                tracing::warn!(
                    "clawgotcha sync_mode includes webhook but callback_public_base_url is unset; skipping remote webhook registration"
                );
            }
        }

        Ok(())
    }

    async fn persist_offline(&self, summary: &RevisionSummary) -> Result<(), ClawgotchaError> {
        let snap = OfflineSnapshot {
            revision: summary.clone(),
            agents: vec![],
            cron_jobs: vec![],
            swarm_defaults: None,
        };
        self.offline.save(&snap).await
    }

    async fn pull_agents_delta(
        &self,
        summary: &mut RevisionSummary,
        since: Option<u64>,
    ) -> Result<(), ClawgotchaError> {
        let tag = self.etag_agents.lock().await.clone();
        let (delta, etag) = self
            .client
            .fetch_agents_since(since, tag.as_deref())
            .await?;
        if let Some(e) = etag {
            *self.etag_agents.lock().await = Some(e);
        }
        match delta {
            FetchDelta::NotModified => Ok(()),
            FetchDelta::Modified(d) => {
                summary.global_max_revision = summary.global_max_revision.max(d.revision_watermark);
                for a in &d.agents {
                    summary
                        .agents_revision_at
                        .insert(a.name.clone(), a.current_revision);
                    self.agents.upsert_agent(a).await?;
                }
                Ok(())
            }
        }
    }

    async fn pull_cron_delta(
        &self,
        summary: &mut RevisionSummary,
        since: Option<u64>,
    ) -> Result<(), ClawgotchaError> {
        let tag = self.etag_cron.lock().await.clone();
        let (delta, etag) = self
            .client
            .fetch_cron_jobs_since(since, tag.as_deref())
            .await?;
        if let Some(e) = etag {
            *self.etag_cron.lock().await = Some(e);
        }
        match delta {
            FetchDelta::NotModified => Ok(()),
            FetchDelta::Modified(d) => {
                summary.global_max_revision = summary.global_max_revision.max(d.revision_watermark);
                for j in &d.jobs {
                    summary
                        .cron_revision_at
                        .insert(j.id.clone(), j.current_revision);
                    self.cron.upsert_job(j).await?;
                }
                Ok(())
            }
        }
    }

    async fn pull_swarm(&self, summary: &mut RevisionSummary) -> Result<(), ClawgotchaError> {
        let tag = self.etag_swarm.lock().await.clone();
        let (delta, etag) = self.client.fetch_swarm_config(tag.as_deref()).await?;
        if let Some(e) = etag {
            *self.etag_swarm.lock().await = Some(e);
        }
        match delta {
            FetchDelta::NotModified => Ok(()),
            FetchDelta::Modified(s) => {
                summary.global_max_revision = summary.global_max_revision.max(s.current_revision);
                summary.config_revision_at = Some(s.current_revision);
                self.reconciler.apply_swarm_defaults(&s).await?;
                Ok(())
            }
        }
    }

    /// Poll + webhook fan-in loop (daemon supervisor restarts on transport failure).
    pub async fn run_periodic(
        &self,
        cfg: ClawgotchaRuntimeConfig,
        mut webhook_rx: tokio::sync::mpsc::Receiver<ChangeEvent>,
    ) -> Result<(), ClawgotchaError> {
        let parts = HybridParts::from_mode(cfg.sync_mode);
        let poll_interval = Duration::from_secs(cfg.poll_interval_secs.max(5));
        let hb_interval = Duration::from_secs(cfg.heartbeat_interval_secs.max(10));

        let mut poll_tick = tokio::time::interval(poll_interval);
        poll_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut hb_tick = tokio::time::interval(hb_interval);
        hb_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                maybe = webhook_rx.recv() => {
                    if let Some(ev) = maybe {
                        self.on_webhook_event(ev).await?;
                    }
                }
                _ = poll_tick.tick(), if parts.poll => {
                    let mut summary = self.revisions.load().await?;
                    let global = Some(summary.global_max_revision);
                    self.pull_agents_delta(&mut summary, global).await?;
                    self.pull_cron_delta(&mut summary, global).await?;
                    self.pull_swarm(&mut summary).await?;
                    self.revisions.save(&summary).await?;
                }
                _ = hb_tick.tick() => {
                    let hb = (self.heartbeat)();
                    if let Err(e) = self.client.send_heartbeat(&hb).await {
                        tracing::warn!(error = %e, "clawgotcha heartbeat failed");
                    }
                }
            }
        }
    }

    async fn on_webhook_event(&self, ev: ChangeEvent) -> Result<(), ClawgotchaError> {
        self.sink.push(ev.clone())?;
        self.reconciler.apply_batch(vec![ev]).await
    }
}
