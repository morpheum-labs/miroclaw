//! First-class task runtime: lifecycle, SQLite store, guardrail gate, and optional reflection log.
//!
//! Cron scheduler integration records each job execution when `[tasks]` is enabled.
//! See the repository plan for phased rollout (SOP chains, gateway SSE, LP guardrails).

mod guardrail;
mod memory_hook;
mod runtime;
mod store;
pub mod types;

pub use guardrail::{DefaultTaskGuardrail, GuardrailError, TaskGuardrail};
pub use runtime::TaskRuntime;
pub use store::{ensure_db, migrate_placeholder};

use anyhow::Result;

use crate::config::Config;

/// `miroclaw migrate tasks` — ensures schema only; does not rewrite cron or SOP definitions.
pub fn migrate_tasks(config: &Config) -> Result<String> {
    migrate_placeholder(config)
}

/// Handle `miroclaw task …` subcommands (CLI).
pub async fn handle_command(command: crate::TaskCommands, config: &Config) -> Result<()> {
    let rt = TaskRuntime::with_default_guardrail(None);
    match command {
        crate::TaskCommands::List { limit } => {
            let rows = rt.list_tasks(config, limit)?;
            if rows.is_empty() {
                println!("No tasks recorded yet.");
                return Ok(());
            }
            println!("Tasks ({}):", rows.len());
            for t in rows {
                println!("- {} | {} | {} | {}", t.id, t.kind, t.status, t.title);
                if let Some(ref cj) = t.cron_job_id {
                    println!("    cron_job_id: {cj}");
                }
                if let Some(ref sr) = t.sop_run_id {
                    println!("    sop_run_id: {sr}");
                }
            }
            Ok(())
        }
        crate::TaskCommands::Show { id } => {
            let row = rt
                .get_task(config, &id)?
                .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?;
            println!("{}", serde_json::to_string_pretty(&row)?);
            Ok(())
        }
        crate::TaskCommands::Kill { id } => {
            rt.kill_task(config, &id).await?;
            println!("Marked task {id} as killed (if it existed and was not already terminal).");
            Ok(())
        }
    }
}
