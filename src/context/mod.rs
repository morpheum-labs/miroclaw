//! Dynamic and hierarchical context assembly for the agent loop (Claude Code–style roadmap).
//!
//! # Cache invalidation
//!
//! [`fingerprint::ContextFingerprint`] changes when:
//! - Any discovered instruction file’s path or modification time changes (`AGENTS.md`, `CLAUDE.md`, …).
//! - `git rev-parse HEAD` changes (when present).
//!
//! When `[agent.dynamic_context]` is disabled, [`format_dynamic_context_block`] returns an empty string.
//!
//! Cross-call memoization: [`format_dynamic_context_block`] skips expensive git snapshot work when the
//! workspace fingerprint and `[agent.dynamic_context]` git options match a recent cache entry.

#![allow(unused_imports)] // `pub use` reexports are for the library surface; the CLI bin shares this tree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use parking_lot::Mutex;

pub mod assembler;
pub mod fingerprint;
pub mod git_snapshot;
pub mod layers;

pub use assembler::{
    AssembledContext, ContextAssembler, ContextAssemblyInput, ContextAssemblyOptions,
    DefaultContextAssembler,
};
pub use fingerprint::{
    compute_fingerprint, git_head_sha, instruction_files_with_mtime, ContextFingerprint,
};
pub use git_snapshot::{capture_git_snapshot, GitSnapshot};
pub use layers::{collect_layered_instruction_paths, ContextLayer, INSTRUCTION_FILENAMES};

/// Max workspace entries kept for cross-call dynamic-context memoization; evicts one arbitrary key when full.
const DYNAMIC_CONTEXT_CACHE_MAX_ENTRIES: usize = 48;

struct DynamicContextCacheEntry {
    fingerprint: ContextFingerprint,
    include_git: bool,
    max_git_log_lines: usize,
    block: String,
}

static DYNAMIC_CONTEXT_BLOCK_CACHE: OnceLock<Mutex<HashMap<PathBuf, DynamicContextCacheEntry>>> =
    OnceLock::new();

fn dynamic_context_block_cache() -> &'static Mutex<HashMap<PathBuf, DynamicContextCacheEntry>> {
    DYNAMIC_CONTEXT_BLOCK_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Optional roots for hierarchical instruction discovery (`AGENTS.md`, `CLAUDE.md`, …).
#[derive(Debug, Clone, Copy, Default)]
pub struct DynamicContextPaths<'a> {
    pub global_config_dir: Option<&'a Path>,
    pub user_config_dir: Option<&'a Path>,
    pub session_dir: Option<&'a Path>,
}

/// Directory name for per-user data under the home directory (`~/.miroclaw`, leading dot included).
pub const USER_DATA_DIR_NAME: &str = ".miroclaw";
/// Pre-dot-prefix preview path (undotted `miroclaw` in home).
const USER_DATA_DIR_PLAIN_NAME: &str = "miroclaw";

/// Preferred per-user data directory: `~/.miroclaw` if it exists, else `~/miroclaw` when present,
/// else `~/.miroclaw` for a fresh install.
#[must_use]
pub fn preferred_user_data_dir_in_home(home: &Path) -> PathBuf {
    let m = home.join(USER_DATA_DIR_NAME);
    let m_plain = home.join(USER_DATA_DIR_PLAIN_NAME);
    if m.exists() {
        m
    } else if m_plain.exists() {
        m_plain
    } else {
        m
    }
}

/// Returns the preferred per-user data path when a home directory is available.
#[must_use]
pub fn default_user_miroclaw_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| preferred_user_data_dir_in_home(b.home_dir()))
}

/// Builds the markdown block appended to the system prompt when `[agent.dynamic_context]` is enabled.
pub fn format_dynamic_context_block(
    cfg: &crate::config::DynamicContextConfig,
    workspace: &Path,
    paths: DynamicContextPaths<'_>,
) -> Result<String> {
    if !cfg.enabled {
        return Ok(String::new());
    }
    let input = ContextAssemblyInput {
        workspace: workspace.to_path_buf(),
        global_config_dir: paths.global_config_dir.map(Path::to_path_buf),
        user_config_dir: paths.user_config_dir.map(Path::to_path_buf),
        session_dir: paths.session_dir.map(Path::to_path_buf),
        options: ContextAssemblyOptions {
            enabled: true,
            include_git_snapshot: cfg.include_git,
            max_git_log_lines: cfg.max_git_log_lines,
        },
    };
    let fp = DefaultContextAssembler.fingerprint_only(&input);
    let ws = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    {
        let guard = dynamic_context_block_cache().lock();
        if let Some(ent) = guard.get(&ws) {
            if ent.fingerprint == fp
                && ent.include_git == cfg.include_git
                && ent.max_git_log_lines == cfg.max_git_log_lines
            {
                return Ok(ent.block.clone());
            }
        }
    }
    let assembled = DefaultContextAssembler.assemble(&input)?;
    let block = assembled.dynamic_block;
    let mut guard = dynamic_context_block_cache().lock();
    if guard.len() >= DYNAMIC_CONTEXT_CACHE_MAX_ENTRIES && !guard.contains_key(&ws) {
        if let Some(k) = guard.keys().next().cloned() {
            guard.remove(&k);
        }
    }
    guard.insert(
        ws,
        DynamicContextCacheEntry {
            fingerprint: fp,
            include_git: cfg.include_git,
            max_git_log_lines: cfg.max_git_log_lines,
            block: block.clone(),
        },
    );
    Ok(block)
}

/// Drop memoized dynamic-context block for `workspace` so the next prompt assembly recomputes from disk.
pub fn clear_dynamic_context_block_cache_for_workspace(workspace: &Path) {
    let ws = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let mut guard = dynamic_context_block_cache().lock();
    guard.remove(&ws);
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn format_dynamic_context_hits_cache_second_call() {
        let tmp = tempdir().unwrap();
        let ws = tmp.path();
        std::fs::write(ws.join("AGENTS.md"), b"# test\n").unwrap();
        let paths = DynamicContextPaths {
            global_config_dir: None,
            user_config_dir: None,
            session_dir: None,
        };
        let mut cfg = crate::config::DynamicContextConfig::default();
        cfg.enabled = true;
        cfg.include_git = false;
        let a = format_dynamic_context_block(&cfg, ws, paths).unwrap();
        let b = format_dynamic_context_block(&cfg, ws, paths).unwrap();
        assert_eq!(a, b);
    }
}
