//! Clawgotcha integration glue (trait stubs, mapping, supervised sync task).

mod glue;
pub mod mapping;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use parking_lot::{Mutex, RwLock};

use crate::config::{Config, DelegateAgentConfig};

/// Build [`clawgotcha::config::ClawgotchaRuntimeConfig`] from loaded host config.
pub fn runtime_config_from_host(
    config: &Config,
) -> anyhow::Result<clawgotcha::config::ClawgotchaRuntimeConfig> {
    let mode = config.clawgotcha.sync_mode.trim().to_ascii_lowercase();
    let sync_mode = match mode.as_str() {
        "webhook" => clawgotcha::config::SyncMode::Webhook,
        "hybrid" => clawgotcha::config::SyncMode::Hybrid,
        _ => clawgotcha::config::SyncMode::Poll,
    };
    let url = config.clawgotcha.url.as_deref().unwrap_or("").trim();
    if url.is_empty() {
        anyhow::bail!("clawgotcha.url is empty");
    }
    let instance_name = config
        .clawgotcha
        .instance_name
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if instance_name.is_empty() {
        anyhow::bail!("clawgotcha.instance_name is empty");
    }

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(clawgotcha::config::ClawgotchaRuntimeConfig {
        base_url: url.to_string(),
        instance_name,
        hostname,
        version: env!("CARGO_PKG_VERSION").to_string(),
        sync_mode,
        heartbeat_interval_secs: config.clawgotcha.heartbeat_interval_seconds,
        poll_interval_secs: config.clawgotcha.poll_interval_seconds,
        callback_public_base_url: config.clawgotcha.callback_public_base_url.clone(),
        webhook_hmac_secret: config.clawgotcha.webhook_hmac_secret.clone(),
    })
}

/// Registration + heartbeat + periodic delta sync + webhook fan-in (daemon supervisor restarts on failure).
#[allow(clippy::implicit_hasher)]
pub async fn run_sync_supervised(
    config: Config,
    webhook_rx: tokio::sync::mpsc::Receiver<clawgotcha::ChangeEvent>,
    shared_config: Arc<Mutex<Config>>,
    delegate_agents: Option<Arc<RwLock<HashMap<String, DelegateAgentConfig>>>>,
) -> anyhow::Result<()> {
    let rt = runtime_config_from_host(&config).context("clawgotcha runtime config")?;
    let client = Arc::new(
        clawgotcha::client::ClawgotchaHttpAdapter::new(&rt).context("clawgotcha HTTP client")?,
    );

    let dir = config.workspace_dir.join("clawgotcha");
    let revisions = Arc::new(clawgotcha::sync::FileRevisionStore::new(
        dir.join("revisions.json"),
    ));
    let offline = Arc::new(clawgotcha::sync::FileOfflineCache::new(
        dir.join("offline.json"),
    ));

    let reconciler = Arc::new(glue::HostReconciler {
        config: Arc::clone(&shared_config),
    });
    let agents = Arc::new(glue::HostAgents {
        config: Arc::clone(&shared_config),
        delegate_agents: delegate_agents.clone(),
    });
    let cron = Arc::new(glue::HostCron {
        config: Arc::clone(&shared_config),
    });
    let sink = Arc::new(clawgotcha::NoOpChangeSink);

    let instance_name = rt.instance_name.clone();
    let hb_cfg = Arc::clone(&shared_config);
    let heartbeat: Arc<dyn Fn() -> clawgotcha::traits::HeartbeatPayload + Send + Sync> =
        Arc::new(move || {
            let cfg = hb_cfg.lock();
            let cron_jobs_count = crate::cron::list_jobs(&cfg).map(|v| v.len()).unwrap_or(0);
            clawgotcha::traits::HeartbeatPayload {
                instance_name: instance_name.clone(),
                loaded_agents_count: cfg.agents.len(),
                cron_jobs_count,
            }
        });

    let sync = clawgotcha::sync::SyncService::new(
        client, revisions, offline, reconciler, agents, cron, sink, heartbeat,
    );

    let callback = config
        .clawgotcha
        .callback_public_base_url
        .as_ref()
        .map(|b| format!("{}/webhook/clawgotcha", b.trim_end_matches('/')));

    sync.bootstrap(&rt, callback)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("clawgotcha bootstrap")?;

    crate::health::mark_component_ok("clawgotcha");

    println!(
        "   Clawgotcha: registered instance {:?} at {}",
        rt.instance_name, rt.base_url
    );
    tracing::info!(
        url = %rt.base_url,
        instance = %rt.instance_name,
        "Registered with Clawgotcha; sync loop running"
    );

    sync.run_periodic(rt, webhook_rx)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("clawgotcha sync loop")
}
