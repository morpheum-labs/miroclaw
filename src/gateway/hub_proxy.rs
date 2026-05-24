//! Public hub WebSocket proxy — routes `/ws/chat` to agent worker backends.

use crate::config::registry::AgentRegistry;
use crate::config::Config;
use anyhow::{bail, Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::security::pairing::PairingGuard;
use super::ws::{extract_ws_token, WsQuery, WS_PROTOCOL};

#[derive(Clone)]
pub struct HubState {
    pub registry: AgentRegistry,
    pub home_dir: PathBuf,
    pub hub_config: Arc<Config>,
    pub pairing: Arc<PairingGuard>,
}

#[derive(Debug, Deserialize, Clone)]
struct HubConnectParams {
    #[serde(rename = "type")]
    _msg_type: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    session_key: Option<String>,
    #[serde(default)]
    last_event_seq: Option<u64>,
}

#[derive(Debug, Serialize)]
struct AgentListItem {
    name: String,
    enabled: bool,
    internal_port: u16,
    config_dir: String,
}

#[derive(Debug, Serialize)]
struct AgentsListResponse {
    agents: Vec<AgentListItem>,
    default_agent: String,
}

type BackendCmd = mpsc::UnboundedSender<String>;

/// Run the public hub gateway (proxy WS + agent discovery API).
pub async fn run_hub_gateway(
    host: &str,
    port: u16,
    hub_config: Config,
    registry: AgentRegistry,
    home_dir: PathBuf,
) -> Result<()> {
    let pairing = Arc::new(PairingGuard::new(
        hub_config.gateway.require_pairing,
        &hub_config.gateway.paired_tokens,
    ));
    let state = HubState {
        registry,
        home_dir,
        hub_config: Arc::new(hub_config),
        pairing,
    };

    let app = Router::new()
        .route("/api/agents", get(handle_list_agents))
        .route("/api/health", get(handle_hub_health))
        .route("/ws/chat", get(handle_hub_ws_chat))
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%host, port, "Hub gateway listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn handle_hub_health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "mode": "hub"}))
}

async fn handle_list_agents(State(state): State<HubState>) -> impl IntoResponse {
    let agents = state
        .registry
        .agents
        .iter()
        .map(|a| AgentListItem {
            name: a.name.clone(),
            enabled: a.enabled,
            internal_port: a.internal_port,
            config_dir: a.config_dir.clone(),
        })
        .collect();
    Json(AgentsListResponse {
        agents,
        default_agent: state.registry.default_agent.clone(),
    })
}

pub async fn handle_hub_ws_chat(
    ws: WebSocketUpgrade,
    State(state): State<HubState>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if state.pairing.require_pairing() {
        let token = extract_ws_token(&headers, query.token.as_deref()).unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                "Unauthorized — provide Authorization header, Sec-WebSocket-Protocol bearer, or ?token= query param",
            )
                .into_response();
        }
    }

    let ws = if headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |protos| protos.split(',').any(|p| p.trim() == WS_PROTOCOL))
    {
        ws.protocols([WS_PROTOCOL])
    } else {
        ws
    };

    ws.on_upgrade(move |socket| handle_hub_socket(socket, state, query))
        .into_response()
}

async fn handle_hub_socket(client: WebSocket, state: HubState, query: WsQuery) {
    let (mut client_tx, mut client_rx) = client.split();
    let (to_client_tx, mut to_client_rx) = mpsc::unbounded_channel::<String>();
    let backend_cmd: Arc<Mutex<Option<BackendCmd>>> = Arc::new(Mutex::new(None));
    let mut backend_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut active_agent = state.registry.default_agent.clone();

    let connect_agent = |state: HubState,
                         agent_id: String,
                         params: HubConnectParams,
                         query: WsQuery,
                         backend_cmd: Arc<Mutex<Option<BackendCmd>>>,
                         to_client_tx: mpsc::UnboundedSender<String>| {
        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::unbounded_channel::<String>();
            *backend_cmd.lock().await = Some(tx);
            if let Err(e) =
                run_backend_relay(&state, &agent_id, &params, &query, &mut rx, to_client_tx).await
            {
                tracing::warn!(agent = %agent_id, error = %e, "backend relay ended");
            }
            *backend_cmd.lock().await = None;
        })
    };

    loop {
        tokio::select! {
            msg = client_rx.next() => {
                let Some(msg) = msg else { break };
                let Ok(msg) = msg else { break };
                match msg {
                    Message::Text(text) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            match msg_type {
                                "connect" => {
                                    if let Ok(params) = serde_json::from_value::<HubConnectParams>(v) {
                                        let connect_params = params.clone();
                                        let agent_id = params.agent_id.filter(|s| !s.is_empty());
                                        let Some(id) = agent_id else {
                                            let _ = client_tx.send(Message::Text(
                                                serde_json::json!({
                                                    "type": "error",
                                                    "code": "AGENT_ID_REQUIRED",
                                                    "message": "agent_id is required on hub connect",
                                                })
                                                .to_string()
                                                .into(),
                                            ))
                                            .await;
                                            continue;
                                        };
                                        active_agent = id;
                                        if let Some(task) = backend_task.take() { task.abort(); }
                                        backend_task = Some(connect_agent(
                                            state.clone(), active_agent.clone(), connect_params, query.clone(), Arc::clone(&backend_cmd), to_client_tx.clone(),
                                        ));
                                    }
                                    continue;
                                }
                                "switch_agent" => {
                                    if let Some(id) = v.get("agent_id").and_then(|x| x.as_str()) {
                                        active_agent = id.to_string();
                                        if let Some(task) = backend_task.take() { task.abort(); }
                                        let params = HubConnectParams {
                                            _msg_type: "connect".into(),
                                            agent_id: Some(active_agent.clone()),
                                            session_id: v.get("session_id").and_then(|x| x.as_str()).map(str::to_string),
                                            session_key: None,
                                            last_event_seq: None,
                                        };
                                        backend_task = Some(connect_agent(
                                            state.clone(), active_agent.clone(), params, query.clone(), Arc::clone(&backend_cmd), to_client_tx.clone(),
                                        ));
                                        let _ = client_tx.send(Message::Text(format!(
                                            r#"{{"type":"agent_switched","agent_id":"{}"}}"#,
                                            active_agent
                                        ).into())).await;
                                    }
                                    continue;
                                }
                                "list_agents" => {
                                    let list = state.registry.agents.iter().map(|a| serde_json::json!({
                                        "name": a.name, "enabled": a.enabled, "internal_port": a.internal_port,
                                    })).collect::<Vec<_>>();
                                    let _ = client_tx.send(Message::Text(serde_json::json!({
                                        "type": "agents_list", "agents": list, "default_agent": state.registry.default_agent,
                                    }).to_string().into())).await;
                                    continue;
                                }
                                "list_sessions" => {
                                    let agent_id = v.get("agent_id").and_then(|x| x.as_str()).unwrap_or(&active_agent);
                                    match fetch_worker_sessions(&state, agent_id).await {
                                        Ok(sessions) => {
                                            let _ = client_tx.send(Message::Text(serde_json::json!({
                                                "type": "sessions_list", "agent_id": agent_id, "sessions": sessions,
                                            }).to_string().into())).await;
                                        }
                                        Err(e) => {
                                            let _ = client_tx.send(Message::Text(format!(r#"{{"type":"error","message":"{e}"}}"#).into())).await;
                                        }
                                    }
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        if backend_task.is_none() {
                            let _ = client_tx.send(Message::Text(
                                serde_json::json!({
                                    "type": "error",
                                    "code": "NOT_CONNECTED",
                                    "message": "Send {\"type\":\"connect\",\"agent_id\":\"<name>\"} before messaging",
                                })
                                .to_string()
                                .into(),
                            ))
                            .await;
                            continue;
                        }
                        if let Some(cmd) = backend_cmd.lock().await.as_ref() {
                            let _ = cmd.send(text.to_string());
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            relay = to_client_rx.recv() => {
                if let Some(text) = relay {
                    let _ = client_tx.send(Message::Text(text.into())).await;
                }
            }
        }
    }
    if let Some(task) = backend_task {
        task.abort();
    }
}

async fn run_backend_relay(
    state: &HubState,
    agent_id: &str,
    params: &HubConnectParams,
    query: &WsQuery,
    to_backend_rx: &mut mpsc::UnboundedReceiver<String>,
    to_client_tx: mpsc::UnboundedSender<String>,
) -> Result<()> {
    let entry = state
        .registry
        .get(agent_id)
        .with_context(|| format!("unknown agent_id '{agent_id}'"))?;
    if !entry.enabled {
        bail!("agent '{agent_id}' is disabled");
    }
    let mut url = format!("ws://127.0.0.1:{}/ws/chat", entry.internal_port);
    let session_id = params
        .session_id
        .as_deref()
        .or(query.session_id.as_deref());
    let mut qs = Vec::new();
    if let Some(sid) = session_id {
        qs.push(format!("session_id={}", urlencoding::encode(sid)));
    }
    if let Some(name) = query.name.as_deref() {
        qs.push(format!("name={}", urlencoding::encode(name)));
    }
    if !qs.is_empty() {
        url.push('?');
        url.push_str(&qs.join("&"));
    }

    let (mut backend, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connect worker WS at {url}"))?;

    backend
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({
                "type": "connect",
                "session_id": session_id,
                "session_key": params.session_key,
                "last_event_seq": params.last_event_seq,
            })
            .to_string()
            .into(),
        ))
        .await?;

    let (mut backend_tx, mut backend_rx) = backend.split();

    loop {
        tokio::select! {
            from_client = to_backend_rx.recv() => {
                match from_client {
                    Some(text) => {
                        backend_tx.send(tokio_tungstenite::tungstenite::Message::Text(text.into())).await?;
                    }
                    None => break,
                }
            }
            from_backend = backend_rx.next() => {
                match from_backend {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        let _ = to_client_tx.send(text.to_string());
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
        }
    }
    Ok(())
}

async fn fetch_worker_sessions(state: &HubState, agent_id: &str) -> Result<Vec<serde_json::Value>> {
    let entry = state
        .registry
        .get(agent_id)
        .with_context(|| format!("unknown agent '{agent_id}'"))?;
    let url = format!("http://127.0.0.1:{}/internal/sessions", entry.internal_port);
    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        bail!("worker sessions API returned {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;
    Ok(body
        .get("sessions")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default())
}
