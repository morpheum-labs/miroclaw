//! Hierarchical discovery of instruction files (global → user → workspace → session).
//!
//! Filenames align with [`crate::agent::prompt::IdentitySection`] plus `CLAUDE.md` / `CONTEXT.md`
//! from the Claude Code–style integration roadmap.

use std::path::{Path, PathBuf};

/// Instruction file names checked in each layer (workspace, user dir, session dir).
pub const INSTRUCTION_FILENAMES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "CONTEXT.md",
    "SOUL.md",
    "TOOLS.md",
    "IDENTITY.md",
    "USER.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
    "MEMORY.md",
];

/// Layer from general to specific (roadmap: managed → user → project → session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextLayer {
    /// Reserved for future managed / system-wide defaults.
    Global,
    /// User config directory (e.g. `~/.miroclaw/`).
    User,
    /// Current workspace / repository root.
    Workspace,
    /// Session-scoped directory when present.
    Session,
}

/// Returns `(layer, path)` for each file that exists, in stable order:
/// global → user → workspace → session, and within each layer alphabetical by filename.
pub fn collect_layered_instruction_paths(
    global_dir: Option<&Path>,
    user_dir: Option<&Path>,
    workspace: &Path,
    session_dir: Option<&Path>,
) -> Vec<(ContextLayer, PathBuf)> {
    let mut out = Vec::new();

    if let Some(dir) = global_dir {
        push_existing_layer(&mut out, ContextLayer::Global, dir);
    }
    if let Some(dir) = user_dir {
        push_existing_layer(&mut out, ContextLayer::User, dir);
    }
    push_existing_layer(&mut out, ContextLayer::Workspace, workspace);
    if let Some(dir) = session_dir {
        push_existing_layer(&mut out, ContextLayer::Session, dir);
    }

    out
}

fn push_existing_layer(out: &mut Vec<(ContextLayer, PathBuf)>, layer: ContextLayer, dir: &Path) {
    for name in INSTRUCTION_FILENAMES {
        let p = dir.join(name);
        if p.is_file() {
            out.push((layer, p));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collects_workspace_file_only() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        fs::write(ws.join("AGENTS.md"), b"x").unwrap();
        let paths = collect_layered_instruction_paths(None, None, ws, None);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, ContextLayer::Workspace);
        assert!(paths[0].1.ends_with("AGENTS.md"));
    }

    #[test]
    fn session_over_workspace_distinct_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        let sess = tmp.path().join("session");
        fs::create_dir_all(&sess).unwrap();
        fs::write(ws.join("IDENTITY.md"), b"w").unwrap();
        fs::write(sess.join("IDENTITY.md"), b"s").unwrap();
        let paths = collect_layered_instruction_paths(None, None, ws, Some(&sess));
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].0, ContextLayer::Workspace);
        assert_eq!(paths[1].0, ContextLayer::Session);
    }
}
