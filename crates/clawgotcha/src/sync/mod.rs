//! Sync orchestration (polling + webhook fan-in).

mod offline;
mod queue;
mod revision;
mod service;
mod strategy;

pub use offline::FileOfflineCache;
pub use queue::DedupeWindow;
pub use revision::FileRevisionStore;
pub use service::SyncService;
pub use strategy::HybridParts;

/// Alias matching the design doc (`HybridSyncStrategy` coordinator flags).
pub type HybridSyncStrategy = HybridParts;
