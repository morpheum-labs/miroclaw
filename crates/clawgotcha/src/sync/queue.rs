//! Dedup helpers for fan-in events.

use std::collections::HashSet;

/// Bounded dedupe window (best-effort): suppress duplicate `dedupe_key()` within one sync batch.
#[derive(Debug, Default)]
pub struct DedupeWindow {
    seen: HashSet<String>,
    cap: usize,
}

impl DedupeWindow {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            seen: HashSet::new(),
            cap: cap.max(1),
        }
    }

    pub fn insert(&mut self, key: String) -> bool {
        if self.seen.contains(&key) {
            return false;
        }
        if self.seen.len() >= self.cap {
            self.seen.clear();
        }
        self.seen.insert(key);
        true
    }
}
