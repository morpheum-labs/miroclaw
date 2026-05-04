//! Process-wide scope for MCP tool execution (active delegate agent name).

use parking_lot::RwLock;
use std::sync::{Arc, OnceLock};

/// While set, remote MCP calls may use agent-scoped credentials from the control plane.
#[derive(Default, Debug)]
pub struct ToolExecutionContext {
    pub active_delegate: Option<String>,
}

static CTX: OnceLock<Arc<RwLock<ToolExecutionContext>>> = OnceLock::new();

/// Ensure the global context exists (idempotent). Call once during daemon/gateway startup.
pub fn init_tool_execution_context() -> Arc<RwLock<ToolExecutionContext>> {
    CTX.get_or_init(|| Arc::new(RwLock::new(ToolExecutionContext::default())))
        .clone()
}

/// Shared handle used by MCP tools and delegate runs.
pub fn tool_execution_context() -> Option<Arc<RwLock<ToolExecutionContext>>> {
    CTX.get().cloned()
}
