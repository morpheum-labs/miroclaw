pub mod builtin;
pub mod planner_payloads;
mod runner;
mod traits;

use std::sync::Arc;

use crate::config::HooksConfig;

pub use runner::HookRunner;
// HookHandler and HookResult are part of the crate's public hook API surface.
// They may appear unused internally but are intentionally re-exported for
// external integrations and future plugin authors.
#[allow(unused_imports)]
pub use traits::{HookHandler, HookResult};

/// Build the default [`HookRunner`] from `[hooks]` config (builtins + optional consolidation).
#[must_use]
pub fn hook_runner_from_config(
    hooks_cfg: &HooksConfig,
    auto_save_memory: bool,
    provider: Arc<dyn crate::providers::Provider>,
    model: String,
    memory: Arc<dyn crate::memory::Memory>,
) -> Option<Arc<HookRunner>> {
    if !hooks_cfg.enabled && !auto_save_memory {
        return None;
    }
    let mut runner = HookRunner::new();
    if hooks_cfg.enabled {
        if hooks_cfg.builtin.command_logger {
            runner.register(Box::new(builtin::CommandLoggerHook::new()));
        }
        if hooks_cfg.builtin.webhook_audit.enabled {
            runner.register(Box::new(builtin::WebhookAuditHook::new(
                hooks_cfg.builtin.webhook_audit.clone(),
            )));
        }
    }
    if auto_save_memory {
        runner.register(Box::new(builtin::MemoryConsolidationHook::new(
            Arc::clone(&provider),
            model,
            memory,
        )));
    }
    Some(Arc::new(runner))
}
