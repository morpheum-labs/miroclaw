//! Optional hooks when tasks complete (reflection / living strategy file).
//!
//! When `[tasks].reflection_log_path` is set, each **terminal** task transition appends one
//! JSON line for downstream indexing or human review. This stays out of the memory crate to
//! avoid circular dependencies; callers may extend this to push into `Memory` in a later phase.

use std::path::Path;

use anyhow::Context;
use tokio::io::AsyncWriteExt;

use crate::config::Config;

use super::types::TaskRecord;

fn terminal_status(s: &str) -> bool {
    matches!(s, "completed" | "failed" | "killed")
}

/// Append a JSON line to the configured reflection log, if any.
pub async fn on_task_terminal(config: &Config, record: &TaskRecord) {
    if !terminal_status(&record.status) {
        return;
    }
    let Some(ref rel) = config.tasks.reflection_log_path else {
        return;
    };
    let path = if rel.is_absolute() {
        rel.clone()
    } else {
        config.workspace_dir.join(rel)
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            tracing::warn!(error = %e, "task reflection: failed to create parent dir");
            return;
        }
    }
    let line = match serde_json::to_string(record) {
        Ok(s) => format!("{s}\n"),
        Err(e) => {
            tracing::warn!(error = %e, "task reflection: serialize failed");
            return;
        }
    };
    if let Err(e) = append_line(&path, &line).await {
        tracing::warn!(path = %path.display(), error = %e, "task reflection: append failed");
    }
}

async fn append_line(path: &Path, line: &str) -> anyhow::Result<()> {
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("open reflection log {}", path.display()))?;
    f.write_all(line.as_bytes()).await?;
    f.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    #[tokio::test]
    async fn reflection_log_appends_jsonl() {
        let tmp = TempDir::new().unwrap();
        let mut config = Config {
            workspace_dir: tmp.path().join("ws"),
            config_path: tmp.path().join("config.toml"),
            ..Config::default()
        };
        config.tasks.reflection_log_path = Some("reflection/tasks.jsonl".into());
        tokio::fs::create_dir_all(config.workspace_dir.join("reflection"))
            .await
            .unwrap();

        let record = TaskRecord {
            id: "t1".into(),
            kind: "cron_job".into(),
            status: "completed".into(),
            parent_id: None,
            title: "cron".into(),
            description: "test".into(),
            cron_job_id: Some("cj1".into()),
            sop_run_id: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            started_at: None,
            finished_at: None,
            output_dir: None,
            error_message: None,
        };
        on_task_terminal(&config, &record).await;
        let path = config.workspace_dir.join("reflection/tasks.jsonl");
        let body = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(body.contains("t1"));
        assert!(body.contains("cron_job"));
    }
}
