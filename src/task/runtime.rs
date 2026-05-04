//! Tokio-native task supervisor: lifecycle, optional kill, and SSE fan-out.

use std::sync::Arc;

use crate::config::Config;
use crate::cron::{scheduler::execute_cron_core, CronJob};
use crate::security::SecurityPolicy;

use super::guardrail::{DefaultTaskGuardrail, TaskGuardrail};
use super::memory_hook;
use super::store;
use super::types::{TaskKind, TaskRecord, TaskStatus};

/// Shared task runtime (daemon-wide). Clone is shallow (`Arc` inner).
#[derive(Clone)]
pub struct TaskRuntime {
    inner: Arc<TaskRuntimeInner>,
}

struct TaskRuntimeInner {
    guardrail: Arc<dyn TaskGuardrail>,
    events: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
}

impl TaskRuntime {
    /// Build runtime with optional broadcast channel for `/api/tasks/stream` and dashboards.
    #[must_use]
    pub fn new(
        guardrail: Arc<dyn TaskGuardrail>,
        events: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
    ) -> Self {
        Self {
            inner: Arc::new(TaskRuntimeInner { guardrail, events }),
        }
    }

    /// Default production gate (security policy only).
    #[must_use]
    pub fn with_default_guardrail(
        events: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
    ) -> Self {
        Self::new(Arc::new(DefaultTaskGuardrail), events)
    }

    fn emit_event(&self, record: &TaskRecord) {
        let Some(tx) = &self.inner.events else {
            return;
        };
        let payload = serde_json::json!({
            "kind": "task_updated",
            "task": record,
        });
        let _ = tx.send(payload);
    }

    /// Run a single cron job with optional task recording + guardrail gate.
    pub async fn run_cron_job(
        &self,
        config: &Config,
        security: &SecurityPolicy,
        job: &CronJob,
        component: &str,
    ) -> (String, bool, String) {
        if !config.tasks.enabled {
            return execute_cron_core(config, security, job, component).await;
        }

        if let Err(gerr) = self
            .inner
            .guardrail
            .evaluate_cron_job(security, config, job)
        {
            let msg = gerr.to_string();
            return (job.id.clone(), false, msg);
        }

        if !config.tasks.record_cron_runs {
            return execute_cron_core(config, security, job, component).await;
        }

        let task_id = store::new_task_id();
        let title = format!("cron {:?}", job.job_type);
        let description = match job.job_type {
            crate::cron::JobType::Shell => job.command.clone(),
            crate::cron::JobType::Agent => job.prompt.clone().unwrap_or_default(),
            crate::cron::JobType::Hand => format!("hand: {}", job.command.trim()),
        };

        if let Err(e) = store::insert_pending(
            config,
            &task_id,
            &TaskKind::CronJobRun,
            None,
            &title,
            &description,
            Some(&job.id),
            None,
        ) {
            tracing::warn!(error = %e, "task store insert failed; running cron without task row");
            return execute_cron_core(config, security, job, component).await;
        }

        let out_dir = config
            .workspace_dir
            .join("tasks")
            .join("artifacts")
            .join(&task_id);
        if let Err(e) = tokio::fs::create_dir_all(&out_dir).await {
            tracing::warn!(error = %e, "task artifact dir failed");
        }
        let out_dir_str = out_dir.to_string_lossy().to_string();
        if let Err(e) = store::mark_running(config, &task_id, Some(out_dir_str.as_str())) {
            tracing::warn!(error = %e, "task mark_running failed");
        }
        if let Ok(Some(rec)) = store::get(config, &task_id) {
            self.emit_event(&rec);
        }

        let (job_id, success, output) = execute_cron_core(config, security, job, component).await;

        let status = if success {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed
        };
        let err = if success { None } else { Some(output.as_str()) };
        if let Err(e) = store::mark_finished(config, &task_id, status, err) {
            tracing::warn!(error = %e, "task mark_finished failed");
        }
        if let Ok(Some(rec)) = store::get(config, &task_id) {
            self.emit_event(&rec);
            memory_hook::on_task_terminal(config, &rec).await;
        }

        let summary_path = out_dir.join("summary.txt");
        let body = format!("job_id={job_id}\nsuccess={success}\n\n{output}");
        if let Err(e) = tokio::fs::write(&summary_path, body).await {
            tracing::warn!(error = %e, "task summary write failed");
        }

        (job_id, success, output)
    }

    /// List recent tasks (newest first).
    pub fn list_tasks(&self, config: &Config, limit: u32) -> anyhow::Result<Vec<TaskRecord>> {
        store::list_recent(config, limit)
    }

    /// Fetch one task by id.
    pub fn get_task(&self, config: &Config, id: &str) -> anyhow::Result<Option<TaskRecord>> {
        store::get(config, id)
    }

    /// Mark a task killed in the store (cron bodies are not cooperatively aborted yet).
    pub async fn kill_task(&self, config: &Config, id: &str) -> anyhow::Result<()> {
        if let Ok(Some(rec)) = store::get(config, id) {
            if matches!(rec.status.as_str(), "completed" | "failed" | "killed") {
                return Ok(());
            }
        }
        store::mark_finished(config, id, TaskStatus::Killed, Some("kill requested"))?;
        if let Ok(Some(rec)) = store::get(config, id) {
            self.emit_event(&rec);
            memory_hook::on_task_terminal(config, &rec).await;
        }
        Ok(())
    }

    /// Register a parent row when an SOP run starts (no-op if tasks disabled).
    pub fn on_sop_run_started(
        &self,
        config: &Config,
        sop_run_id: &str,
        sop_name: &str,
    ) -> anyhow::Result<Option<String>> {
        if !config.tasks.enabled {
            return Ok(None);
        }
        let task_id = store::new_task_id();
        store::insert_pending(
            config,
            &task_id,
            &TaskKind::SopChain,
            None,
            &format!("SOP {sop_name}"),
            &format!("run_id={sop_run_id}"),
            None,
            Some(sop_run_id),
        )?;
        if let Ok(Some(rec)) = store::get(config, &task_id) {
            self.emit_event(&rec);
        }
        Ok(Some(task_id))
    }

    /// Record a completed SOP step as a child task row.
    pub fn on_sop_step(
        &self,
        config: &Config,
        parent_task_id: Option<&str>,
        sop_run_id: &str,
        step_number: u32,
        summary: &str,
    ) -> anyhow::Result<()> {
        if !config.tasks.enabled {
            return Ok(());
        }
        let parent = if let Some(p) = parent_task_id {
            Some(p.to_string())
        } else {
            store::find_task_id_by_sop_run(config, sop_run_id)?
        };
        let Some(parent) = parent else {
            return Ok(());
        };
        let tid = store::new_task_id();
        store::insert_pending(
            config,
            &tid,
            &TaskKind::SopStep,
            Some(&parent),
            &format!("SOP step {step_number}"),
            summary,
            None,
            Some(sop_run_id),
        )?;
        store::mark_running(config, &tid, None)?;
        store::mark_finished(config, &tid, TaskStatus::Completed, None)?;
        if let Ok(Some(rec)) = store::get(config, &tid) {
            self.emit_event(&rec);
        }
        Ok(())
    }

    /// Subscribe to task lifecycle events (same payload shape as SSE stream).
    #[must_use]
    pub fn subscribe_events(&self) -> Option<tokio::sync::broadcast::Receiver<serde_json::Value>> {
        self.inner.events.as_ref().map(|tx| tx.subscribe())
    }
}
