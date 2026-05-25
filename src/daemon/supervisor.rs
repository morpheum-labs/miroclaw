//! Hub-mode supervisor: public gateway + per-profile agent workers.

use crate::config::registry::AgentRegistry;
use crate::config::schema::default_config_dir;
use crate::config::Config;
use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::task::JoinHandle;

use super::{spawn_component_supervisor, wait_for_shutdown_signal};

/// Run hub supervisor when `[hub].enabled = true`.
pub async fn run_hub_supervisor(hub_config: Config, host: String, port: u16) -> Result<()> {
    crate::tools::mcp_tool_context::init_tool_execution_context();

    let home_dir = hub_config
        .config_path
        .parent()
        .map(PathBuf::from)
        .or_else(|| default_config_dir().ok())
        .context("hub config directory")?;

    let registry = AgentRegistry::load_from(&home_dir).await?;
    if registry.agents.is_empty() {
        anyhow::bail!(
            "hub enabled but registry has no agents; run `miroclaw agents create main` first"
        );
    }

    let initial_backoff = hub_config.reliability.channel_initial_backoff_secs.max(1);
    let max_backoff = hub_config
        .reliability
        .channel_max_backoff_secs
        .max(initial_backoff);

    crate::health::mark_component_ok("daemon");
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for entry in registry.enabled_agents().collect::<Vec<_>>() {
        let config_dir = registry.resolve_config_dir(&home_dir, &entry.name)?;
        let agent_name = entry.name.clone();
        let internal_port = entry.internal_port;
        let component_name: &'static str =
            Box::leak(format!("agent-{}", agent_name.clone()).into_boxed_str());
        handles.push(spawn_component_supervisor(
            component_name,
            initial_backoff,
            max_backoff,
            true,
            move || {
                let dir = config_dir.clone();
                let name = agent_name.clone();
                async move {
                    tracing::info!(
                        agent = %name,
                        port = internal_port,
                        dir = %dir.display(),
                        "Starting agent worker"
                    );
                    Box::pin(crate::agent_worker::run_for_profile_dir(
                        &dir,
                        internal_port,
                    ))
                    .await
                }
            },
        ));
    }

    let hub_cfg = hub_config.clone();
    let reg = registry.clone();
    let home = home_dir.clone();
    let hub_host = host.clone();
    handles.push(spawn_component_supervisor(
        "hub-gateway",
        initial_backoff,
        max_backoff,
        true,
        move || {
            let cfg = hub_cfg.clone();
            let reg = reg.clone();
            let home = home.clone();
            let host = hub_host.clone();
            async move {
                Box::pin(crate::gateway::run_hub_gateway(
                    &host, port, cfg, reg, home,
                ))
                .await
            }
        },
    ));

    println!("🧠 Miroclaw hub supervisor started");
    println!("   Public gateway: http://{host}:{port}");
    println!(
        "   Agent workers: {}",
        registry
            .enabled_agents()
            .map(|a| format!("{}:{}", a.name, a.internal_port))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("   Ctrl+C or SIGTERM to stop");

    wait_for_shutdown_signal().await?;
    for handle in handles {
        handle.abort();
    }
    Ok(())
}
