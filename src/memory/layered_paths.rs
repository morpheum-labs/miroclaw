//! Disk paths for layered AutoMemory + SessionMemory (`~/.miroclaw/...`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Stable directory name derived from the workspace root (multi-workspace isolation).
#[must_use]
pub fn workspace_bucket_id(workspace_dir: &Path) -> String {
    let mut h = DefaultHasher::new();
    workspace_dir.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// `~/.miroclaw` when available; otherwise `workspace/.miroclaw/`
/// as a portable fallback.
#[must_use]
pub fn resolved_miroclaw_dir(workspace_dir: &Path) -> PathBuf {
    crate::context::default_user_miroclaw_dir()
        .unwrap_or_else(|| workspace_dir.join(crate::context::USER_DATA_DIR_NAME))
}

/// AutoMemory root: `~/.miroclaw/memory/<bucket>/`.
#[must_use]
pub fn auto_memory_bucket_dir(workspace_dir: &Path) -> PathBuf {
    let z = resolved_miroclaw_dir(workspace_dir);
    z.join("memory").join(workspace_bucket_id(workspace_dir))
}

#[must_use]
pub fn auto_memory_index_path(workspace_dir: &Path) -> PathBuf {
    auto_memory_bucket_dir(workspace_dir).join("MEMORY.md")
}

#[must_use]
pub fn auto_memory_topics_dir(workspace_dir: &Path) -> PathBuf {
    auto_memory_bucket_dir(workspace_dir).join("topics")
}

/// Same stem strategy as session transcripts (safe filesystem segment + hash suffix).
#[must_use]
pub fn session_storage_stem(session_key: &str) -> String {
    let safe: String = session_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_matches('_');
    let head: String = if safe.is_empty() {
        "session".to_string()
    } else {
        safe.chars().take(120).collect()
    };
    let mut h = DefaultHasher::new();
    session_key.hash(&mut h);
    format!("{head}_{:x}", h.finish())
}

/// SessionMemory directory: `~/.miroclaw/sessions/<stem>/session-memory/`.
#[must_use]
pub fn session_memory_dir(workspace_dir: &Path, session_key: &str) -> PathBuf {
    let z = resolved_miroclaw_dir(workspace_dir);
    z.join("sessions")
        .join(session_storage_stem(session_key))
        .join("session-memory")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn workspace_bucket_is_stable() {
        let d = tempdir().unwrap();
        let a = workspace_bucket_id(d.path());
        let b = workspace_bucket_id(d.path());
        assert_eq!(a, b);
    }

    #[test]
    fn session_stem_non_empty() {
        assert!(!session_storage_stem("tg:123").is_empty());
    }
}
