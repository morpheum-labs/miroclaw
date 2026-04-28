//! Clawgotcha integration glue (trait stubs, mapping, supervised sync task).

mod glue;
pub mod mapping;

use std::sync::Arc;

use anyhow::Context;

use crate::config::Config;

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

    Ok(clawgotcha::config::ClawgotchaRuntimeConfig {
        base_url: url.to_string(),
        instance_name,
        sync_mode,
        heartbeat_interval_secs: config.clawgotcha.heartbeat_interval_seconds,
        poll_interval_secs: config.clawgotcha.poll_interval_seconds,
        callback_public_base_url: config.clawgotcha.callback_public_base_url.clone(),
        webhook_hmac_secret: config.clawgotcha.webhook_hmac_secret.clone(),
    })
}

/// Registration + heartbeat + periodic delta sync + webhook fan-in (daemon supervisor restarts on failure).
pub async fn run_sync_supervised(
    config: Config,
    webhook_rx: tokio::sync::mpsc::Receiver<clawgotcha::ChangeEvent>,
) -> anyhow::Result<()> {
    crate::health::mark_component_ok("clawgotcha");

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

    let reconciler = Arc::new(glue::StubReconciler);
    let agents = Arc::new(glue::StubAgents);
    let cron = Arc::new(glue::StubCron);
    let sink = Arc::new(clawgotcha::NoOpChangeSink);

    let sync = clawgotcha::sync::SyncService::new(
        client, revisions, offline, reconciler, agents, cron, sink,
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

    sync.run_periodic(rt, webhook_rx)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("clawgotcha sync loop")
}
