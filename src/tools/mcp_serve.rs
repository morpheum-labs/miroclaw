//! MCP **server**: expose a curated allowlist of ZeroClaw [`Tool`]s via JSON-RPC.
//!
//! Transports:
//! - **stdio** — one JSON-RPC object per line (matches [`mcp_transport::StdioTransport`]).
//! - **HTTP** — `POST /mcp` with a JSON-RPC body; optional `Authorization: Bearer` when
//!   `[mcp_serve].auth_token` is set (same pattern as the gateway REST API).
//!
//! Protocol: `2024-11-05`. Policy: [`McpServeConfig`](crate::config::schema::McpServeConfig).

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::{timeout, Duration};

use crate::config::Config;
use crate::memory;
use crate::runtime;
use crate::security::SecurityPolicy;
use crate::skills;
use crate::tools::mcp_protocol::{
    JsonRpcResponse, INVALID_PARAMS, JSONRPC_VERSION, MCP_PROTOCOL_VERSION, METHOD_NOT_FOUND,
    PARSE_ERROR,
};
use crate::tools::traits::Tool;
use crate::tools::{all_tools_with_runtime, register_skill_tools};

/// Tools allowed in `allowed_tools` / `--allow-tool` when `relax_tool_policy` is `false`.
const SAFE_WITHOUT_RELAX: &[&str] = &[
    "memory_recall",
    "file_read",
    "calculator",
    "weather",
    "project_intel",
    "image_info",
    "glob_search",
    "content_search",
    "pdf_read",
];

/// When `[mcp_serve].allowed_tools` and CLI `--allow-tool` are both empty, expose these names.
fn default_mcp_serve_tool_names() -> Vec<String> {
    vec!["memory_recall".into(), "file_read".into()]
}

#[derive(Debug, Deserialize)]
struct InboundRpc {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

/// Merge config + CLI allowlist; if nothing specified, use [`default_mcp_serve_tool_names`].
#[must_use]
pub fn merge_mcp_serve_allowlist(
    cfg: &crate::config::schema::McpServeConfig,
    cli: &[String],
) -> Vec<String> {
    let mut names: Vec<String> = cfg.allowed_tools.clone();
    for t in cli {
        if !t.trim().is_empty() {
            names.push(t.trim().to_string());
        }
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return default_mcp_serve_tool_names();
    }
    names
}

pub fn validate_mcp_serve_allowlist(names: &[String], relax: bool) -> Result<()> {
    if relax {
        return Ok(());
    }
    for n in names {
        if !SAFE_WITHOUT_RELAX.contains(&n.as_str()) {
            bail!(
                "mcp_serve: tool `{n}` is not in the safe preset. \
Add `[mcp_serve].relax_tool_policy = true` after reviewing risk, \
or use only: {}",
                SAFE_WITHOUT_RELAX.join(", ")
            );
        }
    }
    Ok(())
}

fn tool_to_mcp_def(tool: &dyn Tool) -> crate::tools::mcp_protocol::McpToolDef {
    let spec = tool.spec();
    crate::tools::mcp_protocol::McpToolDef {
        name: spec.name,
        description: Some(spec.description),
        input_schema: spec.parameters,
    }
}

fn call_tool_result_json(text: &str, is_error: bool) -> serde_json::Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error
    })
}

fn response_ok(id: Option<serde_json::Value>, result: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn response_err(
    id: Option<serde_json::Value>,
    code: i32,
    message: impl Into<String>,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: None,
        error: Some(crate::tools::mcp_protocol::JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }),
    }
}

/// Shared MCP JSON-RPC handler (stdio and HTTP).
pub struct McpServeRuntime {
    exposed: Vec<Box<dyn Tool>>,
    allowed: HashSet<String>,
    timeout_secs: u64,
}

impl McpServeRuntime {
    /// Build the filtered tool list from config + CLI allowlist.
    pub async fn from_config(config: &Config, cli_allow_tools: &[String]) -> Result<Self> {
        let names = merge_mcp_serve_allowlist(&config.mcp_serve, cli_allow_tools);
        validate_mcp_serve_allowlist(&names, config.mcp_serve.relax_tool_policy)?;

        let allowed: HashSet<String> = names.iter().cloned().collect();

        let _observer: Arc<dyn crate::observability::Observer> =
            Arc::from(crate::observability::create_observer(&config.observability));
        let runtime: Arc<dyn runtime::RuntimeAdapter> =
            Arc::from(runtime::create_runtime(&config.runtime)?);
        let security = Arc::new(SecurityPolicy::from_config(
            &config.autonomy,
            &config.workspace_dir,
        ));

        let mem: Arc<dyn memory::Memory> =
            Arc::from(memory::create_memory_with_storage_and_routes(
                &config.memory,
                &config.embedding_routes,
                Some(&config.storage.provider.config),
                &config.workspace_dir,
                config.api_key.as_deref(),
            )?);

        let (composio_key, composio_entity_id) = if config.composio.enabled {
            (
                config.composio.api_key.as_deref(),
                Some(config.composio.entity_id.as_str()),
            )
        } else {
            (None, None)
        };

        let (mut tools_registry, _delegate, _r1, _r2, _r3, shell_engine) = all_tools_with_runtime(
            Arc::new(config.clone()),
            &security,
            runtime,
            mem,
            composio_key,
            composio_entity_id,
            &config.browser,
            &config.http_request,
            &config.web_fetch,
            &config.workspace_dir,
            &config.agents,
            config.api_key.as_deref(),
            config,
            None,
            None,
        );

        let loaded_skills = skills::load_skills_with_config(&config.workspace_dir, config);
        register_skill_tools(
            &mut tools_registry,
            &loaded_skills,
            security.clone(),
            shell_engine,
        );

        let peripheral_tools: Vec<Box<dyn Tool>> =
            crate::peripherals::create_peripheral_tools(&config.peripherals).await?;
        tools_registry.extend(peripheral_tools);

        let mut exposed: Vec<Box<dyn Tool>> = Vec::new();
        for want in &names {
            if let Some(pos) = tools_registry.iter().position(|t| t.name() == want) {
                exposed.push(tools_registry.swap_remove(pos));
            } else {
                tracing::warn!(tool = %want, "mcp serve: tool not in registry (skipping)");
            }
        }

        if exposed.is_empty() {
            bail!(
                "mcp serve: no tools matched the allowlist (check tool names against `zeroclaw` registry)"
            );
        }

        tracing::info!(
            count = exposed.len(),
            tools = ?exposed.iter().map(|t| t.name().to_string()).collect::<Vec<_>>(),
            "MCP server tools ready"
        );

        let timeout_secs = config.mcp_serve.tool_timeout_secs.max(1);

        Ok(Self {
            exposed,
            allowed,
            timeout_secs,
        })
    }

    /// One newline-stripped JSON-RPC request line or HTTP POST body → response, or `None` for notifications.
    pub async fn handle_json_line(&self, trimmed: &str) -> Option<JsonRpcResponse> {
        let req: InboundRpc = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                return Some(response_err(None, PARSE_ERROR, format!("Parse error: {e}")));
            }
        };

        self.dispatch(req).await
    }

    async fn dispatch(&self, req: InboundRpc) -> Option<JsonRpcResponse> {
        let id = req.id.clone();

        match req.method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "zeroclaw",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                });
                Some(response_ok(id, result))
            }
            "notifications/initialized" => None,
            "tools/list" => {
                let tools: Vec<_> = self
                    .exposed
                    .iter()
                    .map(|t| tool_to_mcp_def(t.as_ref()))
                    .collect();
                Some(response_ok(id, serde_json::json!({ "tools": tools })))
            }
            "tools/call" => {
                let params = match req.params {
                    Some(p) => p,
                    None => {
                        return Some(response_err(id, INVALID_PARAMS, "missing params"));
                    }
                };
                let name = params
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                if name.is_empty() {
                    return Some(response_err(id, INVALID_PARAMS, "tools/call requires name"));
                }

                if !self.allowed.contains(name) {
                    return Some(response_err(
                        id,
                        INVALID_PARAMS,
                        format!("tool `{name}` is not on the mcp_serve allowlist"),
                    ));
                }

                let Some(tool) = self.exposed.iter().find(|t| t.name() == name) else {
                    return Some(response_err(
                        id,
                        INVALID_PARAMS,
                        format!("tool `{name}` not available"),
                    ));
                };

                let exec_fut = tool.execute(arguments);
                let outcome = timeout(Duration::from_secs(self.timeout_secs), exec_fut).await;

                let result_payload = match outcome {
                    Ok(Ok(r)) => {
                        if r.success {
                            call_tool_result_json(&r.output, false)
                        } else {
                            call_tool_result_json(
                                &r.error.unwrap_or_else(|| r.output.clone()),
                                true,
                            )
                        }
                    }
                    Ok(Err(e)) => call_tool_result_json(&format!("execution error: {e:#}"), true),
                    Err(_) => call_tool_result_json(
                        &format!("tool `{name}` timed out after {}s", self.timeout_secs),
                        true,
                    ),
                };

                Some(response_ok(id, result_payload))
            }
            "ping" => Some(response_ok(id, serde_json::json!({}))),
            _ => Some(response_err(
                id,
                METHOD_NOT_FOUND,
                format!("method not found: {}", req.method),
            )),
        }
    }
}

async fn write_response(out: &mut tokio::io::Stdout, resp: &JsonRpcResponse) -> Result<()> {
    let line = serde_json::to_string(resp).context("serialize JSON-RPC response")?;
    out.write_all(line.as_bytes()).await?;
    out.write_all(b"\n").await?;
    out.flush().await?;
    Ok(())
}

/// Build the filtered registry and run the MCP stdio loop until EOF.
pub async fn run_mcp_stdio_server(config: Config, cli_allow_tools: Vec<String>) -> Result<()> {
    let runtime = McpServeRuntime::from_config(&config, &cli_allow_tools).await?;

    tracing::info!("MCP server (stdio) ready — send JSON-RPC lines on stdin");

    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();

    loop {
        line.clear();
        let n = stdin.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(resp) = runtime.handle_json_line(trimmed).await {
            write_response(&mut stdout, &resp).await?;
        }
    }

    Ok(())
}

// ── HTTP transport ─────────────────────────────────────────────

#[derive(Clone)]
struct McpHttpState {
    runtime: Arc<McpServeRuntime>,
    auth_token: Option<String>,
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
}

fn check_http_auth(expected: Option<&String>, headers: &HeaderMap) -> Result<(), &'static str> {
    let Some(token) = expected else {
        return Ok(());
    };
    let got = extract_bearer_token(headers).unwrap_or("");
    if got == token.as_str() {
        Ok(())
    } else {
        Err("Unauthorized — send Authorization: Bearer <token> (see [mcp_serve].auth_token)")
    }
}

/// Refuse non-loopback bind when no bearer token is configured (mirrors gateway “pair before expose” intent).
pub fn validate_mcp_http_bind(bind: &str, auth_token: Option<&String>) -> Result<()> {
    if auth_token.is_some() {
        return Ok(());
    }
    if is_loopback_host(bind) {
        return Ok(());
    }
    bail!(
        "mcp_serve: HTTP bind `{bind}` is not loopback; set [mcp_serve].auth_token or bind to 127.0.0.1 / ::1"
    );
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn root(State(state): State<McpHttpState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(msg) = check_http_auth(state.auth_token.as_ref(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response();
    }
    Json(serde_json::json!({
        "service": "zeroclaw-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "post_jsonrpc": "/mcp",
    }))
    .into_response()
}

async fn mcp_post(
    State(state): State<McpHttpState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err(msg) = check_http_auth(state.auth_token.as_ref(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response();
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "empty body; expected one JSON-RPC object" })),
        )
            .into_response();
    }

    match state.runtime.handle_json_line(trimmed).await {
        Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// Axum router for MCP HTTP (for tests and embedding).
pub fn mcp_http_router(runtime: Arc<McpServeRuntime>, auth_token: Option<String>) -> Router {
    let state = McpHttpState {
        runtime,
        auth_token,
    };
    Router::new()
        .route("/mcp", post(mcp_post))
        .route("/health", get(health))
        .route("/", get(root))
        .with_state(state)
}

/// Run MCP over HTTP until the process is interrupted.
pub async fn run_mcp_http_server(
    config: Config,
    cli_allow_tools: Vec<String>,
    bind_override: Option<String>,
    port_override: Option<u16>,
) -> Result<()> {
    let runtime = Arc::new(McpServeRuntime::from_config(&config, &cli_allow_tools).await?);
    let bind = bind_override.unwrap_or_else(|| config.mcp_serve.http_bind.clone());
    let port = port_override.unwrap_or(config.mcp_serve.http_port);
    if port == 0 {
        bail!("mcp_serve: HTTP port must be non-zero");
    }
    validate_mcp_http_bind(&bind, config.mcp_serve.auth_token.as_ref())?;
    let addr = format!("{bind}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, transport = "http", "MCP server listening");
    let router = mcp_http_router(Arc::clone(&runtime), config.mcp_serve.auth_token.clone());
    axum::serve(listener, router).await.context("HTTP server")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::McpServeConfig;
    use crate::tools::traits::{Tool, ToolResult};
    use async_trait::async_trait;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    struct ContractPingTool;

    #[async_trait]
    impl Tool for ContractPingTool {
        fn name(&self) -> &str {
            "contract_ping"
        }

        fn description(&self) -> &str {
            "test"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({ "type": "object" })
        }

        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "pong".to_string(),
                error: None,
            })
        }
    }

    #[test]
    fn merge_defaults_when_empty() {
        let cfg = McpServeConfig::default();
        let m = merge_mcp_serve_allowlist(&cfg, &[]);
        assert_eq!(m, vec!["memory_recall", "file_read"]);
    }

    #[test]
    fn merge_dedupes() {
        let cfg = McpServeConfig {
            allowed_tools: vec!["calculator".into(), "file_read".into()],
            ..Default::default()
        };
        let m = merge_mcp_serve_allowlist(&cfg, &["file_read".into()]);
        assert_eq!(m, vec!["calculator", "file_read"]);
    }

    #[test]
    fn validate_rejects_unsafe_without_relax() {
        let err = validate_mcp_serve_allowlist(&["shell".into()], false).unwrap_err();
        let s = format!("{err:#}");
        assert!(s.contains("relax_tool_policy"));
    }

    #[test]
    fn validate_allows_shell_with_relax() {
        validate_mcp_serve_allowlist(&["shell".into()], true).unwrap();
    }

    #[test]
    fn validate_http_rejects_public_bind_without_token() {
        let e = validate_mcp_http_bind("0.0.0.0", None).unwrap_err();
        assert!(format!("{e:#}").contains("auth_token"));
    }

    #[test]
    fn validate_http_allows_loopback_without_token() {
        validate_mcp_http_bind("127.0.0.1", None).unwrap();
    }

    #[tokio::test]
    async fn contract_initialize_and_tools_list() {
        let rt = McpServeRuntime {
            exposed: vec![Box::new(ContractPingTool)],
            allowed: HashSet::from(["contract_ping".to_string()]),
            timeout_secs: 5,
        };
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let r1 = rt.handle_json_line(init).await.unwrap();
        assert!(r1.result.is_some());
        let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":null}"#;
        let r2 = rt.handle_json_line(list).await.unwrap();
        let tools = r2.result.unwrap();
        let arr = tools["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "contract_ping");
    }

    #[tokio::test]
    async fn contract_tools_call_ping() {
        let rt = McpServeRuntime {
            exposed: vec![Box::new(ContractPingTool)],
            allowed: HashSet::from(["contract_ping".to_string()]),
            timeout_secs: 5,
        };
        let call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"contract_ping","arguments":{}}}"#;
        let r3 = rt.handle_json_line(call).await.unwrap();
        let res = r3.result.unwrap();
        let text = res["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "pong");
    }

    #[tokio::test]
    async fn http_post_mcp_tools_list() {
        let rt = Arc::new(McpServeRuntime {
            exposed: vec![Box::new(ContractPingTool)],
            allowed: HashSet::from(["contract_ping".to_string()]),
            timeout_secs: 5,
        });
        let app = mcp_http_router(Arc::clone(&rt), None);
        let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":null}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["result"]["tools"][0]["name"], "contract_ping");
    }

    #[tokio::test]
    async fn http_rejects_without_bearer_when_token_set() {
        let rt = Arc::new(McpServeRuntime {
            exposed: vec![Box::new(ContractPingTool)],
            allowed: HashSet::from(["contract_ping".to_string()]),
            timeout_secs: 5,
        });
        let app = mcp_http_router(Arc::clone(&rt), Some("secret".into()));
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":null}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .body(Body::from(body))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_accepts_bearer_when_token_set() {
        let rt = Arc::new(McpServeRuntime {
            exposed: vec![Box::new(ContractPingTool)],
            allowed: HashSet::from(["contract_ping".to_string()]),
            timeout_secs: 5,
        });
        let app = mcp_http_router(Arc::clone(&rt), Some("secret".into()));
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":null}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::AUTHORIZATION, "Bearer secret")
            .body(Body::from(body))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
