//! WebSocket agent chat handler.
//!
//! Connect: `ws://host:port/ws/chat?session_id=ID&name=My+Session`
//!
//! Protocol:
//! ```text
//! Server -> Client: {"type":"session_start","session_id":"...","name":"...","resumed":true,"message_count":42}
//! Client -> Server: {"type":"message","content":"Hello"}
//! Server -> Client: {"type":"chunk","content":"..."}   // Visible assistant text deltas + legacy DeltaText progress
//! Server -> Client: {"type":"reasoning_chunk","content":"..."}   // Thinking/reasoning stream when the provider separates it
//! Server -> Client: {"type":"tool_call","name":"shell","args":{...}}
//! Server -> Client: {"type":"tool_result","name":"shell","output":"..."}
//! Server -> Client: {"type":"chunk_reset"}
//! Server -> Client: {"type":"done","full_response":"...","full_reasoning":"..."}   // full_reasoning omitted when empty
//! ```
//!
//! Streamed frames may include `"seq": <u64>` for ordering; clients can reconnect with
//! `{"type":"connect","last_event_seq":N,...}` to replay buffered events with `seq > N`.
//!
//! Query params:
//! - `session_id` — resume or create a session (default: new UUID)
//! - `name` — optional human-readable label for the session
//! - `token` — bearer auth token (alternative to Authorization header)
//! - `fresh` — when `true` / `1` / `yes` (case-insensitive), clear persisted gateway history and
//!   any stored route override for this `session_id` before hydrating the agent

use std::sync::Arc;

use super::session_runner;
use super::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tracing::debug;

/// Optional connection parameters sent as the first WebSocket message.
///
/// If the first message after upgrade is `{"type":"connect",...}`, these
/// parameters are extracted and an acknowledgement is sent back. Old clients
/// that send `{"type":"message",...}` as the first frame still work — the
/// message is processed normally (backward-compatible).
#[derive(Debug, Deserialize)]
struct ConnectParams {
    #[serde(rename = "type")]
    msg_type: String,
    /// Client-chosen session ID for memory persistence
    #[serde(default)]
    session_id: Option<String>,
    /// Device name for device registry tracking
    #[serde(default)]
    device_name: Option<String>,
    /// Client capabilities
    #[serde(default)]
    capabilities: Vec<String>,
    /// Replay gateway stream events strictly after this sequence (optional).
    #[serde(default)]
    last_event_seq: Option<u64>,
}

/// The sub-protocol we support for the chat WebSocket.
const WS_PROTOCOL: &str = "zeroclaw.v1";

/// Prefix used in `Sec-WebSocket-Protocol` to carry a bearer token.
const BEARER_SUBPROTO_PREFIX: &str = "bearer.";

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
    pub session_id: Option<String>,
    /// Optional human-readable name for the session.
    pub name: Option<String>,
    /// Fresh navigation / explicit reset (`true`, `1`, `yes`, case-insensitive).
    #[serde(default)]
    pub fresh: Option<String>,
}

fn parse_ws_fresh_query(raw: Option<&str>) -> bool {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    s.eq_ignore_ascii_case("1") || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes")
}

/// Extract a bearer token from WebSocket-compatible sources.
///
/// Precedence (first non-empty wins):
/// 1. `Authorization: Bearer <token>` header
/// 2. `Sec-WebSocket-Protocol: bearer.<token>` subprotocol
/// 3. `?token=<token>` query parameter
///
/// Browsers cannot set custom headers on `new WebSocket(url)`, so the query
/// parameter and subprotocol paths are required for browser-based clients.
fn extract_ws_token<'a>(headers: &'a HeaderMap, query_token: Option<&'a str>) -> Option<&'a str> {
    // 1. Authorization header
    if let Some(t) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
    {
        if !t.is_empty() {
            return Some(t);
        }
    }

    // 2. Sec-WebSocket-Protocol: bearer.<token>
    if let Some(t) = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .and_then(|protos| {
            protos
                .split(',')
                .map(|p| p.trim())
                .find_map(|p| p.strip_prefix(BEARER_SUBPROTO_PREFIX))
        })
    {
        if !t.is_empty() {
            return Some(t);
        }
    }

    // 3. ?token= query parameter
    if let Some(t) = query_token {
        if !t.is_empty() {
            return Some(t);
        }
    }

    None
}

/// GET /ws/chat — WebSocket upgrade for agent chat
pub async fn handle_ws_chat(
    State(state): State<AppState>,
    Query(params): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Auth: check header, subprotocol, then query param (precedence order)
    if state.pairing.require_pairing() {
        let token = extract_ws_token(&headers, params.token.as_deref()).unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                "Unauthorized — provide Authorization header, Sec-WebSocket-Protocol bearer, or ?token= query param",
            )
                .into_response();
        }
    }

    // Echo Sec-WebSocket-Protocol if the client requests our sub-protocol.
    let ws = if headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |protos| {
            protos.split(',').any(|p| p.trim() == WS_PROTOCOL)
        }) {
        ws.protocols([WS_PROTOCOL])
    } else {
        ws
    };

    let session_id = params.session_id;
    let session_name = params.name;
    let nav_fresh = parse_ws_fresh_query(params.fresh.as_deref());
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id, session_name, nav_fresh))
        .into_response()
}

async fn send_gateway_slash_done(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    reply: &str,
) {
    let reset = serde_json::json!({ "type": "chunk_reset" });
    let _ = sender.send(Message::Text(reset.to_string().into())).await;
    let done = serde_json::json!({
        "type": "done",
        "full_response": reply,
    });
    let _ = sender.send(Message::Text(done.to_string().into())).await;
}

async fn handle_gateway_chat_slash_via_runner(
    state: &AppState,
    session_key: &str,
    content: &str,
    entry: &Arc<session_runner::GatewaySessionEntry>,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let Some(res) = super::chat_slash::handle_gateway_ws_slash(state, session_key, content).await
    else {
        return false;
    };

    let mut reply = res.reply;
    match entry
        .apply_slash_effects(res.clear_chat_session, res.rebind.clone())
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(provider_err)) => {
            reply = format!("{reply}\n\n⚠️ Failed to apply route to agent: {provider_err}");
        }
        Err(_) => {
            reply = format!("{reply}\n\n⚠️ Session runner unavailable");
        }
    }

    send_gateway_slash_done(sender, &reply).await;
    true
}

async fn handle_ws_text_message(
    state: &AppState,
    session_key: &str,
    entry: &Arc<session_runner::GatewaySessionEntry>,
    text: &str,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) {
    let parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            let err = serde_json::json!({
                "type": "error",
                "message": format!("Invalid JSON: {}", e),
                "code": "INVALID_JSON"
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;
            return;
        }
    };

    let msg_type = parsed["type"].as_str().unwrap_or("");
    if msg_type != "message" {
        let err = serde_json::json!({
            "type": "error",
            "message": format!(
                "Unsupported message type \"{msg_type}\". Send {{\"type\":\"message\",\"content\":\"your text\"}}"
            ),
            "code": "UNKNOWN_MESSAGE_TYPE"
        });
        let _ = sender.send(Message::Text(err.to_string().into())).await;
        return;
    }

    let content = parsed["content"].as_str().unwrap_or("").to_string();
    if content.is_empty() {
        let err = serde_json::json!({
            "type": "error",
            "message": "Message content cannot be empty",
            "code": "EMPTY_CONTENT"
        });
        let _ = sender.send(Message::Text(err.to_string().into())).await;
        return;
    }

    if handle_gateway_chat_slash_via_runner(state, session_key, &content, entry, sender).await {
        return;
    }

    if entry.send_user_message(content).await.is_err() {
        let err = serde_json::json!({
            "type": "error",
            "message": "Session runner unavailable",
            "code": "RUNNER_GONE"
        });
        let _ = sender.send(Message::Text(err.to_string().into())).await;
    }
}

async fn send_ws_session_start(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    session_id: &str,
    resumed: bool,
    message_count: usize,
    effective_name: Option<&str>,
) {
    let mut session_start = serde_json::json!({
        "type": "session_start",
        "session_id": session_id,
        "resumed": resumed,
        "message_count": message_count,
    });
    if let Some(name) = effective_name {
        session_start["name"] = serde_json::Value::String(name.to_string());
    }
    let _ = sender
        .send(Message::Text(session_start.to_string().into()))
        .await;
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    session_id: Option<String>,
    session_name: Option<String>,
    nav_fresh: bool,
) {
    let (mut sender, mut receiver) = socket.split();

    let mut session_id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut last_event_seq: Option<u64> = None;

    if nav_fresh {
        let session_key_preview = crate::agent::session_record::gateway_backend_key(&session_id);
        session_runner::remove_session_runner(&state, &session_key_preview).await;
        state
            .gateway_chat_routes
            .lock()
            .remove(&session_key_preview);
        if let Some(ref backend) = state.session_backend {
            let _ = backend.delete_session(&session_key_preview);
        }
        tracing::info!(
            session_id = %session_id,
            "gateway ws chat: fresh query honored; cleared persisted session and route overrides"
        );
    }

    let mut first_msg_fallback: Option<String> = None;
    let mut connect_handshake = false;

    if let Some(first) = receiver.next().await {
        match first {
            Ok(Message::Text(text)) => {
                if let Ok(cp) = serde_json::from_str::<ConnectParams>(&text) {
                    if cp.msg_type == "connect" {
                        connect_handshake = true;
                        last_event_seq = cp.last_event_seq;
                        debug!(
                            session_id = ?cp.session_id,
                            device_name = ?cp.device_name,
                            capabilities = ?cp.capabilities,
                            last_event_seq = ?last_event_seq,
                            "WebSocket connect params received"
                        );
                        if let Some(sid) = cp
                            .session_id
                            .as_ref()
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                        {
                            session_id = sid.to_string();
                        }
                    } else {
                        first_msg_fallback = Some(text.to_string());
                    }
                } else {
                    first_msg_fallback = Some(text.to_string());
                }
            }
            Ok(Message::Close(_)) | Err(_) => return,
            _ => {}
        }
    } else {
        return;
    }

    let (session_key, resumed, message_count, effective_name) =
        session_runner::gateway_ws_session_metadata(&state, &session_id, session_name.as_deref());

    send_ws_session_start(
        &mut sender,
        &session_id,
        resumed,
        message_count,
        effective_name.as_deref(),
    )
    .await;

    if connect_handshake {
        let ack = serde_json::json!({
            "type": "connected",
            "message": "Connection established"
        });
        let _ = sender.send(Message::Text(ack.to_string().into())).await;
    }

    let entry = match session_runner::get_or_create_gateway_session(
        &state,
        &session_key,
        &session_id,
        session_name.clone(),
    )
    .await
    {
        Ok(e) => e,
        Err(session_runner::GatewaySessionCreateError::MaxSessions { limit }) => {
            let err = serde_json::json!({
                "type": "error",
                "message": format!("Too many concurrent gateway chat sessions (limit {limit})"),
                "code": "MAX_WS_SESSIONS",
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;
            return;
        }
        Err(session_runner::GatewaySessionCreateError::AgentInit(e)) => {
            tracing::error!(error = %e, "Agent initialization failed");
            let err = serde_json::json!({
                "type": "error",
                "message": format!("Failed to initialise agent: {e}"),
                "code": "AGENT_INIT_FAILED"
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;
            let _ = sender
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011,
                    reason: axum::extract::ws::Utf8Bytes::from_static(
                        "Agent initialization failed",
                    ),
                })))
                .await;
            return;
        }
    };

    let (snapshot, mut live_rx) = match entry.attach_subscriber(last_event_seq).await {
        Ok(x) => x,
        Err(_) => {
            let err = serde_json::json!({
                "type": "error",
                "message": "Failed to attach to session runner",
                "code": "RUNNER_ATTACH_FAILED",
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;
            return;
        }
    };

    for frame in snapshot {
        if sender
            .send(Message::Text(frame.to_string().into()))
            .await
            .is_err()
        {
            let _ = entry.unregister_subscriber().await;
            return;
        }
    }

    if let Some(text) = first_msg_fallback {
        handle_ws_text_message(&state, &session_key, &entry, &text, &mut sender).await;
    }

    loop {
        tokio::select! {
            frame_opt = live_rx.recv() => {
                match frame_opt {
                    Some(frame) => {
                        if sender.send(Message::Text(frame.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_ws_text_message(&state, &session_key, &entry, &text, &mut sender)
                            .await;
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    let _ = entry.unregister_subscriber().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn extract_ws_token_from_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer zc_test123".parse().unwrap());
        assert_eq!(extract_ws_token(&headers, None), Some("zc_test123"));
    }

    #[test]
    fn extract_ws_token_from_subprotocol() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "sec-websocket-protocol",
            "zeroclaw.v1, bearer.zc_sub456".parse().unwrap(),
        );
        assert_eq!(extract_ws_token(&headers, None), Some("zc_sub456"));
    }

    #[test]
    fn extract_ws_token_from_query_param() {
        let headers = HeaderMap::new();
        assert_eq!(
            extract_ws_token(&headers, Some("zc_query789")),
            Some("zc_query789")
        );
    }

    #[test]
    fn extract_ws_token_precedence_header_over_subprotocol() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer zc_header".parse().unwrap());
        headers.insert("sec-websocket-protocol", "bearer.zc_sub".parse().unwrap());
        assert_eq!(
            extract_ws_token(&headers, Some("zc_query")),
            Some("zc_header")
        );
    }

    #[test]
    fn extract_ws_token_precedence_subprotocol_over_query() {
        let mut headers = HeaderMap::new();
        headers.insert("sec-websocket-protocol", "bearer.zc_sub".parse().unwrap());
        assert_eq!(extract_ws_token(&headers, Some("zc_query")), Some("zc_sub"));
    }

    #[test]
    fn extract_ws_token_returns_none_when_empty() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ws_token(&headers, None), None);
    }

    #[test]
    fn extract_ws_token_skips_empty_header_value() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());
        assert_eq!(
            extract_ws_token(&headers, Some("zc_fallback")),
            Some("zc_fallback")
        );
    }

    #[test]
    fn extract_ws_token_skips_empty_query_param() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ws_token(&headers, Some("")), None);
    }

    #[test]
    fn gateway_ws_slash_parsing_matches_runtime_slash() {
        use crate::channels::runtime_slash::{parse_gateway_ws_slash, ParsedRuntimeSlash};
        assert_eq!(
            parse_gateway_ws_slash("  /new  "),
            Some(ParsedRuntimeSlash::NewSession)
        );
        assert_eq!(
            parse_gateway_ws_slash("/fresh-session"),
            Some(ParsedRuntimeSlash::NewSession)
        );
        assert_eq!(
            parse_gateway_ws_slash("/models"),
            Some(ParsedRuntimeSlash::ShowProviders)
        );
    }

    #[test]
    fn parse_ws_fresh_query_accepts_common_truthy_strings() {
        assert!(parse_ws_fresh_query(Some("1")));
        assert!(parse_ws_fresh_query(Some("true")));
        assert!(parse_ws_fresh_query(Some("TRUE")));
        assert!(parse_ws_fresh_query(Some("yes")));
        assert!(!parse_ws_fresh_query(None));
        assert!(!parse_ws_fresh_query(Some("")));
        assert!(!parse_ws_fresh_query(Some("0")));
        assert!(!parse_ws_fresh_query(Some("false")));
    }

    #[test]
    fn extract_ws_token_subprotocol_with_multiple_entries() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "sec-websocket-protocol",
            "zeroclaw.v1, bearer.zc_tok, other".parse().unwrap(),
        );
        assert_eq!(extract_ws_token(&headers, None), Some("zc_tok"));
    }
}
