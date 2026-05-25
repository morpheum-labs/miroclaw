//! In-flight channel session registry with event fan-out for hub attach/monitor.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

const DEFAULT_REPLAY_CAP: usize = 128;

#[derive(Debug)]
struct ReplayState {
    events: VecDeque<(u64, serde_json::Value)>,
    next_seq: u64,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            next_seq: 1,
        }
    }
}

impl ReplayState {
    fn push_event(&mut self, cap: usize, mut value: serde_json::Value) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        if let Some(obj) = value.as_object_mut() {
            obj.insert("seq".to_string(), seq.into());
        }
        self.events.push_back((seq, value));
        while self.events.len() > cap {
            self.events.pop_front();
        }
        seq
    }

    fn snapshot_after(&self, after_seq: Option<u64>) -> Vec<serde_json::Value> {
        self.events
            .iter()
            .filter(|(seq, _)| after_seq.map_or(true, |s| *seq > s))
            .map(|(_, v)| v.clone())
            .collect()
    }
}

#[derive(Debug)]
struct ChannelSessionState {
    replay: ReplayState,
    subscribers: Vec<tokio::sync::mpsc::Sender<serde_json::Value>>,
}

impl ChannelSessionState {
    fn emit(&mut self, cap: usize, value: serde_json::Value) {
        let seq = self.replay.push_event(cap, value);
        let payload = self
            .replay
            .events
            .iter()
            .find(|(s, _)| *s == seq)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| serde_json::json!({"type":"error","message":"replay miss"}));
        self.subscribers
            .retain(|tx| match tx.try_send(payload.clone()) {
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => false,
                Ok(()) | Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => true,
            });
    }
}

struct ChannelSessionStore {
    sessions: HashMap<String, ChannelSessionState>,
}

impl ChannelSessionStore {
    fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }
}

static STORE: OnceLock<Mutex<ChannelSessionStore>> = OnceLock::new();

fn store() -> &'static Mutex<ChannelSessionStore> {
    STORE.get_or_init(|| Mutex::new(ChannelSessionStore::new()))
}

fn with_store<R>(f: impl FnOnce(&mut ChannelSessionStore) -> R) -> R {
    let mut g = store().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

/// Register an in-flight channel turn (idempotent per key).
pub fn register_channel_session(key: &str) {
    with_store(|g| {
        g.sessions.entry(key.to_string()).or_default();
    });
}

/// Remove session state when the channel turn completes.
pub fn unregister_channel_session(key: &str) {
    with_store(|g| {
        g.sessions.remove(key);
    });
}

/// Active channel session keys (in-flight turns).
pub fn active_channel_session_keys() -> Vec<String> {
    with_store(|g| g.sessions.keys().cloned().collect())
}

/// Emit a JSON event to all attach subscribers for this channel session.
pub fn emit_channel_session_event(key: &str, value: serde_json::Value) {
    with_store(|g| {
        if let Some(state) = g.sessions.get_mut(key) {
            state.emit(DEFAULT_REPLAY_CAP, value);
        }
    });
}

/// Attach a subscriber; returns replay snapshot and a live event channel.
pub async fn attach_channel_session(
    key: &str,
    after_seq: Option<u64>,
) -> Result<
    (
        Vec<serde_json::Value>,
        tokio::sync::mpsc::Receiver<serde_json::Value>,
    ),
    &'static str,
> {
    let (live_tx, live_rx) = tokio::sync::mpsc::channel(256);
    let snapshot = with_store(|g| -> Result<Vec<serde_json::Value>, &'static str> {
        let state = g
            .sessions
            .get_mut(key)
            .ok_or("channel session not active")?;
        let snap = state.replay.snapshot_after(after_seq);
        state.subscribers.push(live_tx);
        Ok(snap)
    })?;
    Ok((snapshot, live_rx))
}

pub async fn unregister_channel_subscriber(key: &str) {
    with_store(|g| {
        if let Some(state) = g.sessions.get_mut(key) {
            state.subscribers.retain(|tx| !tx.is_closed());
        }
    });
}

impl Default for ChannelSessionState {
    fn default() -> Self {
        Self {
            replay: ReplayState::default(),
            subscribers: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_snapshot_after_seq() {
        let mut r = ReplayState::default();
        r.push_event(10, serde_json::json!({"type":"chunk","content":"a"}));
        r.push_event(10, serde_json::json!({"type":"chunk","content":"b"}));
        assert_eq!(r.snapshot_after(Some(1)).len(), 1);
        assert_eq!(r.snapshot_after(None).len(), 2);
    }
}
