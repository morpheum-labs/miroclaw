//! SQLite persistence for task rows (`<workspace>/tasks/tasks.db`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::config::Config;

use super::types::{TaskId, TaskKind, TaskRecord, TaskStatus};

fn db_path(config: &Config) -> PathBuf {
    config.workspace_dir.join("tasks").join("tasks.db")
}

fn open(config: &Config) -> Result<Connection> {
    let path = db_path(config);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("create tasks dir {}", dir.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("open tasks database {}", path.display()))?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r"
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            parent_id TEXT,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            cron_job_id TEXT,
            sop_run_id TEXT,
            created_at TEXT NOT NULL,
            started_at TEXT,
            finished_at TEXT,
            output_dir TEXT,
            error_message TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_tasks_cron_job ON tasks(cron_job_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_sop_run ON tasks(sop_run_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
        ",
    )?;
    Ok(())
}

/// Ensure database file exists (for `miroclaw migrate tasks`).
pub fn ensure_db(config: &Config) -> Result<()> {
    let _ = open(config)?;
    Ok(())
}

pub fn insert_pending(
    config: &Config,
    id: &str,
    kind: &TaskKind,
    parent_id: Option<&str>,
    title: &str,
    description: &str,
    cron_job_id: Option<&str>,
    sop_run_id: Option<&str>,
) -> Result<()> {
    let conn = open(config)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (id, kind, status, parent_id, title, description, cron_job_id, sop_run_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            kind.as_db_str(),
            TaskStatus::Pending.as_str(),
            parent_id,
            title,
            description,
            cron_job_id,
            sop_run_id,
            now,
        ],
    )
    .context("insert pending task")?;
    Ok(())
}

pub fn mark_running(config: &Config, id: &str, output_dir: Option<&str>) -> Result<()> {
    let conn = open(config)?;
    let started = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE tasks SET status = ?1, started_at = ?2, output_dir = ?3 WHERE id = ?4",
        params![TaskStatus::Running.as_str(), started, output_dir, id],
    )
    .context("mark task running")?;
    Ok(())
}

pub fn mark_finished(
    config: &Config,
    id: &str,
    status: TaskStatus,
    error_message: Option<&str>,
) -> Result<()> {
    let conn = open(config)?;
    let finished = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE tasks SET status = ?1, finished_at = ?2, error_message = ?3 WHERE id = ?4",
        params![status.as_str(), finished, error_message, id],
    )
    .context("mark task finished")?;
    Ok(())
}

pub fn list_recent(config: &Config, limit: u32) -> Result<Vec<TaskRecord>> {
    let conn = open(config)?;
    let limit = i64::from(limit.max(1).min(500));
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, status, parent_id, title, description, cron_job_id, sop_run_id,
                    created_at, started_at, finished_at, output_dir, error_message
             FROM tasks ORDER BY datetime(created_at) DESC LIMIT ?1",
        )
        .context("prepare list tasks")?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                kind: row.get(1)?,
                status: row.get(2)?,
                parent_id: row.get(3)?,
                title: row.get(4)?,
                description: row.get(5)?,
                cron_job_id: row.get(6)?,
                sop_run_id: row.get(7)?,
                created_at: row.get(8)?,
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
                output_dir: row.get(11)?,
                error_message: row.get(12)?,
            })
        })
        .context("query tasks")?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get(config: &Config, id: &str) -> Result<Option<TaskRecord>> {
    let conn = open(config)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, status, parent_id, title, description, cron_job_id, sop_run_id,
                    created_at, started_at, finished_at, output_dir, error_message
             FROM tasks WHERE id = ?1",
        )
        .context("prepare get task")?;

    let mut rows = stmt
        .query_map(params![id], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                kind: row.get(1)?,
                status: row.get(2)?,
                parent_id: row.get(3)?,
                title: row.get(4)?,
                description: row.get(5)?,
                cron_job_id: row.get(6)?,
                sop_run_id: row.get(7)?,
                created_at: row.get(8)?,
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
                output_dir: row.get(11)?,
                error_message: row.get(12)?,
            })
        })
        .context("query task")?;

    match rows.next() {
        None => Ok(None),
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(e.into()),
    }
}

pub fn find_task_id_by_sop_run(config: &Config, sop_run_id: &str) -> Result<Option<TaskId>> {
    let conn = open(config)?;
    let mut stmt = conn.prepare(
        "SELECT id FROM tasks WHERE sop_run_id = ?1 AND kind = 'sop_chain' ORDER BY datetime(created_at) DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![sop_run_id], |row| row.get::<_, String>(0))?;
    match rows.next() {
        None => Ok(None),
        Some(Ok(id)) => Ok(Some(id)),
        Some(Err(e)) => Err(e.into()),
    }
}

#[must_use]
pub fn new_task_id() -> TaskId {
    Uuid::new_v4().to_string()
}

/// Backfill hook: stamp `tasks` DB schema only (no data rewrite).
pub fn migrate_placeholder(config: &Config) -> Result<String> {
    ensure_db(config)?;
    let path = db_path(config);
    Ok(format!(
        "tasks store ready at {} (schema v1, no cron/SOP rows modified)",
        path.display()
    ))
}
