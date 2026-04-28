//! Minimal Clawgotcha-compatible HTTP API for local testing with Miroclaw.
//!
//! Contract: see `docs/reference/integrations/clawgotcha-api-contract.md`.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

#[derive(Clone, Default)]
struct InstanceRow {
    callback_url: Option<String>,
    last_heartbeat_at: Option<chrono::DateTime<Utc>>,
    loaded_agents_count: usize,
    cron_jobs_count: usize,
}

struct ControlPlane {
    instances: HashMap<String, InstanceRow>,
    revision_watermark: u64,
    agents: Vec<Value>,
    cron_jobs: Vec<Value>,
    swarm_config: Value,
    etag_agents: String,
    etag_cron: String,
    etag_swarm: String,
}

impl Default for ControlPlane {
    fn default() -> Self {
        Self {
            instances: HashMap::new(),
            revision_watermark: 0,
            agents: Vec::new(),
            cron_jobs: Vec::new(),
            swarm_config: json!({
                "default_provider": null,
                "default_model": null,
                "current_revision": 0u64,
            }),
            etag_agents: "\"agents-0\"".to_string(),
            etag_cron: "\"cron-0\"".to_string(),
            etag_swarm: "\"swarm-0\"".to_string(),
        }
    }
}

impl ControlPlane {
    fn bump_revision(&mut self) -> u64 {
        self.revision_watermark = self.revision_watermark.saturating_add(1);
        let wm = self.revision_watermark;
        self.refresh_etags(wm);
        wm
    }

    fn refresh_etags(&mut self, wm: u64) {
        self.etag_agents = format!("\"agents-{wm}\"");
        self.etag_cron = format!("\"cron-{wm}\"");
        self.etag_swarm = format!("\"swarm-{wm}\"");
    }
}

#[derive(Deserialize)]
struct RegisterBody {
    instance_name: String,
    #[serde(default)]
    callback_url: Option<String>,
}

#[derive(Deserialize)]
struct HeartbeatBody {
    instance_name: String,
    loaded_agents_count: usize,
    cron_jobs_count: usize,
}

#[derive(Deserialize)]
struct SinceQuery {
    since_revision: Option<u64>,
}

async fn post_register(
    State(st): State<Arc<RwLock<ControlPlane>>>,
    Json(body): Json<RegisterBody>,
) -> impl IntoResponse {
    let name = body.instance_name.trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "instance_name required").into_response();
    }
    let mut g = st.write().await;
    g.bump_revision();
    g.instances.insert(
        name,
        InstanceRow {
            callback_url: body.callback_url,
            ..Default::default()
        },
    );
    StatusCode::OK.into_response()
}

async fn post_heartbeat(
    State(st): State<Arc<RwLock<ControlPlane>>>,
    Json(body): Json<HeartbeatBody>,
) -> impl IntoResponse {
    let name = body.instance_name.trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "instance_name required").into_response();
    }
    let mut g = st.write().await;
    if let Some(row) = g.instances.get_mut(&name) {
        row.last_heartbeat_at = Some(Utc::now());
        row.loaded_agents_count = body.loaded_agents_count;
        row.cron_jobs_count = body.cron_jobs_count;
        StatusCode::OK.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown instance"})),
        )
            .into_response()
    }
}

async fn get_instances(State(st): State<Arc<RwLock<ControlPlane>>>) -> impl IntoResponse {
    let g = st.read().await;
    let now = Utc::now();
    let instances: Vec<Value> = g
        .instances
        .iter()
        .map(|(name, row)| {
            let online = row
                .last_heartbeat_at
                .is_some_and(|t| (now - t).num_seconds() < 120);
            json!({
                "instance_name": name,
                "callback_url": row.callback_url,
                "online": online,
                "last_heartbeat_at": row.last_heartbeat_at.map(|t| t.to_rfc3339()),
                "loaded_agents_count": row.loaded_agents_count,
                "cron_jobs_count": row.cron_jobs_count,
            })
        })
        .collect();
    Json(json!({ "instances": instances }))
}

async fn get_instance(
    State(st): State<Arc<RwLock<ControlPlane>>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let g = st.read().await;
    let Some(row) = g.instances.get(&name) else {
        return (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response();
    };
    let now = Utc::now();
    let online = row
        .last_heartbeat_at
        .is_some_and(|t| (now - t).num_seconds() < 120);
    Json(json!({
        "instance_name": name,
        "callback_url": row.callback_url,
        "online": online,
        "last_heartbeat_at": row.last_heartbeat_at.map(|t| t.to_rfc3339()),
        "loaded_agents_count": row.loaded_agents_count,
        "cron_jobs_count": row.cron_jobs_count,
    }))
    .into_response()
}

fn not_modified_if_match(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.trim_matches('"'))
}

async fn get_agents(
    State(st): State<Arc<RwLock<ControlPlane>>>,
    Query(q): Query<SinceQuery>,
    headers: HeaderMap,
) -> Response {
    let g = st.read().await;
    let wm = g.revision_watermark;
    if not_modified_if_match(&headers, &g.etag_agents) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &g.etag_agents)
            .body(Body::empty())
            .unwrap();
    }
    let _ = q.since_revision;
    let body = json!({
        "revision_watermark": wm,
        "agents": g.agents,
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ETAG, &g.etag_agents)
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn get_cron(State(st): State<Arc<RwLock<ControlPlane>>>, headers: HeaderMap) -> Response {
    let g = st.read().await;
    let wm = g.revision_watermark;
    if not_modified_if_match(&headers, &g.etag_cron) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &g.etag_cron)
            .body(Body::empty())
            .unwrap();
    }
    let body = json!({
        "revision_watermark": wm,
        "jobs": g.cron_jobs,
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ETAG, &g.etag_cron)
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn get_swarm(State(st): State<Arc<RwLock<ControlPlane>>>, headers: HeaderMap) -> Response {
    let g = st.read().await;
    if not_modified_if_match(&headers, &g.etag_swarm) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &g.etag_swarm)
            .body(Body::empty())
            .unwrap();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ETAG, &g.etag_swarm)
        .body(Body::from(g.swarm_config.to_string()))
        .unwrap()
}

async fn post_webhooks(Json(_body): Json<Value>) -> StatusCode {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut cp = ControlPlane::default();
    cp.bump_revision();
    let state = Arc::new(RwLock::new(cp));

    let app = Router::new()
        .route("/v1/instances/register", post(post_register))
        .route("/v1/instances/heartbeat", post(post_heartbeat))
        .route("/v1/instances", get(get_instances))
        .route("/v1/instances/{name}", get(get_instance))
        .route("/v1/agents", get(get_agents))
        .route("/v1/cron", get(get_cron))
        .route("/v1/swarm/config", get(get_swarm))
        .route("/v1/webhooks", post(post_webhooks))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let port: u16 = env::var("CLAWGOTCHA_SERVER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9847);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("clawgotcha-server listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
