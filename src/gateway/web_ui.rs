//! Optional on-disk dashboard (`[webui].external_path`).
//!
//! When `[webui].external_path` points at a valid Vite `dist/` directory, static files are read
//! from disk.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use parking_lot::RwLock;
use serde::Serialize;

use super::AppState;
use crate::config::Config;

/// Optional manifest inside an external `dist/` folder. When present, `schema` should be `1`.
pub const WEBUI_MANIFEST: &str = "zeroclaw-webui.json";

#[derive(Debug, Clone, Serialize)]
pub struct WebUiStatus {
    pub source: &'static str,
    pub external_path: Option<String>,
    pub path_prefix_rewrite: bool,
}

#[derive(Clone)]
pub struct WebUiServeState {
    inner: Arc<WebUiInner>,
}

struct WebUiInner {
    active: RwLock<WebUiActive>,
}

#[derive(Clone)]
enum WebUiActive {
    /// Dashboard intentionally off (`[webui].disabled` / `MIROCLAW_WEBUI_DISABLED`).
    Disabled,
    External {
        root_display: PathBuf,
        root_canonical: PathBuf,
    },
}

impl WebUiServeState {
    /// Build-time + runtime validation. Fails when the dashboard is not disabled and there is
    /// no usable `[webui].external_path`.
    pub fn bootstrap(config: &Config) -> anyhow::Result<Self> {
        let active = resolve_initial_active(config)?;
        log_startup(&active, config.gateway.path_prefix.as_deref().unwrap_or(""));
        Ok(Self {
            inner: Arc::new(WebUiInner {
                active: RwLock::new(active),
            }),
        })
    }

    /// Test helper: external path from `config`, or a minimal temp `dist/` so gateway tests can
    /// construct [`AppState`] without bundling the full dashboard.
    pub fn for_tests(config: &Config) -> Self {
        match Self::bootstrap(config) {
            Ok(s) => s,
            Err(_) => {
                let dir = tempfile::tempdir().expect("tempdir for web UI test dist");
                std::fs::write(
                    dir.path().join("index.html"),
                    "<!doctype html><title>t</title>",
                )
                .expect("write minimal index.html for tests");
                let mut cfg = config.clone();
                cfg.webui.external_path = dir.path().display().to_string();
                let s = Self::bootstrap(&cfg)
                    .unwrap_or_else(|e| panic!("web UI bootstrap in tests (temp dist): {e}"));
                std::mem::forget(dir);
                s
            }
        }
    }

    pub fn status_json(&self, path_prefix: &str) -> WebUiStatus {
        let active = self.inner.active.read().clone();
        let path_prefix_rewrite = !path_prefix.is_empty();
        match &active {
            WebUiActive::Disabled => WebUiStatus {
                source: "disabled",
                external_path: None,
                path_prefix_rewrite,
            },
            WebUiActive::External { root_display, .. } => WebUiStatus {
                source: "external",
                external_path: Some(root_display.display().to_string()),
                path_prefix_rewrite,
            },
        }
    }

    /// Re-resolve `[webui].external_path` from the given config (e.g. after `PUT /api/config`
    /// or `POST /api/webui/reload`).
    pub fn reload_from_config(&self, config: &Config) {
        let active = match try_resolve_active(config) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("WebUI reload: {e} — keeping previous static file source");
                return;
            }
        };
        log_startup(&active, config.gateway.path_prefix.as_deref().unwrap_or(""));
        *self.inner.active.write() = active;
    }

    fn active(&self) -> WebUiActive {
        self.inner.active.read().clone()
    }
}

pub fn format_slash_status(state: &AppState) -> String {
    let s = state.web_ui.status_json(state.path_prefix.as_str());
    let mut out = String::new();
    let _ = writeln!(
        &mut out,
        "WebUI source: {} (path-prefix rewrite: {})\n",
        s.source,
        if s.path_prefix_rewrite { "on" } else { "off" }
    );
    if let Some(p) = &s.external_path {
        let _ = writeln!(&mut out, "External root: {p}");
    }
    let _ = writeln!(
        &mut out,
        "Reload: POST /api/webui/reload (or change config and save)."
    );
    out
}

fn log_startup(active: &WebUiActive, path_prefix: &str) {
    let rewrite = if path_prefix.is_empty() {
        "path-prefix rewriting off"
    } else {
        "path-prefix rewriting on"
    };
    match active {
        WebUiActive::Disabled => {
            tracing::info!("WebUI: disabled — API-only gateway ({rewrite})");
        }
        WebUiActive::External { root_display, .. } => {
            tracing::info!(
                "WebUI source: external path {} ({rewrite})",
                root_display.display()
            );
        }
    }
}

fn resolve_initial_active(config: &Config) -> anyhow::Result<WebUiActive> {
    try_resolve_active(config).map_err(|e| anyhow::anyhow!(e))
}

fn try_resolve_active(config: &Config) -> Result<WebUiActive, String> {
    if config.webui.disabled {
        return Ok(WebUiActive::Disabled);
    }

    let raw = config.webui.external_path.trim();
    if raw.is_empty() {
        return Err(
            "[webui].external_path is unset. Set it to a built `web/dist` directory (with index.html), \
             or set [webui].disabled / MIROCLAW_WEBUI_DISABLED."
                .into(),
        );
    }

    let candidate = resolve_external_path(raw, &config.workspace_dir);
    match validate_external_root(&candidate) {
        Ok((display, canonical)) => Ok(WebUiActive::External {
            root_display: display,
            root_canonical: canonical,
        }),
        Err(e) => Err(format!("Invalid [webui].external_path {raw:?}: {e}")),
    }
}

fn resolve_external_path(raw: &str, workspace_dir: &Path) -> PathBuf {
    let expanded = shellexpand::tilde(raw.trim()).to_string();
    let p = Path::new(&expanded);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let ws = workspace_dir.join(p);
    if ws.exists() {
        return ws;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(p))
        .unwrap_or_else(|_| ws)
}

fn validate_external_root(path: &Path) -> Result<(PathBuf, PathBuf), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    let index = path.join("index.html");
    if !index.is_file() {
        return Err(format!("{} has no index.html", path.display()));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let display = path.to_path_buf();

    let manifest = canonical.join(WEBUI_MANIFEST);
    if manifest.is_file() {
        let txt = std::fs::read_to_string(&manifest)
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        let v: serde_json::Value = serde_json::from_str(&txt)
            .map_err(|e| format!("{}: invalid JSON ({e})", manifest.display()))?;
        let schema = v
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if schema != 1 {
            tracing::warn!(
                "WebUI manifest {} has schema {schema} (expected 1); continuing anyway",
                manifest.display()
            );
        }
    }

    Ok((display, canonical))
}

/// Join `rel` under `root_canonical` with no `..` escape. Returns absolute file path.
fn safe_file_path(root_canonical: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return None;
    }
    let mut out = PathBuf::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(x) => out.push(x),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    let full = root_canonical.join(out);
    let full_canon = full.canonicalize().ok()?;
    if full_canon.starts_with(root_canonical) {
        Some(full_canon)
    } else {
        None
    }
}

fn apply_index_transform(html: &str, path_prefix: &str) -> String {
    if path_prefix.is_empty() {
        return html.to_string();
    }
    let json_pfx = serde_json::to_string(path_prefix).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!("<script>window.__MIROCLAW_BASE__={json_pfx};</script>");
    html.replace("/_app/", &format!("{path_prefix}/_app/"))
        .replacen("<head>", &format!("<head>{script}"), 1)
}

fn response_bytes(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    let cache = if path.contains("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        bytes,
    )
        .into_response()
}

fn webui_disabled_gateway_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "Web dashboard disabled ([webui].disabled or MIROCLAW_WEBUI_DISABLED).",
    )
        .into_response()
}

fn webui_external_unavailable_response(reason: &str) -> Response {
    tracing::warn!("WebUI: {reason}");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        format!("Web dashboard unavailable ({reason})."),
    )
        .into_response()
}

pub async fn handle_static(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri
        .path()
        .strip_prefix("/_app/")
        .unwrap_or(uri.path())
        .trim_start_matches('/');

    match state.web_ui.active() {
        WebUiActive::Disabled => webui_disabled_gateway_response(),
        WebUiActive::External { root_canonical, .. } => {
            if !root_canonical.is_dir() || !root_canonical.join("index.html").is_file() {
                return webui_external_unavailable_response("external root missing");
            }
            let rel = path;
            let file_path = if rel.is_empty() {
                root_canonical.join("index.html")
            } else {
                match safe_file_path(&root_canonical, rel) {
                    Some(p) => p,
                    None => return (StatusCode::NOT_FOUND, "Not found").into_response(),
                }
            };
            if file_path.is_dir() {
                return (StatusCode::NOT_FOUND, "Not found").into_response();
            }
            let mime_path = if rel.is_empty() { "index.html" } else { rel };
            if file_path.file_name().and_then(|n| n.to_str()) == Some("index.html") {
                match tokio::fs::read_to_string(&file_path).await {
                    Ok(html) => {
                        let body = apply_index_transform(&html, state.path_prefix.as_str());
                        return (
                            StatusCode::OK,
                            [
                                (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
                                (header::CACHE_CONTROL, "no-cache".to_string()),
                            ],
                            body,
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return webui_external_unavailable_response(&format!(
                            "index read failed: {e}"
                        ));
                    }
                }
            }
            match tokio::fs::read(&file_path).await {
                Ok(bytes) => response_bytes(mime_path, bytes),
                Err(e) => webui_external_unavailable_response(&format!("read failed: {e}")),
            }
        }
    }
}

pub async fn handle_spa_fallback(State(state): State<AppState>) -> Response {
    let path_prefix = state.path_prefix.as_str();

    match state.web_ui.active() {
        WebUiActive::Disabled => webui_disabled_gateway_response(),
        WebUiActive::External { root_canonical, .. } => {
            let index = root_canonical.join("index.html");
            if !root_canonical.is_dir() || !index.is_file() {
                return webui_external_unavailable_response("SPA fallback: external index missing");
            }
            match tokio::fs::read_to_string(&index).await {
                Ok(html) => {
                    let html = apply_index_transform(&html, path_prefix);
                    (
                        StatusCode::OK,
                        [
                            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
                            (header::CACHE_CONTROL, "no-cache".to_string()),
                        ],
                        html,
                    )
                        .into_response()
                }
                Err(e) => webui_external_unavailable_response(&format!("SPA read failed: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_file_rejects_parent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        std::fs::write(tmp.path().join("x.txt"), "ok").unwrap();
        assert!(safe_file_path(&root, "../Cargo.toml").is_none());
        assert!(safe_file_path(&root, "x.txt").is_some());
    }

    #[test]
    fn index_transform_inserts_base() {
        let h = "<head></head><script src=\"/_app/assets/a.js\">";
        let out = apply_index_transform(h, "/zc");
        assert!(out.contains("window.__MIROCLAW_BASE__=\"/zc\""));
        assert!(out.contains("/zc/_app/assets/a.js"));
    }
}
