//! Hooks Clawgotcha agent sync to MCP scoped-connection caches.

use std::sync::{Arc, OnceLock, Weak};

use parking_lot::Mutex;

use super::mcp_client::McpRegistry;
use super::mcp_credentials::McpCredentialResolver;

struct McpVaultHandles {
    resolver: Weak<McpCredentialResolver>,
    registry: Weak<McpRegistry>,
}

static HANDLES: OnceLock<Mutex<McpVaultHandles>> = OnceLock::new();

fn handles() -> &'static Mutex<McpVaultHandles> {
    HANDLES.get_or_init(|| {
        Mutex::new(McpVaultHandles {
            resolver: Weak::new(),
            registry: Weak::new(),
        })
    })
}

/// Register weak refs so [`invalidate_mcp_scoped_state`] can clear caches after control-plane updates.
pub fn register_mcp_vault_for_invalidation(
    resolver: Option<Arc<McpCredentialResolver>>,
    registry: Arc<McpRegistry>,
) {
    let mut g = handles().lock();
    g.registry = Arc::downgrade(&registry);
    g.resolver = resolver.as_ref().map(Arc::downgrade).unwrap_or_default();
}

/// Invalidate resolver entries and drop scoped MCP transports (global connections unchanged).
pub fn invalidate_mcp_scoped_state() {
    let g = handles().lock();
    if let Some(r) = g.resolver.upgrade() {
        r.invalidate_all();
    }
    if let Some(reg) = g.registry.upgrade() {
        reg.evict_scoped_connections();
    }
}
