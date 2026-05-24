//! Per-agent worker runtime: internal gateway + channels + scheduler for one profile.

use crate::config::Config;
use anyhow::Result;
use std::future::Future;
use std::path::Path;
use tokio::task::JoinHandle;

const STATUS_FLUSH_SECONDS: u64 = 5;

/// Load and run a worker from a profile directory (`config.toml` + `workspace/`).
pub async fn run_for_profile_dir(config_dir: &Path, internal_port: u16) -> Result<()> {
    std::env::set_var("MIROCLAW_CONFIG_DIR", config_dir);
    let mut config = Config::load_or_init().await?;
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = internal_port;
    config.gateway.allow_public_bind = false;
    run(config, internal_port).await
}

/// Run one agent profile worker (localhost gateway + channels + optional cron/heartbeat).
pub async fn run(mut config: Config, internal_port: u16) -> Result<()> {
    crate::tools::mcp_tool_context::init_tool_execution_context();

    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = internal_port;
    config.gateway.allow_public_bind = false;

    let initial_backoff = config.reliability.channel_initial_backoff_secs.max(1);
    let max_backoff = config
        .reliability
        .channel_max_backoff_secs
        .max(initial_backoff);

    crate::health::mark_component_ok("agent_worker");

    if config.heartbeat.enabled {
        let _ =
            crate::heartbeat::engine::HeartbeatEngine::ensure_heartbeat_file(&config.workspace_dir)
                .await;
    }

    let agent_name = config
        .config_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "agent".to_string());

    let mut handles: Vec<JoinHandle<()>> =
        vec![spawn_state_writer(config.clone(), agent_name.clone())];

    {
        let gw_cfg = config.clone();
        handles.push(spawn_component_supervisor(
            &format!("gateway-{agent_name}"),
            initial_backoff,
            max_backoff,
            true,
            move || {
                let cfg = gw_cfg.clone();
                async move {
                    Box::pin(crate::gateway::run_gateway(
                        "127.0.0.1",
                        internal_port,
                        cfg,
                        None,
                        None,
                        None,
                        None,
                    ))
                    .await
                }
            },
        ));
    }

    {
        if crate::daemon::has_supervised_channels(&config) {
            let channels_cfg = config.clone();
            handles.push(spawn_component_supervisor(
                &format!("channels-{agent_name}"),
                initial_backoff,
                max_backoff,
                true,
                move || {
                    let cfg = channels_cfg.clone();
                    async move { Box::pin(crate::channels::start_channels(cfg)).await }
                },
            ));
        }
    }

    if config.cron.enabled {
        let scheduler_cfg = config.clone();
        handles.push(spawn_component_supervisor(
            &format!("scheduler-{agent_name}"),
            initial_backoff,
            max_backoff,
            true,
            move || {
                let cfg = scheduler_cfg.clone();
                async move { Box::pin(crate::cron::scheduler::run(cfg)).await }
            },
        ));
    }

    tracing::info!(
        agent = %agent_name,
        port = internal_port,
        "Agent worker running (internal gateway)"
    );

    tokio::signal::ctrl_c().await.ok();
    for handle in handles {
        handle.abort();
    }
    Ok(())
}

fn spawn_state_writer(config: Config, agent_name: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        let state_path = config.workspace_dir.join("state").join(format!(
            "agent-worker-{agent_name}.json"
        ));
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(STATUS_FLUSH_SECONDS)).await;
            let payload = serde_json::json!({
                "agent": agent_name,
                "workspace": config.workspace_dir.display().to_string(),
                "ts": chrono::Utc::now().to_rfc3339(),
            });
            if let Some(parent) = state_path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = tokio::fs::write(&state_path, payload.to_string()).await;
        }
    })
}

fn spawn_component_supervisor<F, Fut>(
    name: &str,
    initial_backoff: u64,
    max_backoff: u64,
    restart: bool,
    mut factory: F,
) -> JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let name = name.to_string();
    tokio::spawn(async move {
        let mut backoff = initial_backoff;
        loop {
            crate::health::mark_component_ok(&name);
            let result = factory().await;
            if !restart {
                break;
            }
            if result.is_ok() {
                tracing::warn!(component = %name, "Component exited unexpectedly; restarting");
            } else if let Err(e) = result {
                tracing::error!(component = %name, error = %e, "Component failed; restarting");
                crate::health::mark_component_error(&name, &e.to_string());
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(max_backoff);
        }
    })
}
