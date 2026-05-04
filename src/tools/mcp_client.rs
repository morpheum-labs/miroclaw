//! MCP (Model Context Protocol) client — connects to external tool servers.
//!
//! Supports multiple transports: stdio (spawn local process), HTTP, and SSE.

use std::collections::HashMap;
#[cfg(not(target_has_atomic = "64"))]
use std::sync::atomic::AtomicU32;
#[cfg(target_has_atomic = "64")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::RwLock;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

use crate::config::schema::McpServerConfig;
use crate::tools::mcp_credentials::McpCredentialResolver;
use crate::tools::mcp_protocol::{
    JsonRpcRequest, McpToolDef, McpToolsListResult, MCP_PROTOCOL_VERSION,
};
use crate::tools::mcp_tool_context::ToolExecutionContext;
use crate::tools::mcp_transport::{create_transport, McpTransportConn};

/// Timeout for receiving a response from an MCP server during init/list.
/// Prevents a hung server from blocking the daemon indefinitely.
const RECV_TIMEOUT_SECS: u64 = 30;

/// Default timeout for tool calls (seconds) when not configured per-server.
const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 180;

/// Maximum allowed tool call timeout (seconds) — hard safety ceiling.
const MAX_TOOL_TIMEOUT_SECS: u64 = 600;

// ── Internal server state ──────────────────────────────────────────────────

struct McpServerInner {
    config: McpServerConfig,
    transport: Box<dyn McpTransportConn>,
    #[cfg(target_has_atomic = "64")]
    next_id: AtomicU64,
    #[cfg(not(target_has_atomic = "64"))]
    next_id: AtomicU32,
    tools: Vec<McpToolDef>,
}

// ── McpServer ──────────────────────────────────────────────────────────────

/// A live connection to one MCP server (any transport).
#[derive(Clone)]
pub struct McpServer {
    inner: Arc<Mutex<McpServerInner>>,
}

impl McpServer {
    /// Connect to the server, perform the initialize handshake, and fetch the tool list.
    pub async fn connect(config: McpServerConfig) -> Result<Self> {
        // Create transport based on config
        let mut transport = create_transport(&config).with_context(|| {
            format!(
                "failed to create transport for MCP server `{}`",
                config.name
            )
        })?;

        // Initialize handshake
        let id = 1u64;
        let init_req = JsonRpcRequest::new(
            id,
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "zeroclaw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        );

        let init_resp = timeout(
            Duration::from_secs(RECV_TIMEOUT_SECS),
            transport.send_and_recv(&init_req),
        )
        .await
        .with_context(|| {
            format!(
                "MCP server `{}` timed out after {}s waiting for initialize response",
                config.name, RECV_TIMEOUT_SECS
            )
        })??;

        if init_resp.error.is_some() {
            bail!(
                "MCP server `{}` rejected initialize: {:?}",
                config.name,
                init_resp.error
            );
        }

        // Notify server that client is initialized (no response expected for notifications)
        // For notifications, we send but don't wait for response
        let notif = JsonRpcRequest::notification("notifications/initialized", json!({}));
        // Best effort - ignore errors for notifications
        let _ = transport.send_and_recv(&notif).await;

        // Fetch available tools
        let id = 2u64;
        let list_req = JsonRpcRequest::new(id, "tools/list", json!({}));

        let list_resp = timeout(
            Duration::from_secs(RECV_TIMEOUT_SECS),
            transport.send_and_recv(&list_req),
        )
        .await
        .with_context(|| {
            format!(
                "MCP server `{}` timed out after {}s waiting for tools/list response",
                config.name, RECV_TIMEOUT_SECS
            )
        })??;

        let result = list_resp
            .result
            .ok_or_else(|| anyhow!("tools/list returned no result from `{}`", config.name))?;
        let tool_list: McpToolsListResult = serde_json::from_value(result)
            .with_context(|| format!("failed to parse tools/list from `{}`", config.name))?;

        let tool_count = tool_list.tools.len();

        let inner = McpServerInner {
            config,
            transport,
            #[cfg(target_has_atomic = "64")]
            next_id: AtomicU64::new(3), // Start at 3 since we used 1 and 2
            #[cfg(not(target_has_atomic = "64"))]
            next_id: AtomicU32::new(3), // Start at 3 since we used 1 and 2
            tools: tool_list.tools,
        };

        tracing::info!(
            "MCP server `{}` connected — {} tool(s) available",
            inner.config.name,
            tool_count
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    /// Tools advertised by this server.
    pub async fn tools(&self) -> Vec<McpToolDef> {
        self.inner.lock().await.tools.clone()
    }

    /// Server display name.
    pub async fn name(&self) -> String {
        self.inner.lock().await.config.name.clone()
    }

    /// Call a tool on this server. Returns the raw JSON result.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut inner = self.inner.lock().await;
        let id = inner.next_id.fetch_add(1, Ordering::Relaxed) as u64;
        let req = JsonRpcRequest::new(
            id,
            "tools/call",
            json!({ "name": tool_name, "arguments": arguments }),
        );

        // Use per-server tool timeout if configured, otherwise default.
        // Cap at MAX_TOOL_TIMEOUT_SECS for safety.
        let tool_timeout = inner
            .config
            .tool_timeout_secs
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS)
            .min(MAX_TOOL_TIMEOUT_SECS);

        let resp = timeout(
            Duration::from_secs(tool_timeout),
            inner.transport.send_and_recv(&req),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "MCP server `{}` timed out after {}s during tool call `{tool_name}`",
                inner.config.name,
                tool_timeout
            )
        })?
        .with_context(|| {
            format!(
                "MCP server `{}` error during tool call `{tool_name}`",
                inner.config.name
            )
        })?;

        if let Some(err) = resp.error {
            bail!("MCP tool `{tool_name}` error {}: {}", err.code, err.message);
        }
        Ok(resp.result.unwrap_or(serde_json::Value::Null))
    }
}

// ── McpRegistry ───────────────────────────────────────────────────────────

struct ScopedConn {
    server_name: String,
    agent_name: String,
    server: Arc<McpServer>,
    last_touch: std::time::Instant,
}

struct McpRegistryInner {
    configs_by_name: HashMap<String, McpServerConfig>,
    global: HashMap<String, Arc<McpServer>>,
    scoped: StdMutex<Vec<ScopedConn>>,
    /// prefixed_name → (server_name, original_tool_name)
    tool_index: HashMap<String, (String, String)>,
    resolver: Option<Arc<McpCredentialResolver>>,
    exec_ctx: Option<Arc<RwLock<ToolExecutionContext>>>,
    scoped_pool_max: usize,
}

/// Registry of MCP servers: global connections plus optional per-delegate scoped transports.
pub struct McpRegistry {
    inner: Arc<McpRegistryInner>,
}

/// Options for [`McpRegistry::connect_all_with_options`].
pub struct McpConnectOptions {
    pub resolver: Option<Arc<McpCredentialResolver>>,
    pub exec_ctx: Option<Arc<RwLock<ToolExecutionContext>>>,
    pub scoped_pool_max: usize,
}

impl Default for McpConnectOptions {
    fn default() -> Self {
        Self {
            resolver: None,
            exec_ctx: None,
            scoped_pool_max: 64,
        }
    }
}

impl McpRegistry {
    /// Connect to all configured servers. Non-fatal: failures are logged and skipped.
    pub async fn connect_all(configs: &[McpServerConfig]) -> Result<Self> {
        Self::connect_all_with_options(configs, McpConnectOptions::default()).await
    }

    /// Like [`Self::connect_all`] with optional Clawgotcha-backed credential overlays for delegates.
    pub async fn connect_all_with_options(
        configs: &[McpServerConfig],
        options: McpConnectOptions,
    ) -> Result<Self> {
        let mut configs_by_name = HashMap::with_capacity(configs.len());
        let mut global = HashMap::new();
        let mut tool_index = HashMap::new();

        for config in configs {
            configs_by_name.insert(config.name.clone(), config.clone());
            match McpServer::connect(config.clone()).await {
                Ok(server) => {
                    let arc_srv = Arc::new(server);
                    let tools = arc_srv.tools().await;
                    for tool in &tools {
                        let prefixed = format!("{}__{}", config.name, tool.name);
                        tool_index.insert(prefixed, (config.name.clone(), tool.name.clone()));
                    }
                    global.insert(config.name.clone(), arc_srv);
                }
                Err(e) => {
                    tracing::error!("Failed to connect to MCP server `{}`: {:#}", config.name, e);
                }
            }
        }

        Ok(Self {
            inner: Arc::new(McpRegistryInner {
                configs_by_name,
                global,
                scoped: StdMutex::new(Vec::new()),
                tool_index,
                resolver: options.resolver,
                exec_ctx: options.exec_ctx,
                scoped_pool_max: options.scoped_pool_max.max(1),
            }),
        })
    }

    /// Drop cached delegate-scoped MCP transports (global pool unchanged).
    pub fn evict_scoped_connections(&self) {
        self.inner
            .scoped
            .lock()
            .expect("scoped mutex poisoned")
            .clear();
    }

    async fn resolve_server(&self, server_name: &str) -> Result<Arc<McpServer>> {
        let delegate_opt = self
            .inner
            .exec_ctx
            .as_ref()
            .and_then(|cx| cx.read().active_delegate.clone());

        if let (Some(agent), Some(resolver)) = (delegate_opt, self.inner.resolver.as_ref()) {
            if let Some(auth) = resolver
                .resolve_for_server(Some(agent.as_str()), server_name)
                .await
            {
                let now = std::time::Instant::now();
                {
                    let mut guard = self.inner.scoped.lock().expect("scoped mutex poisoned");
                    if let Some(i) = guard
                        .iter()
                        .position(|s| s.server_name == server_name && s.agent_name == agent)
                    {
                        guard[i].last_touch = now;
                        return Ok(Arc::clone(&guard[i].server));
                    }
                }

                let base = self
                    .inner
                    .configs_by_name
                    .get(server_name)
                    .ok_or_else(|| anyhow!("unknown MCP server `{server_name}`"))?
                    .clone();
                let mut merged = base;
                merged.headers.extend(auth.headers);
                merged.env.extend(auth.env);
                let connected = Arc::new(McpServer::connect(merged).await?);

                let mut guard = self.inner.scoped.lock().expect("scoped mutex poisoned");
                let now = std::time::Instant::now();
                if let Some(i) = guard
                    .iter()
                    .position(|s| s.server_name == server_name && s.agent_name == agent)
                {
                    guard[i].last_touch = now;
                    return Ok(Arc::clone(&guard[i].server));
                }
                guard.push(ScopedConn {
                    server_name: server_name.to_string(),
                    agent_name: agent.clone(),
                    server: Arc::clone(&connected),
                    last_touch: now,
                });
                while guard.len() > self.inner.scoped_pool_max {
                    if let Some(oldest_idx) = guard
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, s)| s.last_touch)
                        .map(|(i, _)| i)
                    {
                        guard.remove(oldest_idx);
                    } else {
                        break;
                    }
                }
                return Ok(connected);
            }
        }

        self.inner
            .global
            .get(server_name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown MCP server `{server_name}`"))
    }

    /// All prefixed tool names across all connected servers.
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_index.keys().cloned().collect()
    }

    /// Tool definition for a given prefixed name (cloned). Uses the global connection's schema.
    pub async fn get_tool_def(&self, prefixed_name: &str) -> Option<McpToolDef> {
        let (server_name, original_name) = self.inner.tool_index.get(prefixed_name)?;
        let srv = self.inner.global.get(server_name)?;
        let inner = srv.inner.lock().await;
        inner
            .tools
            .iter()
            .find(|t| &t.name == original_name)
            .cloned()
    }

    /// Execute a tool by prefixed name.
    pub async fn call_tool(
        &self,
        prefixed_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        let (server_name, original_name) = self
            .inner
            .tool_index
            .get(prefixed_name)
            .ok_or_else(|| anyhow!("unknown MCP tool `{prefixed_name}`"))?;
        let server = self.resolve_server(server_name).await?;
        let result = server.call_tool(original_name, arguments).await?;
        serde_json::to_string_pretty(&result)
            .with_context(|| format!("failed to serialize result of MCP tool `{prefixed_name}`"))
    }

    pub fn is_empty(&self) -> bool {
        self.inner.global.is_empty()
    }

    pub fn server_count(&self) -> usize {
        self.inner.global.len()
    }

    pub fn tool_count(&self) -> usize {
        self.inner.tool_index.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::McpTransport;

    #[test]
    fn tool_name_prefix_format() {
        let prefixed = format!("{}__{}", "filesystem", "read_file");
        assert_eq!(prefixed, "filesystem__read_file");
    }

    #[tokio::test]
    async fn connect_nonexistent_command_fails_cleanly() {
        // A command that doesn't exist should fail at spawn, not panic.
        let config = McpServerConfig {
            name: "nonexistent".to_string(),
            command: "/usr/bin/this_binary_does_not_exist_zeroclaw_test".to_string(),
            args: vec![],
            env: std::collections::HashMap::default(),
            tool_timeout_secs: None,
            transport: McpTransport::Stdio,
            url: None,
            headers: std::collections::HashMap::default(),
        };
        let result = McpServer::connect(config).await;
        assert!(result.is_err());
        let msg = result.err().unwrap().to_string();
        assert!(msg.contains("failed to create transport"), "got: {msg}");
    }

    #[tokio::test]
    async fn connect_all_nonfatal_on_single_failure() {
        // If one server config is bad, connect_all should succeed (with 0 servers).
        let configs = vec![McpServerConfig {
            name: "bad".to_string(),
            command: "/usr/bin/does_not_exist_zc_test".to_string(),
            args: vec![],
            env: std::collections::HashMap::default(),
            tool_timeout_secs: None,
            transport: McpTransport::Stdio,
            url: None,
            headers: std::collections::HashMap::default(),
        }];
        let registry = McpRegistry::connect_all(&configs)
            .await
            .expect("connect_all should not fail");
        assert!(registry.is_empty());
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn http_transport_requires_url() {
        let config = McpServerConfig {
            name: "test".into(),
            transport: McpTransport::Http,
            ..Default::default()
        };
        let result = create_transport(&config);
        assert!(result.is_err());
    }

    #[test]
    fn sse_transport_requires_url() {
        let config = McpServerConfig {
            name: "test".into(),
            transport: McpTransport::Sse,
            ..Default::default()
        };
        let result = create_transport(&config);
        assert!(result.is_err());
    }

    // ── Empty registry (no servers) ────────────────────────────────────────

    #[tokio::test]
    async fn empty_registry_is_empty() {
        let registry = McpRegistry::connect_all(&[])
            .await
            .expect("connect_all on empty slice should succeed");
        assert!(registry.is_empty());
        assert_eq!(registry.server_count(), 0);
        assert_eq!(registry.tool_count(), 0);
    }

    #[tokio::test]
    async fn empty_registry_tool_names_is_empty() {
        let registry = McpRegistry::connect_all(&[])
            .await
            .expect("connect_all should succeed");
        assert!(registry.tool_names().is_empty());
    }

    #[tokio::test]
    async fn empty_registry_get_tool_def_returns_none() {
        let registry = McpRegistry::connect_all(&[])
            .await
            .expect("connect_all should succeed");
        let result = registry.get_tool_def("nonexistent__tool").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn empty_registry_call_tool_unknown_name_returns_error() {
        let registry = McpRegistry::connect_all(&[])
            .await
            .expect("connect_all should succeed");
        let err = registry
            .call_tool("nonexistent__tool", serde_json::json!({}))
            .await
            .expect_err("should fail for unknown tool");
        assert!(err.to_string().contains("unknown MCP tool"), "got: {err}");
    }

    #[tokio::test]
    async fn connect_all_empty_gives_zero_servers() {
        let registry = McpRegistry::connect_all(&[])
            .await
            .expect("connect_all should succeed");
        // Verify all three count methods agree on zero.
        assert_eq!(registry.server_count(), 0);
        assert_eq!(registry.tool_count(), 0);
        assert!(registry.is_empty());
    }
}
