use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::Mutex;

use crate::agent::session_record::sessions_root_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrokBrowserSession {
    pub conversation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSidecar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok_browser: Option<GrokBrowserSession>,
}

#[derive(Default)]
struct StoreInner {
    memory: HashMap<String, GrokBrowserSession>,
}

pub struct GrokSessionStore {
    inner: Mutex<StoreInner>,
}

impl GrokSessionStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(StoreInner::default()),
        }
    }

    pub async fn get(&self, session_key: &str) -> Option<GrokBrowserSession> {
        let key = session_key.trim();
        if key.is_empty() {
            return None;
        }
        let guard = self.inner.lock().await;
        if let Some(found) = guard.memory.get(key) {
            return Some(found.clone());
        }
        drop(guard);
        load_sidecar_file(key).ok().flatten()
    }

    pub async fn set(&self, session_key: &str, session: GrokBrowserSession) {
        let key = session_key.trim();
        if key.is_empty() {
            return;
        }
        {
            let mut guard = self.inner.lock().await;
            guard.memory.insert(key.to_string(), session.clone());
        }
        let _ = save_sidecar_file(key, &session);
    }

    pub async fn clear(&self, session_key: &str) {
        let key = session_key.trim();
        if key.is_empty() {
            return;
        }
        {
            let mut guard = self.inner.lock().await;
            guard.memory.remove(key);
        }
        if let Ok(path) = sidecar_path(key) {
            let _ = std::fs::remove_file(path);
        }
    }

    pub async fn hydrate_from_sidecar(&self, session_key: &str, sidecar: &ProviderSidecar) {
        if let Some(session) = sidecar.grok_browser.clone() {
            let mut guard = self.inner.lock().await;
            guard.memory.insert(session_key.trim().to_string(), session);
        }
    }

    pub async fn snapshot_sidecar(&self, session_key: &str) -> Option<ProviderSidecar> {
        self.get(session_key).await.map(|session| ProviderSidecar {
            grok_browser: Some(session),
        })
    }
}

fn sidecar_dir() -> Option<PathBuf> {
    sessions_root_dir().map(|root| root.join("grok-browser"))
}

fn safe_key(session_key: &str) -> String {
    session_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn sidecar_path(session_key: &str) -> std::io::Result<PathBuf> {
    let dir = sidecar_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "sessions root unavailable")
    })?;
    Ok(dir.join(format!("{}.json", safe_key(session_key))))
}

fn load_sidecar_file(session_key: &str) -> std::io::Result<Option<GrokBrowserSession>> {
    let path = match sidecar_path(session_key) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)?;
    let session: GrokBrowserSession = serde_json::from_str(&raw)?;
    Ok(Some(session))
}

fn save_sidecar_file(session_key: &str, session: &GrokBrowserSession) -> std::io::Result<()> {
    let Some(dir) = sidecar_dir() else {
        return Ok(());
    };
    std::fs::create_dir_all(&dir)?;
    let path = sidecar_path(session_key)?;
    let payload = serde_json::to_string_pretty(session)?;
    std::fs::write(path, payload)
}

pub fn sync_sidecar_to_disk(session_key: &str, sidecar: &ProviderSidecar) -> std::io::Result<()> {
    if let Some(session) = sidecar.grok_browser.as_ref() {
        save_sidecar_file(session_key, session)?;
    }
    Ok(())
}

pub fn clear_sidecar_file(session_key: &str) -> std::io::Result<()> {
    if let Ok(path) = sidecar_path(session_key) {
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn sidecar_from_disk(session_key: &str) -> Option<ProviderSidecar> {
    load_sidecar_file(session_key)
        .ok()
        .flatten()
        .map(|session| ProviderSidecar {
            grok_browser: Some(session),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn store_round_trip_in_memory() {
        let store = GrokSessionStore::new();
        let session = GrokBrowserSession {
            conversation_id: "conv-1".into(),
            agent_id: None,
            model: "fast".into(),
            tab_id: None,
        };
        store.set("cli:test", session.clone()).await;
        let loaded = store.get("cli:test").await;
        assert_eq!(loaded, Some(session));
        store.clear("cli:test").await;
        assert!(store.get("cli:test").await.is_none());
    }

    #[test]
    fn sidecar_file_round_trip() {
        let dir = tempdir().unwrap();
        let sidecar_root = dir.path().join("sessions");
        std::fs::create_dir_all(&sidecar_root).unwrap();
        // Temporarily rely on safe_key path logic via direct save/load helpers is hard without
        // sessions_root_dir override; test serialization instead.
        let session = GrokBrowserSession {
            conversation_id: "abc".into(),
            agent_id: Some("agent".into()),
            model: "auto".into(),
            tab_id: Some("c416".into()),
        };
        let json = serde_json::to_string(&session).unwrap();
        let parsed: GrokBrowserSession = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, session);
    }
}
