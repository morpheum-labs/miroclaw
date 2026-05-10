//! Per-`session_id` gateway WebSocket workers: own [`crate::agent::Agent`], persist turns,
//! and fan out streamed events to attached clients with bounded replay for reconnect.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot, Mutex};

use super::AppState;
use crate::agent::{TurnEvent, TurnEventSink};
use crate::channels::runtime_slash::SlashRouteSelection;
use crate::channels::session_backend::SessionBackend;
use crate::providers::ChatMessage;

/// Default cap on replayed WS events per gateway session ([`GatewayConfig::ws_event_replay_cap`]).
pub const DEFAULT_WS_EVENT_REPLAY_CAP: usize = 128;

/// Live subscriber queue depth per WebSocket attachment.
const SUBSCRIBER_CHANNEL_CAP: usize = 256;

/// Cloneable context passed into the long-lived session task (avoids cloning full [`AppState`]).
#[derive(Clone)]
pub struct GatewayRunnerContext {
    pub config: Arc<parking_lot::Mutex<crate::config::Config>>,
    pub session_backend: Option<Arc<dyn SessionBackend>>,
    pub hooks: Option<Arc<crate::hooks::HookRunner>>,
    pub gateway_mcp: Option<Arc<crate::tools::GatewayMcpBundle>>,
    pub gateway_chat_routes: Arc<parking_lot::Mutex<HashMap<String, SlashRouteSelection>>>,
    pub event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
}

impl From<&AppState> for GatewayRunnerContext {
    fn from(s: &AppState) -> Self {
        Self {
            config: Arc::clone(&s.config),
            session_backend: s.session_backend.clone(),
            hooks: s.hooks.clone(),
            gateway_mcp: s.gateway_mcp.clone(),
            gateway_chat_routes: Arc::clone(&s.gateway_chat_routes),
            event_tx: s.event_tx.clone(),
        }
    }
}

// ── Session metadata (backend only; no Agent) ───────────────────────────────

/// Load session display metadata for `session_start` without constructing an [`crate::agent::Agent`].
pub fn gateway_ws_session_metadata(
    state: &AppState,
    session_id: &str,
    name_from_query: Option<&str>,
) -> (String, bool, usize, Option<String>) {
    let session_key = crate::agent::session_record::gateway_backend_key(session_id);
    let Some(ref backend) = state.session_backend else {
        return (session_key, false, 0, None);
    };
    let messages = backend.load(&session_key);
    let resumed = !messages.is_empty();
    let message_count = messages.len();
    let mut effective_name = None;
    if let Some(name) = name_from_query.filter(|n| !n.is_empty()) {
        let _ = backend.set_session_name(&session_key, name);
        effective_name = Some(name.to_string());
    }
    if effective_name.is_none() {
        effective_name = backend.get_session_name(&session_key).unwrap_or(None);
    }
    (session_key, resumed, message_count, effective_name)
}

fn hydrate_gateway_ws_agent(
    agent: &mut crate::agent::Agent,
    backend: &dyn SessionBackend,
    session_id: &str,
    name_from_query: Option<&str>,
) -> (String, bool, usize, Option<String>) {
    let session_key = crate::agent::session_record::gateway_backend_key(session_id);
    agent.clear_history();
    agent.set_memory_session_id(Some(session_id.to_string()));
    let messages = backend.load(&session_key);
    let mut resumed = false;
    let mut message_count = 0;
    if !messages.is_empty() {
        message_count = messages.len();
        agent.seed_history(&messages);
        resumed = true;
    }
    let mut effective_name = None;
    if let Some(name) = name_from_query.filter(|n| !n.is_empty()) {
        let _ = backend.set_session_name(&session_key, name);
        effective_name = Some(name.to_string());
    }
    if effective_name.is_none() {
        effective_name = backend.get_session_name(&session_key).unwrap_or(None);
    }
    (session_key, resumed, message_count, effective_name)
}

fn apply_stored_gateway_route_override(
    agent: &mut crate::agent::Agent,
    ctx: &GatewayRunnerContext,
    session_key: &str,
) {
    let sel = ctx.gateway_chat_routes.lock().get(session_key).cloned();
    let Some(sel) = sel else {
        return;
    };
    let cfg = ctx.config.lock().clone();
    if let Err(e) = agent.reset_provider_for_gateway_route(
        &cfg,
        &sel.provider,
        &sel.model,
        sel.api_key.as_deref(),
    ) {
        tracing::warn!(
            error = %e,
            %session_key,
            "Stored gateway route override failed to apply"
        );
    }
}

// ── Replay buffer ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ReplayState {
    pub events: VecDeque<(u64, serde_json::Value)>,
    pub next_seq: u64,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            // Client-visible seq starts at 1 (`last_event_seq: 0` means "replay from start of buffer").
            next_seq: 1,
        }
    }
}

impl ReplayState {
    fn next_sequence(&mut self) -> u64 {
        let s = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        s
    }

    /// Append an event with a new monotonic `seq` field (inserted into the JSON object).
    pub fn push_event(&mut self, cap: usize, mut value: serde_json::Value) -> u64 {
        let seq = self.next_sequence();
        if let Some(obj) = value.as_object_mut() {
            obj.insert("seq".to_string(), seq.into());
        }
        self.events.push_back((seq, value));
        while self.events.len() > cap {
            self.events.pop_front();
        }
        seq
    }

    /// Events strictly after `after_seq`, or all buffered events if `after_seq` is `None`.
    pub fn snapshot_after(&self, after_seq: Option<u64>) -> Vec<serde_json::Value> {
        self.events
            .iter()
            .filter(|(seq, _)| after_seq.map_or(true, |s| *seq > s))
            .map(|(_, v)| v.clone())
            .collect()
    }
}

// ── Runner commands ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum GatewaySessionCmd {
    Shutdown,
    AttachSubscriber {
        after_seq: Option<u64>,
        snapshot: oneshot::Sender<Vec<serde_json::Value>>,
        live: mpsc::Sender<serde_json::Value>,
    },
    UnregisterSubscriber,
    UserMessage {
        content: String,
    },
    ApplySlashEffects {
        clear_chat_session: bool,
        rebind: Option<(String, String, Option<String>)>,
        ack: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Debug)]
pub enum GatewaySessionCreateError {
    MaxSessions { limit: usize },
    AgentInit(anyhow::Error),
}

#[derive(Debug)]
pub struct SessionMeta {
    pub subscribers: u32,
    pub turn_busy: bool,
    pub last_activity: Instant,
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self {
            subscribers: 0,
            turn_busy: false,
            last_activity: Instant::now(),
        }
    }
}

impl SessionMeta {
    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn should_evict(&self, now: Instant, idle_minutes: u32) -> bool {
        idle_minutes > 0
            && self.subscribers == 0
            && !self.turn_busy
            && now.duration_since(self.last_activity)
                >= Duration::from_secs(u64::from(idle_minutes) * 60)
    }
}

/// Handle for a running gateway chat session worker.
pub struct GatewaySessionEntry {
    pub session_key: String,
    pub session_id: String,
    cmd_tx: mpsc::Sender<GatewaySessionCmd>,
    pub meta: Arc<Mutex<SessionMeta>>,
    pub task: tokio::task::JoinHandle<()>,
}

impl GatewaySessionEntry {
    pub async fn attach_subscriber(
        &self,
        after_seq: Option<u64>,
    ) -> Result<(Vec<serde_json::Value>, mpsc::Receiver<serde_json::Value>), &'static str> {
        let (snap_tx, snap_rx) = oneshot::channel();
        let (live_tx, live_rx) = mpsc::channel(SUBSCRIBER_CHANNEL_CAP);
        self.cmd_tx
            .send(GatewaySessionCmd::AttachSubscriber {
                after_seq,
                snapshot: snap_tx,
                live: live_tx,
            })
            .await
            .map_err(|_| "session runner command channel closed")?;
        let snapshot = snap_rx
            .await
            .map_err(|_| "session runner dropped snapshot")?;
        Ok((snapshot, live_rx))
    }

    pub async fn send_user_message(&self, content: String) -> Result<(), &'static str> {
        self.cmd_tx
            .send(GatewaySessionCmd::UserMessage { content })
            .await
            .map_err(|_| "session runner command channel closed")
    }

    pub async fn apply_slash_effects(
        &self,
        clear_chat_session: bool,
        rebind: Option<(String, String, Option<String>)>,
    ) -> Result<Result<(), String>, &'static str> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.cmd_tx
            .send(GatewaySessionCmd::ApplySlashEffects {
                clear_chat_session,
                rebind,
                ack: ack_tx,
            })
            .await
            .map_err(|_| "session runner command channel closed")?;
        ack_rx.await.map_err(|_| "session runner dropped slash ack")
    }

    pub async fn unregister_subscriber(&self) -> Result<(), &'static str> {
        self.cmd_tx
            .send(GatewaySessionCmd::UnregisterSubscriber)
            .await
            .map_err(|_| "session runner command channel closed")
    }
}

async fn shutdown_session_entry(entry: Arc<GatewaySessionEntry>) {
    let _ = entry.cmd_tx.send(GatewaySessionCmd::Shutdown).await;
    match Arc::try_unwrap(entry) {
        Ok(inner) => {
            let _ = tokio::time::timeout(Duration::from_secs(30), inner.task).await;
        }
        Err(arc) => {
            arc.task.abort();
        }
    }
}

async fn emit_ws_event_async(
    replay: &Arc<Mutex<ReplayState>>,
    replay_cap: usize,
    subscribers: &mut Vec<mpsc::Sender<serde_json::Value>>,
    value: serde_json::Value,
) {
    let seq = {
        let mut g = replay.lock().await;
        g.push_event(replay_cap, value)
    };
    let payload = {
        let g = replay.lock().await;
        g.events
            .iter()
            .find(|(s, _)| *s == seq)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(
                || serde_json::json!({"type":"error","message":"replay miss","code":"INTERNAL"}),
            )
    };
    subscribers.retain(|tx| match tx.try_send(payload.clone()) {
        Err(mpsc::error::TrySendError::Closed(_)) => false,
        Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => true,
    });
}

fn gateway_semantic_fast_path_reply(route: &str) -> Option<&'static str> {
    match route {
        "tool_help" => Some(
            "(fast path) Tools include shell, file read/write, memory, cron, MCP, browser, and workspace skills. Ask the model to run a specific tool by name or describe what you want done.",
        ),
        "mcp_help" => Some(
            "(fast path) MCP connects this agent to external tool servers. Configure MCP servers in your workspace config; enabled tools appear in the agent tool list when connected.",
        ),
        "cron_help" => Some(
            "(fast path) Scheduled jobs are available via cron-related tools (e.g. list or add jobs). Ask the model to inspect or manage your schedule.",
        ),
        _ => None,
    }
}

async fn try_gateway_semantic_router_fast_path(
    ctx: &GatewayRunnerContext,
    content: &str,
) -> Option<String> {
    let cfg = ctx.config.lock().clone();
    let sr = cfg.gateway.semantic_router;
    if !sr.enabled {
        return None;
    }
    let base = sr.base_url.as_deref()?;
    if !super::semantic_router_client::base_url_is_loopback(base) {
        tracing::warn!(
            target: "gateway_semantic_router",
            "semantic_router.base_url is not loopback; skipping fast path"
        );
        return None;
    }
    let timeout = std::time::Duration::from_millis(sr.timeout_ms.max(1));
    let client = crate::config::build_runtime_proxy_client("gateway.semantic_router");
    let classified =
        match super::semantic_router_client::classify(&client, base, content, timeout).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    target: "gateway_semantic_router",
                    error = %e,
                    "semantic router classify failed; falling back to LLM"
                );
                return None;
            }
        };
    let result = classified?;
    if result.score < sr.min_score {
        tracing::debug!(
            target: "gateway_semantic_router",
            route = %result.route,
            score = result.score,
            min = sr.min_score,
            "semantic router score below min_score"
        );
        return None;
    }
    gateway_semantic_fast_path_reply(&result.route).map(std::string::ToString::to_string)
}

async fn emit_gateway_ws_turn_success(
    ctx: &GatewayRunnerContext,
    session_key: &str,
    replay: &Arc<Mutex<ReplayState>>,
    replay_cap: usize,
    subscribers: &mut Vec<mpsc::Sender<serde_json::Value>>,
    provider_label: &str,
    model_label: &str,
    answer: &str,
    reasoning: &str,
) {
    if let Some(ref backend) = ctx.session_backend {
        let assistant_msg = ChatMessage::assistant(answer);
        let _ = backend.append(session_key, &assistant_msg);
    }

    let reset = serde_json::json!({ "type": "chunk_reset" });
    emit_ws_event_async(replay, replay_cap, subscribers, reset).await;

    let mut done = serde_json::json!({
        "type": "done",
        "full_response": answer,
    });
    if !reasoning.is_empty() {
        done["full_reasoning"] = serde_json::json!(reasoning);
    }
    emit_ws_event_async(replay, replay_cap, subscribers, done).await;

    let _ = ctx.event_tx.send(serde_json::json!({
        "type": "agent_end",
        "provider": provider_label,
        "model": model_label,
    }));
}

async fn process_user_turn(
    ctx: &GatewayRunnerContext,
    agent: &mut crate::agent::Agent,
    session_key: &str,
    content: &str,
    replay: &Arc<Mutex<ReplayState>>,
    replay_cap: usize,
    subscribers: &mut Vec<mpsc::Sender<serde_json::Value>>,
) {
    let provider_label = agent.provider_label_str().to_string();
    let model_label = agent.model_name_str().to_string();

    let _ = ctx.event_tx.send(serde_json::json!({
        "type": "agent_start",
        "provider": provider_label,
        "model": model_label,
    }));

    if let Some(answer) = try_gateway_semantic_router_fast_path(ctx, content).await {
        emit_gateway_ws_turn_success(
            ctx,
            session_key,
            replay,
            replay_cap,
            subscribers,
            &provider_label,
            &model_label,
            &answer,
            "",
        )
        .await;
        return;
    }

    let (event_tx, mut event_rx) = mpsc::channel::<TurnEventSink>(64);
    let content_owned = content.to_string();
    let turn_fut = async { agent.turn_streamed(&content_owned, event_tx).await };

    let replay_cl = Arc::clone(replay);
    let replay_cap_cl = replay_cap;
    let forward_fut = async {
        while let Some(item) = event_rx.recv().await {
            let ws_msg = match item {
                TurnEventSink::DeltaText(delta)
                | TurnEventSink::Emit(TurnEvent::Chunk { delta }) => {
                    serde_json::json!({ "type": "chunk", "content": delta })
                }
                TurnEventSink::Emit(TurnEvent::ReasoningChunk { delta }) => {
                    serde_json::json!({ "type": "reasoning_chunk", "content": delta })
                }
                TurnEventSink::Emit(TurnEvent::ToolCall { name, args }) => {
                    serde_json::json!({ "type": "tool_call", "name": name, "args": args })
                }
                TurnEventSink::Emit(TurnEvent::ToolResult { name, output }) => {
                    serde_json::json!({ "type": "tool_result", "name": name, "output": output })
                }
            };
            emit_ws_event_async(&replay_cl, replay_cap_cl, subscribers, ws_msg).await;
        }
    };

    let (result, ()) = tokio::join!(turn_fut, forward_fut);

    match result {
        Ok(out) => {
            emit_gateway_ws_turn_success(
                ctx,
                session_key,
                replay,
                replay_cap,
                subscribers,
                &provider_label,
                &model_label,
                &out.answer,
                &out.reasoning,
            )
            .await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Agent turn failed");
            let sanitized = crate::providers::sanitize_api_error(&e.to_string());
            let error_code = if sanitized.to_lowercase().contains("api key")
                || sanitized.to_lowercase().contains("authentication")
                || sanitized.to_lowercase().contains("unauthorized")
            {
                "AUTH_ERROR"
            } else if sanitized.to_lowercase().contains("provider")
                || sanitized.to_lowercase().contains("model")
            {
                "PROVIDER_ERROR"
            } else {
                "AGENT_ERROR"
            };
            let err = serde_json::json!({
                "type": "error",
                "message": sanitized,
                "code": error_code,
            });
            emit_ws_event_async(replay, replay_cap, subscribers, err).await;

            let _ = ctx.event_tx.send(serde_json::json!({
                "type": "error",
                "component": "ws_chat",
                "message": sanitized,
            }));
        }
    }
}

async fn gateway_session_runner_loop(
    mut agent: crate::agent::Agent,
    mut cmd_rx: mpsc::Receiver<GatewaySessionCmd>,
    ctx: GatewayRunnerContext,
    session_key: String,
    replay: Arc<Mutex<ReplayState>>,
    replay_cap: usize,
    meta: Arc<Mutex<SessionMeta>>,
) {
    let mut subscribers: Vec<mpsc::Sender<serde_json::Value>> = Vec::new();

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            GatewaySessionCmd::Shutdown => break,
            GatewaySessionCmd::AttachSubscriber {
                after_seq,
                snapshot,
                live,
            } => {
                let snap = {
                    let g = replay.lock().await;
                    g.snapshot_after(after_seq)
                };
                let _ = snapshot.send(snap);
                {
                    let mut m = meta.lock().await;
                    m.subscribers = m.subscribers.saturating_add(1);
                    m.touch();
                }
                subscribers.push(live);
            }
            GatewaySessionCmd::UnregisterSubscriber => {
                let mut m = meta.lock().await;
                m.subscribers = m.subscribers.saturating_sub(1);
                m.touch();
            }
            GatewaySessionCmd::ApplySlashEffects {
                clear_chat_session,
                rebind,
                ack,
            } => {
                let mut result_ok = Ok(());
                if clear_chat_session {
                    if let Some(ref backend) = ctx.session_backend {
                        let _ = backend.delete_session(&session_key);
                    }
                    agent.clear_history();
                    if let Some(ref backend) = ctx.session_backend {
                        let messages = backend.load(&session_key);
                        agent.seed_history(&messages);
                    } else {
                        agent.seed_history(&[]);
                    }
                }
                if let Some((ref p, ref mname, ref ak)) = rebind {
                    let cfg = ctx.config.lock().clone();
                    if let Err(e) =
                        agent.reset_provider_for_gateway_route(&cfg, p, mname, ak.as_deref())
                    {
                        result_ok = Err(crate::providers::sanitize_api_error(&e.to_string()));
                    }
                }
                {
                    let mut m = meta.lock().await;
                    m.touch();
                }
                let _ = ack.send(result_ok);
            }
            GatewaySessionCmd::UserMessage { content } => {
                {
                    let mut m = meta.lock().await;
                    m.turn_busy = true;
                    m.touch();
                }
                if let Some(ref backend) = ctx.session_backend {
                    let user_msg = ChatMessage::user(&content);
                    let _ = backend.append(&session_key, &user_msg);
                }
                process_user_turn(
                    &ctx,
                    &mut agent,
                    &session_key,
                    &content,
                    &replay,
                    replay_cap,
                    &mut subscribers,
                )
                .await;
                {
                    let mut m = meta.lock().await;
                    m.turn_busy = false;
                    m.touch();
                }
            }
        }
    }

    tracing::debug!(%session_key, "gateway session runner exited");
}

async fn spawn_gateway_session_runner_inner(
    ctx: GatewayRunnerContext,
    session_key: String,
    session_id: String,
    session_name: Option<String>,
) -> Result<GatewaySessionEntry, GatewaySessionCreateError> {
    let config = ctx.config.lock().clone();
    let mut agent = crate::agent::Agent::from_config_with_hooks(
        &config,
        ctx.hooks.clone(),
        ctx.gateway_mcp.clone(),
    )
    .await
    .map_err(GatewaySessionCreateError::AgentInit)?;

    if let Some(ref backend) = ctx.session_backend {
        hydrate_gateway_ws_agent(
            &mut agent,
            backend.as_ref(),
            &session_id,
            session_name.as_deref(),
        );
    } else {
        agent.set_memory_session_id(Some(session_id.clone()));
    }
    apply_stored_gateway_route_override(&mut agent, &ctx, &session_key);

    let replay_cap = {
        let c = ctx.config.lock().gateway.ws_event_replay_cap;
        if c == 0 {
            DEFAULT_WS_EVENT_REPLAY_CAP
        } else {
            c
        }
    };

    let replay = Arc::new(Mutex::new(ReplayState::default()));
    let meta = Arc::new(Mutex::new(SessionMeta::default()));
    let (cmd_tx, cmd_rx) = mpsc::channel::<GatewaySessionCmd>(32);

    let ctx_task = ctx.clone();
    let sk = session_key.clone();
    let meta_task = Arc::clone(&meta);
    let replay_task = Arc::clone(&replay);
    #[allow(clippy::large_futures)]
    let task = tokio::spawn(async move {
        gateway_session_runner_loop(
            agent,
            cmd_rx,
            ctx_task,
            sk,
            replay_task,
            replay_cap,
            meta_task,
        )
        .await;
    });

    Ok(GatewaySessionEntry {
        session_key,
        session_id,
        cmd_tx,
        meta,
        task,
    })
}

/// Obtain or spawn the runner for `session_key`.
pub async fn get_or_create_gateway_session(
    state: &AppState,
    session_key: &str,
    session_id: &str,
    session_name: Option<String>,
) -> Result<Arc<GatewaySessionEntry>, GatewaySessionCreateError> {
    let guard = state.gateway_sessions.lock().await;
    if let Some(e) = guard.get(session_key) {
        return Ok(Arc::clone(e));
    }
    let max = state.config.lock().gateway.ws_runner_max_sessions;
    if max > 0 && guard.len() >= max {
        return Err(GatewaySessionCreateError::MaxSessions { limit: max });
    }
    drop(guard);

    let ctx = GatewayRunnerContext::from(state);
    let fresh_entry = spawn_gateway_session_runner_inner(
        ctx,
        session_key.to_string(),
        session_id.to_string(),
        session_name,
    )
    .await?;

    let mut guard = state.gateway_sessions.lock().await;
    if let Some(e) = guard.get(session_key) {
        let _ = fresh_entry.cmd_tx.send(GatewaySessionCmd::Shutdown).await;
        fresh_entry.task.abort();
        return Ok(Arc::clone(e));
    }
    if max > 0 && guard.len() >= max {
        let _ = fresh_entry.cmd_tx.send(GatewaySessionCmd::Shutdown).await;
        fresh_entry.task.abort();
        return Err(GatewaySessionCreateError::MaxSessions { limit: max });
    }
    let arc = Arc::new(fresh_entry);
    guard.insert(session_key.to_string(), Arc::clone(&arc));
    Ok(arc)
}

/// Remove and shut down a session runner (e.g. `fresh=` query or session switch).
pub async fn remove_session_runner(state: &AppState, session_key: &str) {
    let entry = state.gateway_sessions.lock().await.remove(session_key);
    if let Some(e) = entry {
        shutdown_session_entry(e).await;
    }
}

/// Drain all session runners (gateway shutdown).
#[allow(clippy::implicit_hasher)]
pub async fn shutdown_all_gateway_sessions(
    sessions: &Arc<Mutex<HashMap<String, Arc<GatewaySessionEntry>>>>,
) {
    let entries: Vec<Arc<GatewaySessionEntry>> =
        sessions.lock().await.drain().map(|(_, v)| v).collect();
    for e in entries {
        shutdown_session_entry(e).await;
    }
}

/// Periodic idle eviction (call from a spawned interval task).
#[allow(clippy::implicit_hasher)]
pub async fn gateway_sessions_eviction_tick(
    sessions: &Arc<Mutex<HashMap<String, Arc<GatewaySessionEntry>>>>,
    config: &Arc<parking_lot::Mutex<crate::config::Config>>,
) {
    let idle = config.lock().gateway.ws_runner_idle_minutes;
    if idle == 0 {
        return;
    }
    let now = Instant::now();
    let keys: Vec<String> = {
        let g = sessions.lock().await;
        g.iter()
            .filter_map(|(k, e)| {
                e.meta
                    .try_lock()
                    .ok()
                    .filter(|m| m.should_evict(now, idle))
                    .map(|_| k.clone())
            })
            .collect()
    };
    for k in keys {
        let entry = sessions.lock().await.remove(&k);
        if let Some(e) = entry {
            shutdown_session_entry(e).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_push_respects_cap_and_seq_monotonic() {
        let mut r = ReplayState::default();
        let cap = 3;
        r.push_event(cap, serde_json::json!({"type":"chunk","content":"a"}));
        r.push_event(cap, serde_json::json!({"type":"chunk","content":"b"}));
        r.push_event(cap, serde_json::json!({"type":"chunk","content":"c"}));
        r.push_event(cap, serde_json::json!({"type":"chunk","content":"d"}));
        assert_eq!(r.events.len(), cap);
        let joined: String = r
            .events
            .iter()
            .filter_map(|(_, v)| v.get("content").and_then(|x| x.as_str()))
            .collect();
        assert_eq!(joined, "bcd");
        let seqs: Vec<u64> = r
            .events
            .iter()
            .filter_map(|(_, v)| v.get("seq").and_then(|x| x.as_u64()))
            .collect();
        assert_eq!(seqs, vec![2, 3, 4]);
    }

    #[test]
    fn snapshot_after_none_replays_all() {
        let mut r = ReplayState::default();
        r.push_event(10, serde_json::json!({"type":"a"}));
        r.push_event(10, serde_json::json!({"type":"b"}));
        let all = r.snapshot_after(None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0]["type"], "a");
        let tail = r.snapshot_after(Some(0));
        assert_eq!(tail.len(), 2);
        let tail2 = r.snapshot_after(Some(1));
        assert_eq!(tail2.len(), 1);
        assert_eq!(tail2[0]["type"], "b"); // seq 2 > 1
    }
}
