//! Clawgotcha control-plane client and sync orchestration for Miroclaw / ZeroClaw.
//!
//! Host binaries depend on this crate for HTTP adapters, delta sync, and trait ports;
//! concrete `DelegateAgentConfig` / cron DB wiring lives in the `zeroclaw` library.

#![warn(clippy::all)]

pub mod client;
pub mod config;
pub mod error;
pub mod events;
pub mod models;
pub mod sync;
pub mod traits;

pub use error::ClawgotchaError;
pub use events::ChangeEvent;
pub use traits::{
    AgentRuntimeUpdater, ChangeEventSink, ClawgotchaClient, ConfigReconciler, CronSchedulerUpdater,
    MpscEventSink, NoOpChangeSink,
};
