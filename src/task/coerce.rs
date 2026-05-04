//! JSON round-trip between the `miroclaw` binary’s duplicate module tree and the `zeroclaw`
//! library types so [`super::TaskRuntime`] always sees a single `Config` / `CronJob` definition.

use serde::Serialize;

use crate::config::Config;
use crate::cron::CronJob;

/// Deserialize a peer crate’s `Config` (same `schema.rs` source) into this crate’s `Config`.
pub fn config_for_task_runtime(c: &impl Serialize) -> anyhow::Result<Config> {
    let v =
        serde_json::to_value(c).map_err(|e| anyhow::anyhow!("serialize config for task: {e}"))?;
    serde_json::from_value(v).map_err(|e| anyhow::anyhow!("deserialize config for task: {e}"))
}

/// Deserialize a peer crate’s `CronJob` into this crate’s `CronJob`.
pub fn cron_job_for_task_runtime(j: &impl Serialize) -> anyhow::Result<CronJob> {
    let v =
        serde_json::to_value(j).map_err(|e| anyhow::anyhow!("serialize cron job for task: {e}"))?;
    serde_json::from_value(v).map_err(|e| anyhow::anyhow!("deserialize cron job for task: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_json_roundtrip_is_stable() {
        let c = Config::default();
        let c2 = config_for_task_runtime(&c).expect("coerce");
        let c3 = config_for_task_runtime(&c2).expect("coerce again");
        assert_eq!(c2.tasks.enabled, c3.tasks.enabled);
        assert_eq!(c2.tasks.record_cron_runs, c3.tasks.record_cron_runs);
    }
}
