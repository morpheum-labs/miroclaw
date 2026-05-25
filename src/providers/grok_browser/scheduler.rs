use super::retry::{run_grok_site_with_retry, GrokRetryResult, GrokSiteOp};
use crate::providers::bun_browser::BunBrowserClient;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock};
use tokio::sync::{oneshot, Mutex, Notify};
use uuid::Uuid;

static GLOBAL_JOB_REGISTRY: OnceLock<Arc<GrokJobRegistry>> = OnceLock::new();

pub fn global_job_registry() -> Option<Arc<GrokJobRegistry>> {
    GLOBAL_JOB_REGISTRY.get().cloned()
}

fn register_global_job_registry(registry: Arc<GrokJobRegistry>) {
    let _ = GLOBAL_JOB_REGISTRY.set(registry);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatusKind {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobStatusSnapshot {
    pub request_id: String,
    pub status: JobStatusKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_position: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
}

pub struct GrokJobRegistry {
    inner: Mutex<HashMap<String, JobEntry>>,
}

struct JobEntry {
    snapshot: JobStatusSnapshot,
    waiter: Option<oneshot::Sender<Result<GrokRetryResult, String>>>,
}

impl GrokJobRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub async fn poll(&self, request_id: &str) -> Option<JobStatusSnapshot> {
        let guard = self.inner.lock().await;
        guard.get(request_id).map(|entry| entry.snapshot.clone())
    }

    async fn register(
        &self,
        request_id: String,
        waiter: oneshot::Sender<Result<GrokRetryResult, String>>,
    ) {
        let mut guard = self.inner.lock().await;
        guard.insert(
            request_id.clone(),
            JobEntry {
                snapshot: JobStatusSnapshot {
                    request_id,
                    status: JobStatusKind::Queued,
                    queue_position: None,
                    tab: None,
                    error: None,
                    result_preview: None,
                },
                waiter: Some(waiter),
            },
        );
    }

    async fn set_queue_positions(&self, ordered_ids: &[String]) {
        let mut guard = self.inner.lock().await;
        for (idx, request_id) in ordered_ids.iter().enumerate() {
            if let Some(entry) = guard.get_mut(request_id) {
                if entry.snapshot.status == JobStatusKind::Queued {
                    entry.snapshot.queue_position = Some(idx + 1);
                }
            }
        }
    }

    async fn update<F>(&self, request_id: &str, update: F)
    where
        F: FnOnce(&mut JobStatusSnapshot),
    {
        let mut guard = self.inner.lock().await;
        if let Some(entry) = guard.get_mut(request_id) {
            update(&mut entry.snapshot);
        }
    }

    async fn finish(&self, request_id: &str, result: Result<GrokRetryResult, String>) {
        let mut guard = self.inner.lock().await;
        let Some(entry) = guard.get_mut(request_id) else {
            return;
        };
        match &result {
            Ok(ok) => {
                entry.snapshot.status = JobStatusKind::Completed;
                entry.snapshot.result_preview =
                    Some(ok.answer.chars().take(200).collect::<String>());
                entry.snapshot.tab = ok.tab_id.clone();
            }
            Err(err) => {
                entry.snapshot.status = JobStatusKind::Failed;
                entry.snapshot.error = Some(err.clone());
            }
        }
        entry.snapshot.queue_position = None;
        if let Some(tx) = entry.waiter.take() {
            let _ = tx.send(result);
        }
    }
}

struct QueuedJob {
    request_id: String,
    op: GrokSiteOp,
    disable_search: bool,
    pinned_tab: Option<String>,
}

struct SchedulerState {
    queue: VecDeque<QueuedJob>,
    running: usize,
}

pub struct GrokBrowserScheduler {
    registry: Arc<GrokJobRegistry>,
    client: Arc<Mutex<BunBrowserClient>>,
    max_parallel: usize,
    state: Mutex<SchedulerState>,
    notify: Notify,
    started: Mutex<bool>,
}

impl GrokBrowserScheduler {
    pub fn new(client: Arc<Mutex<BunBrowserClient>>, max_parallel: usize) -> Arc<Self> {
        let registry = Arc::new(GrokJobRegistry::new());
        register_global_job_registry(Arc::clone(&registry));
        let scheduler = Arc::new(Self {
            registry,
            client,
            max_parallel: max_parallel.max(1),
            state: Mutex::new(SchedulerState {
                queue: VecDeque::new(),
                running: 0,
            }),
            notify: Notify::new(),
            started: Mutex::new(false),
        });
        scheduler
    }

    async fn ensure_started(self: &Arc<Self>) {
        let mut started = self.started.lock().await;
        if !*started {
            let worker = Arc::clone(self);
            tokio::spawn(async move {
                worker.dispatch_loop().await;
            });
            *started = true;
        }
    }

    pub fn registry(&self) -> Arc<GrokJobRegistry> {
        Arc::clone(&self.registry)
    }

    pub async fn submit(
        self: &Arc<Self>,
        op: GrokSiteOp,
        disable_search: bool,
        pinned_tab: Option<String>,
    ) -> (String, oneshot::Receiver<Result<GrokRetryResult, String>>) {
        self.ensure_started().await;
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.registry.register(request_id.clone(), tx).await;

        {
            let mut state = self.state.lock().await;
            state.queue.push_back(QueuedJob {
                request_id: request_id.clone(),
                op,
                disable_search,
                pinned_tab,
            });
            let queued_ids: Vec<String> =
                state.queue.iter().map(|j| j.request_id.clone()).collect();
            drop(state);
            self.registry.set_queue_positions(&queued_ids).await;
        }

        tracing::info!(
            request_id = %request_id,
            "grok browser job queued"
        );
        self.notify.notify_one();
        (request_id, rx)
    }

    pub async fn wait(
        rx: oneshot::Receiver<Result<GrokRetryResult, String>>,
    ) -> anyhow::Result<GrokRetryResult> {
        match rx.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(err)) => anyhow::bail!(err),
            Err(_) => anyhow::bail!("grok browser job dropped before completion"),
        }
    }

    async fn dispatch_loop(self: Arc<Self>) {
        loop {
            let job = {
                let mut state = self.state.lock().await;
                if state.running >= self.max_parallel || state.queue.is_empty() {
                    None
                } else {
                    state.running += 1;
                    state.queue.pop_front()
                }
            };

            if let Some(job) = job {
                {
                    let state = self.state.lock().await;
                    let queued_ids: Vec<String> =
                        state.queue.iter().map(|j| j.request_id.clone()).collect();
                    drop(state);
                    self.registry.set_queue_positions(&queued_ids).await;
                }

                self.registry
                    .update(&job.request_id, |snapshot| {
                        snapshot.status = JobStatusKind::Running;
                        snapshot.queue_position = None;
                    })
                    .await;

                tracing::info!(
                    request_id = %job.request_id,
                    "grok browser job running"
                );

                let scheduler = Arc::clone(&self);
                tokio::spawn(async move {
                    let result = scheduler.run_job(job).await;
                    scheduler.registry.finish(&result.0, result.1).await;
                    {
                        let mut state = scheduler.state.lock().await;
                        state.running = state.running.saturating_sub(1);
                    }
                    scheduler.notify.notify_one();
                });
            } else {
                self.notify.notified().await;
            }
        }
    }

    async fn run_job(&self, job: QueuedJob) -> (String, Result<GrokRetryResult, String>) {
        let request_id = job.request_id.clone();
        let mut client = self.client.lock().await;
        let tab_result = client
            .ensure_grok_tab(job.pinned_tab.as_deref())
            .await
            .map_err(|e| e.to_string());

        let tab_id = match tab_result {
            Ok(tab) => {
                self.registry
                    .update(&request_id, |snapshot| {
                        snapshot.tab = Some(tab.clone());
                    })
                    .await;
                Some(tab)
            }
            Err(err) => return (request_id, Err(err)),
        };

        match run_grok_site_with_retry(&mut client, &job.op, job.disable_search, tab_id).await {
            Ok(result) => (request_id, Ok(result)),
            Err(err) => (request_id, Err(err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_poll_unknown_returns_none() {
        let registry = GrokJobRegistry::new();
        assert!(registry.poll("missing").await.is_none());
    }

    #[tokio::test]
    async fn queue_position_updates() {
        let registry = GrokJobRegistry::new();
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();
        registry.register("a".into(), tx1).await;
        registry.register("b".into(), tx2).await;
        registry
            .set_queue_positions(&["a".into(), "b".into()])
            .await;
        let first = registry.poll("a").await.unwrap();
        let second = registry.poll("b").await.unwrap();
        assert_eq!(first.queue_position, Some(1));
        assert_eq!(second.queue_position, Some(2));
    }

    #[tokio::test]
    async fn queue_positions_are_monotonic_for_nine_jobs() {
        let registry = GrokJobRegistry::new();
        let ids: Vec<String> = (0..9).map(|i| format!("job-{i}")).collect();
        for id in &ids {
            let (tx, _rx) = oneshot::channel();
            registry.register(id.clone(), tx).await;
        }
        registry.set_queue_positions(&ids).await;
        for (idx, id) in ids.iter().enumerate() {
            let snap = registry.poll(id).await.unwrap();
            assert_eq!(snap.queue_position, Some(idx + 1));
        }
    }

    #[tokio::test]
    async fn submit_assigns_request_id_immediately() {
        use super::super::retry::GrokSiteOp;
        use crate::providers::bun_browser::BunBrowserClient;

        let client = Arc::new(Mutex::new(
            BunBrowserClient::new_deferred(Some("http://127.0.0.1:1".into()), Some(1)).unwrap(),
        ));
        let scheduler = GrokBrowserScheduler::new(client, 2);
        let (request_id, rx) = scheduler
            .submit(
                GrokSiteOp::Simple {
                    adapter: "grok/modes".to_string(),
                    args: Default::default(),
                },
                true,
                None,
            )
            .await;
        assert!(!request_id.is_empty());
        let snapshot = scheduler.registry().poll(&request_id).await.unwrap();
        assert!(matches!(
            snapshot.status,
            JobStatusKind::Queued | JobStatusKind::Running
        ));
        let _ = rx.await;
    }
}
