//! Ports (dependency inversion): HTTP client, sinks, reconciler hooks.

use async_trait::async_trait;

use crate::error::ClawgotchaError;
use crate::events::ChangeEvent;
use crate::models::domain::{
    AgentDefinition, ClawgotchaInstance, CronJobDefinition, OfflineSnapshot, RevisionSummary,
    SwarmDefaults,
};

/// Payload for periodic heartbeat (counts filled by host).
#[derive(Debug, Clone, Default)]
pub struct HeartbeatPayload {
    /// Same logical id as registration (`[clawgotcha].instance_name` on Miroclaw).
    pub instance_name: String,
    pub loaded_agents_count: usize,
    pub cron_jobs_count: usize,
}

/// Registration metadata for this runtime instance.
#[derive(Debug, Clone)]
pub struct InstanceRegistration {
    pub instance: ClawgotchaInstance,
}

/// Result of a conditional GET for deltas.
#[derive(Debug, Clone)]
pub enum FetchDelta<T> {
    NotModified,
    Modified(T),
}

#[derive(Debug, Clone, Default)]
pub struct AgentsDelta {
    pub revision_watermark: u64,
    pub agents: Vec<AgentDefinition>,
}

#[derive(Debug, Clone, Default)]
pub struct CronDelta {
    pub revision_watermark: u64,
    pub jobs: Vec<CronJobDefinition>,
}

/// Segregated registration + heartbeat API.
#[async_trait]
pub trait ClawgotchaRegistration: Send + Sync {
    async fn register_instance(&self, reg: &InstanceRegistration) -> Result<(), ClawgotchaError>;
    async fn send_heartbeat(&self, hb: &HeartbeatPayload) -> Result<(), ClawgotchaError>;
}

/// Segregated read API for delta sync.
#[async_trait]
pub trait ClawgotchaSyncRead: Send + Sync {
    async fn fetch_agents_since(
        &self,
        revision: Option<u64>,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<AgentsDelta>, Option<String>), ClawgotchaError>;

    async fn fetch_cron_jobs_since(
        &self,
        revision: Option<u64>,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<CronDelta>, Option<String>), ClawgotchaError>;

    async fn fetch_swarm_config(
        &self,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<SwarmDefaults>, Option<String>), ClawgotchaError>;
}

/// Webhook registration at Clawgotcha (callback URL).
#[async_trait]
pub trait ClawgotchaWebhooks: Send + Sync {
    async fn register_webhook(
        &self,
        callback_url: &str,
        event_types: &[&str],
    ) -> Result<(), ClawgotchaError>;
}

/// Composite client used by `SyncService`.
#[async_trait]
pub trait ClawgotchaClient:
    ClawgotchaRegistration + ClawgotchaSyncRead + ClawgotchaWebhooks
{
}

impl<T> ClawgotchaClient for T where
    T: ClawgotchaRegistration + ClawgotchaSyncRead + ClawgotchaWebhooks
{
}

/// Bounded sink for `ChangeEvent` (implemented by host queue).
pub trait ChangeEventSink: Send + Sync {
    fn push(&self, event: ChangeEvent) -> Result<(), ClawgotchaError>;
}

/// Fan-in queue backed by a bounded Tokio MPSC channel.
pub struct MpscEventSink(pub tokio::sync::mpsc::Sender<ChangeEvent>);

impl ChangeEventSink for MpscEventSink {
    fn push(&self, event: ChangeEvent) -> Result<(), ClawgotchaError> {
        self.0.try_send(event).map_err(|_| {
            ClawgotchaError::Validation("clawgotcha change-event queue closed or full".to_string())
        })
    }
}

/// Accepts events when no secondary fan-out is needed (polling applies directly).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpChangeSink;

impl ChangeEventSink for NoOpChangeSink {
    fn push(&self, _: ChangeEvent) -> Result<(), ClawgotchaError> {
        Ok(())
    }
}

/// Applies remote changes to host structures (implemented in `zeroclaw`).
#[async_trait]
pub trait ConfigReconciler: Send + Sync {
    async fn apply_batch(&self, events: Vec<ChangeEvent>) -> Result<(), ClawgotchaError>;

    /// Merge swarm-level defaults after a successful `GET /v1/swarm/config` pull.
    async fn apply_swarm_defaults(
        &self,
        defs: &crate::models::domain::SwarmDefaults,
    ) -> Result<(), ClawgotchaError>;
}

/// Host hook: update delegate agents map / gateway-visible snapshot.
#[async_trait]
pub trait AgentRuntimeUpdater: Send + Sync {
    async fn upsert_agent(&self, def: &AgentDefinition) -> Result<(), ClawgotchaError>;
    async fn remove_agent(&self, name: &str) -> Result<(), ClawgotchaError>;
}

/// Host hook: upsert/delete cron rows via the same paths as the gateway API.
#[async_trait]
pub trait CronSchedulerUpdater: Send + Sync {
    async fn upsert_job(&self, job: &CronJobDefinition) -> Result<(), ClawgotchaError>;
    async fn remove_job(&self, job_id: &str) -> Result<(), ClawgotchaError>;
}

/// Persists last-known revision watermarks (workspace-local).
#[async_trait]
pub trait RevisionStore: Send + Sync {
    async fn load(&self) -> Result<RevisionSummary, ClawgotchaError>;
    async fn save(&self, summary: &RevisionSummary) -> Result<(), ClawgotchaError>;
}

/// Last-good snapshot when remote is unavailable.
#[async_trait]
pub trait OfflineCache: Send + Sync {
    async fn load(&self) -> Result<Option<OfflineSnapshot>, ClawgotchaError>;
    async fn save(&self, snapshot: &OfflineSnapshot) -> Result<(), ClawgotchaError>;
}
