//! Process-local handle for the daemon [`super::TaskRuntime`] (avoids passing a trait object
//! through the cron scheduler while the CLI binary and library share source paths).

use std::sync::Arc;

use anyhow::Context;
use parking_lot::Mutex;

use crate::config::Config;
use crate::cron::CronJob;
use crate::security::SecurityPolicy;

use super::TaskRuntime;

static DAEMON_TASK_RUNTIME: Mutex<Option<Arc<TaskRuntime>>> = Mutex::new(None);

/// Install the shared task runtime for this process (daemon startup).
pub fn init_daemon_task_runtime(rt: Arc<TaskRuntime>) {
    *DAEMON_TASK_RUNTIME.lock() = Some(rt);
}

#[must_use]
pub fn daemon_task_runtime() -> Option<Arc<TaskRuntime>> {
    DAEMON_TASK_RUNTIME.lock().clone()
}

/// When the daemon installed a runtime, deserialize JSON from the caller’s `Config` / `CronJob`
/// into this crate’s types and delegate to [`TaskRuntime::run_cron_job`].
///
/// Returns [`None`] if no runtime is installed. Returns [`Some(Err(..))`] if JSON bridging or
/// execution fails (caller should fall back to direct cron execution).
pub async fn try_run_cron_via_daemon_runtime(
    cfg_json: serde_json::Value,
    job_json: serde_json::Value,
    component: &str,
) -> Option<anyhow::Result<(String, bool, String)>> {
    let rt = daemon_task_runtime()?;
    let component = component.to_string();
    Some(
        (async move {
            let config: Config = serde_json::from_value(cfg_json).context("task bridge config")?;
            let job: CronJob = serde_json::from_value(job_json).context("task bridge job")?;
            let security = SecurityPolicy::from_config(&config.autonomy, &config.workspace_dir);
            Ok::<_, anyhow::Error>(rt.run_cron_job(&config, &security, &job, &component).await)
        })
        .await,
    )
}
