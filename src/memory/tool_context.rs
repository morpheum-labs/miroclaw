//! Per-task context for `memory_recall` / `memory_store` / `memory_forget` so multi-agent
//! runs (delegate sub-agents, coordinator workers) isolate storage without a shared
//! "default" pool.
//!
//! The tool call loop sets [`MEMORY_TOOL_NAMESPACE`] (tokio task-local). The WebSocket
//! [`crate::agent::agent::Agent`] path uses a thread-safe stack instead so the same
//! `memory_*` tools work without a nested `scope()`.

use std::sync::{LazyLock, Mutex};

const NS_SEP: char = '\u{001f}';

static MEMORY_TOOL_STACK: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

// Task-local: effective namespace for the memory *tools* (string label, matches `namespace` on rows).
tokio::task_local! {
    pub static MEMORY_TOOL_NAMESPACE: String;
}

/// Push a namespace for the current sync agent turn (e.g. WebSocket). Paired with [`pop_memory_tool_stack`].
pub fn push_memory_tool_stack(namespace: String) {
    if let Ok(mut g) = MEMORY_TOOL_STACK.lock() {
        g.push(namespace);
    }
}

/// Pop the innermost entry from [`push_memory_tool_stack`]. Call once per `push` (e.g. in a `turn` `defer`-style `Drop` guard).
pub fn pop_memory_tool_stack() {
    if let Ok(mut g) = MEMORY_TOOL_STACK.lock() {
        g.pop();
    }
}

/// RAII: push on construction, pop on drop (per-turn memory tool namespace for the WebSocket agent).
pub struct MemoryToolStackGuard {
    _private: (),
}

/// RAII: [`push_memory_tool_stack`] on construction, [`pop_memory_tool_stack`] on drop.
#[must_use]
pub fn memory_tool_stack_guard(namespace: String) -> MemoryToolStackGuard {
    push_memory_tool_stack(namespace);
    MemoryToolStackGuard { _private: () }
}

impl Drop for MemoryToolStackGuard {
    fn drop(&mut self) {
        pop_memory_tool_stack();
    }
}

/// Namespace for memory tools: task-local (primary), then per-turn stack, then `"default"`.
#[must_use]
pub fn effective_memory_tool_namespace() -> String {
    if let Ok(ns) = MEMORY_TOOL_NAMESPACE.try_with(std::string::String::clone) {
        return ns;
    }
    if let Ok(g) = MEMORY_TOOL_STACK.lock() {
        if let Some(n) = g.last() {
            return n.clone();
        }
    }
    "default".to_string()
}

/// Physical `memories.key` for sqlite/postgres: globally unique (table has `UNIQUE(key)`) and
/// stable per (namespace, logical key). When `namespace` is `"default"`, preserves legacy keys.
#[must_use]
pub fn memory_storage_key(namespace: &str, logical_key: &str) -> String {
    if namespace == "default" {
        logical_key.to_string()
    } else {
        format!("{namespace}{NS_SEP}{logical_key}")
    }
}
