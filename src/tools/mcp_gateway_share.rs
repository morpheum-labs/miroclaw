//! Shared MCP client state for the HTTP gateway so WebSocket agents reuse the
//! same [`McpRegistry`] instead of opening a second connection set.

use std::sync::{Arc, Mutex};

use crate::config::schema::McpConfig;

use super::mcp_client::{McpConnectOptions, McpRegistry};
use super::mcp_credentials::McpCredentialResolver;
use super::mcp_deferred::{self, ActivatedToolSet, DeferredMcpToolSet};
use super::mcp_tool::McpToolWrapper;
use super::mcp_tool_context::ToolExecutionContext;
use super::tool_search::ToolSearchTool;
use super::traits::Tool;
use super::ArcToolRef;
use super::DelegateParentToolsHandle;
use parking_lot::RwLock;

/// Optional Clawgotcha-backed MCP credential resolver + shared delegate execution scope.
pub struct McpClawgotchaVault {
    pub resolver: Arc<McpCredentialResolver>,
    pub exec_ctx: Arc<RwLock<ToolExecutionContext>>,
}

/// Result of [`attach_mcp_tools`] when wiring deferred MCP into a tool registry.
pub struct McpAttachOutcome {
    /// System prompt fragment listing deferred tool names (empty if not deferred).
    pub deferred_section: String,
    /// Per-session activated tool set handle (only for deferred MCP mode).
    pub activated_tools: Option<Arc<Mutex<ActivatedToolSet>>>,
}

/// MCP connections and optional deferred tool index built once at gateway startup.
#[derive(Clone)]
pub struct GatewayMcpBundle {
    pub registry: Arc<McpRegistry>,
    /// Present when the gateway was started with `mcp.deferred_loading = true`.
    pub deferred_set: Option<DeferredMcpToolSet>,
    /// Present when `[clawgotcha]` instance API secret is configured for MCP overlays.
    pub clawgotcha_vault: Option<Arc<McpClawgotchaVault>>,
}

impl GatewayMcpBundle {
    /// Connect to all configured MCP servers when MCP is enabled. Returns `None`
    /// when MCP is disabled or has no servers.
    ///
    /// `skill_mcp_hints` are MCP server names from workspace skills; they tune
    /// `tool_search` ranking in the shared deferred index.
    pub async fn connect_if_enabled(
        mcp: &McpConfig,
        skill_mcp_hints: &[String],
        clawgotcha_vault: Option<Arc<McpClawgotchaVault>>,
    ) -> Option<Arc<Self>> {
        if !mcp.enabled || mcp.servers.is_empty() {
            return None;
        }
        tracing::info!(
            "Gateway MCP: connecting {} server(s) (shared pool)",
            mcp.servers.len()
        );
        let options = McpConnectOptions {
            resolver: clawgotcha_vault.as_ref().map(|v| Arc::clone(&v.resolver)),
            exec_ctx: clawgotcha_vault.as_ref().map(|v| Arc::clone(&v.exec_ctx)),
            scoped_pool_max: 64,
        };
        let registry = match McpRegistry::connect_all_with_options(&mcp.servers, options).await {
            Ok(r) => Arc::new(r),
            Err(e) => {
                tracing::error!("Gateway MCP registry failed to initialize: {e:#}");
                return None;
            }
        };
        let deferred_set = if mcp.deferred_loading {
            let ds = DeferredMcpToolSet::from_registry_with_hints(
                Arc::clone(&registry),
                skill_mcp_hints,
            )
            .await;
            tracing::info!(
                "Gateway MCP deferred: {} tool stub(s) from {} server(s)",
                ds.len(),
                registry.server_count()
            );
            Some(ds)
        } else {
            tracing::info!(
                "Gateway MCP: {} tool(s) indexed from {} server(s)",
                registry.tool_count(),
                registry.server_count()
            );
            None
        };
        Some(Arc::new(Self {
            registry,
            deferred_set,
            clawgotcha_vault,
        }))
    }
}

/// Append MCP tools to an in-process tool list, either reusing [`GatewayMcpBundle`]
/// or opening new connections when `shared` is `None`.
///
/// When not using a shared bundle, `skill_mcp_hints` seeds deferred `tool_search` ranking.
///
pub async fn attach_mcp_tools(
    tools: &mut Vec<Box<dyn Tool>>,
    delegate_handle: Option<&DelegateParentToolsHandle>,
    mcp: &McpConfig,
    skill_mcp_hints: &[String],
    shared: Option<&Arc<GatewayMcpBundle>>,
) -> McpAttachOutcome {
    if !mcp.enabled || mcp.servers.is_empty() {
        return McpAttachOutcome {
            deferred_section: String::new(),
            activated_tools: None,
        };
    }

    if let Some(bundle) = shared {
        let registry = Arc::clone(&bundle.registry);
        if mcp.deferred_loading {
            let deferred = match &bundle.deferred_set {
                Some(ds) => ds.clone(),
                None => {
                    DeferredMcpToolSet::from_registry_with_hints(
                        Arc::clone(&registry),
                        skill_mcp_hints,
                    )
                    .await
                }
            };
            let deferred_section = mcp_deferred::build_deferred_tools_section(&deferred);
            tracing::debug!(
                "MCP deferred (shared): {} tool stub(s) from {} server(s)",
                deferred.len(),
                registry.server_count()
            );
            let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
            let activated_tools = Some(Arc::clone(&activated));
            tools.push(Box::new(ToolSearchTool::new(deferred, activated)));
            return McpAttachOutcome {
                deferred_section,
                activated_tools,
            };
        }
        let names = registry.tool_names();
        let mut registered = 0usize;
        for name in names {
            if let Some(def) = registry.get_tool_def(&name).await {
                let wrapper: Arc<dyn Tool> =
                    Arc::new(McpToolWrapper::new(name, def, Arc::clone(&registry)));
                if let Some(handle) = delegate_handle {
                    handle.write().push(Arc::clone(&wrapper));
                }
                tools.push(Box::new(ArcToolRef(wrapper)));
                registered += 1;
            }
        }
        tracing::info!(
            "MCP (shared): {} tool(s) registered from {} server(s)",
            registered,
            registry.server_count()
        );
        return McpAttachOutcome {
            deferred_section: String::new(),
            activated_tools: None,
        };
    }

    tracing::info!(
        "Initializing MCP client — {} server(s) configured",
        mcp.servers.len()
    );
    match McpRegistry::connect_all(&mcp.servers).await {
        Ok(registry) => {
            let registry = Arc::new(registry);
            if mcp.deferred_loading {
                let deferred_set = DeferredMcpToolSet::from_registry_with_hints(
                    Arc::clone(&registry),
                    skill_mcp_hints,
                )
                .await;
                let deferred_section = mcp_deferred::build_deferred_tools_section(&deferred_set);
                tracing::info!(
                    "MCP deferred: {} tool stub(s) from {} server(s)",
                    deferred_set.len(),
                    registry.server_count()
                );
                let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
                let activated_tools = Some(Arc::clone(&activated));
                tools.push(Box::new(ToolSearchTool::new(deferred_set, activated)));
                return McpAttachOutcome {
                    deferred_section,
                    activated_tools,
                };
            }
            let names = registry.tool_names();
            let mut registered = 0usize;
            for name in names {
                if let Some(def) = registry.get_tool_def(&name).await {
                    let wrapper: Arc<dyn Tool> =
                        Arc::new(McpToolWrapper::new(name, def, Arc::clone(&registry)));
                    if let Some(handle) = delegate_handle {
                        handle.write().push(Arc::clone(&wrapper));
                    }
                    tools.push(Box::new(ArcToolRef(wrapper)));
                    registered += 1;
                }
            }
            tracing::info!(
                "MCP: {} tool(s) registered from {} server(s)",
                registered,
                registry.server_count()
            );
        }
        Err(e) => {
            tracing::error!("MCP registry failed to initialize: {e:#}");
        }
    }
    McpAttachOutcome {
        deferred_section: String::new(),
        activated_tools: None,
    }
}
