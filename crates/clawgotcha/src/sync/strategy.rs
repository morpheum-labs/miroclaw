//! Hybrid polling + webhook coordination.

use crate::config::SyncMode;

/// Whether polling and/or webhook paths should run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridParts {
    pub poll: bool,
    pub webhook: bool,
}

impl HybridParts {
    #[must_use]
    pub fn from_mode(mode: SyncMode) -> Self {
        match mode {
            SyncMode::Poll => Self {
                poll: true,
                webhook: false,
            },
            SyncMode::Webhook => Self {
                poll: false,
                webhook: true,
            },
            SyncMode::Hybrid => Self {
                poll: true,
                webhook: true,
            },
        }
    }
}
