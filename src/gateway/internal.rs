//! Internal worker API (localhost-only): session discovery for hub attach/monitor.

use axum::{extract::State, response::Json};
use serde::Serialize;

use super::AppState;

#[derive(Serialize)]
pub struct InternalSessionRow {
    pub session_key: String,
    pub kind: String,
    pub in_progress: bool,
}

#[derive(Serialize)]
pub struct InternalSessionsResponse {
    pub sessions: Vec<InternalSessionRow>,
}

/// GET /internal/sessions — gateway WS runners + active channel turns.
pub async fn handle_internal_sessions(
    State(state): State<AppState>,
) -> Json<InternalSessionsResponse> {
    let mut sessions = Vec::new();

    if let Ok(guard) = state.gateway_sessions.try_lock() {
        for key in guard.keys() {
            sessions.push(InternalSessionRow {
                session_key: key.clone(),
                kind: "gateway".into(),
                in_progress: true,
            });
        }
    }

    for key in crate::channels::session_runner::active_channel_session_keys() {
        sessions.push(InternalSessionRow {
            session_key: key,
            kind: "channel".into(),
            in_progress: true,
        });
    }

    Json(InternalSessionsResponse { sessions })
}
