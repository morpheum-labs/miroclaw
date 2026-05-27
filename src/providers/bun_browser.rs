//! HTTP client for the bun-browser daemon (`POST /site/run`, `POST /command`).
//!
//! Auth and host resolution mirrors taxonomy-processor's `bun_browser.py`.

use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_HOST: &str = "http://127.0.0.1:19824";
/// Upper bound for expert/heavy grok modes (40m + buffer).
pub const DEFAULT_TIMEOUT_SECS: u64 = 40 * 60 + 300;

pub const BUN_BROWSER_HOST_ENV: &str = "BUN_BROWSER_HOST";
pub const BUN_BROWSER_TOKEN_ENV: &str = "BUN_BROWSER_TOKEN";
pub const BUN_BROWSER_TIMEOUT_ENV: &str = "BUN_BROWSER_TIMEOUT";

const DAEMON_CONFIG_REL: &str = ".bun-browser/daemon.json";
const GROK_ORIGIN: &str = "https://grok.com";

#[derive(Debug, Clone)]
pub struct BunBrowserConfig {
    pub host: String,
    pub token: String,
    pub timeout: Duration,
}

impl BunBrowserConfig {
    pub fn resolve(
        host_override: Option<&str>,
        timeout_secs: Option<u64>,
        toml_token: Option<&str>,
    ) -> Result<Self> {
        let host = resolve_host(host_override);
        let token = resolve_bun_browser_token(toml_token, None, false, false)?;
        let timeout = resolve_timeout(timeout_secs);
        Ok(Self {
            host,
            token,
            timeout,
        })
    }
}

/// Returned when the bun-browser daemon rejects the bearer token.
#[derive(Debug, Clone)]
pub struct BunBrowserAuthError {
    pub status: u16,
}

impl fmt::Display for BunBrowserAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bun-browser authentication failed (HTTP {})",
            self.status
        )
    }
}

impl std::error::Error for BunBrowserAuthError {}

#[derive(Debug, Clone, Default)]
pub struct RunSiteOptions {
    pub tab_id: Option<String>,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct TabInfo {
    pub tab_id: String,
    pub url: String,
}

fn resolve_host(host_override: Option<&str>) -> String {
    if let Some(raw) = host_override.map(str::trim).filter(|s| !s.is_empty()) {
        return raw.trim_end_matches('/').to_string();
    }
    std::env::var(BUN_BROWSER_HOST_ENV)
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_HOST.to_string())
}

/// Resolve bun-browser bearer token: env → TOML (memory or disk) → `daemon.json`.
pub fn resolve_bun_browser_token(
    toml_token: Option<&str>,
    config_path: Option<&Path>,
    secrets_encrypt: bool,
    refresh_from_disk: bool,
) -> Result<String> {
    if let Ok(raw) = std::env::var(BUN_BROWSER_TOKEN_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let config_token = if refresh_from_disk {
        if let Some(path) = config_path {
            crate::config::read_grok_browser_token_from_config(path, secrets_encrypt)?
        } else {
            None
        }
    } else {
        toml_token
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    if let Some(token) = config_token {
        return Ok(token);
    }

    let path = daemon_config_path();
    if path.is_file() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("read bun-browser daemon config at {}", path.display()))?;
        let parsed: DaemonConfigFile = serde_json::from_str(&raw)
            .with_context(|| format!("parse bun-browser daemon config at {}", path.display()))?;
        let trimmed = parsed.token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    anyhow::bail!(
        "Missing bun-browser token. Set {BUN_BROWSER_TOKEN_ENV}, [grok_browser].token in config.toml, or ensure {} exists with a token field.",
        daemon_config_path().display()
    )
}

pub fn is_auth_error(status: StatusCode, body: &str) -> bool {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return true;
    }
    let lower = body.to_lowercase();
    ["unauthorized", "invalid token", "forbidden"]
        .iter()
        .any(|hint| lower.contains(hint))
}

fn resolve_timeout(timeout_secs: Option<u64>) -> Duration {
    if let Some(secs) = timeout_secs.filter(|s| *s > 0) {
        return Duration::from_secs(secs);
    }
    if let Ok(raw) = std::env::var(BUN_BROWSER_TIMEOUT_ENV) {
        if let Ok(secs) = raw.trim().parse::<u64>() {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
    }
    Duration::from_secs(DEFAULT_TIMEOUT_SECS)
}

fn daemon_config_path() -> PathBuf {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().join(DAEMON_CONFIG_REL))
        .unwrap_or_else(|| PathBuf::from(DAEMON_CONFIG_REL))
}

#[derive(Debug, Deserialize)]
struct DaemonConfigFile {
    token: String,
}

#[derive(Debug, Serialize)]
struct SiteRunRequest<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<&'a Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tabId: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct SiteRunEnvelope {
    success: bool,
    #[serde(default)]
    data: Value,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    tab: Option<Value>,
}

#[derive(Debug, Serialize)]
struct CommandRequest {
    id: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommandResponse {
    success: bool,
    #[serde(default)]
    data: Option<CommandData>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommandData {
    #[serde(default)]
    tabs: Vec<CommandTab>,
    #[serde(default)]
    tabId: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CommandTab {
    #[serde(default)]
    tabId: Option<Value>,
    #[serde(default)]
    url: Option<String>,
}

pub fn format_site_error(error: &str, hint: Option<&str>) -> String {
    if let Some(h) = hint.filter(|s| !s.is_empty()) {
        format!("{error}: {h}")
    } else {
        error.to_string()
    }
}

fn value_to_tab_id(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    value.as_u64().map(|n| n.to_string())
}

#[derive(Debug, Clone)]
pub struct BunBrowserClient {
    config: Option<BunBrowserConfig>,
    host_override: Option<String>,
    timeout_secs: Option<u64>,
    config_path: Option<PathBuf>,
    secrets_encrypt: bool,
    toml_token: Option<String>,
    refresh_token_from_disk: bool,
    http: Client,
}

impl BunBrowserClient {
    pub fn new(config: BunBrowserConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .context("build bun-browser HTTP client")?;
        Ok(Self {
            config: Some(config),
            host_override: None,
            timeout_secs: None,
            config_path: None,
            secrets_encrypt: false,
            toml_token: None,
            refresh_token_from_disk: false,
            http,
        })
    }

    /// Create a client that resolves daemon auth on first use.
    pub fn new_deferred(
        host_override: Option<String>,
        timeout_secs: Option<u64>,
        config_path: Option<PathBuf>,
        secrets_encrypt: bool,
        toml_token: Option<String>,
    ) -> Result<Self> {
        let timeout = resolve_timeout(timeout_secs);
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("build bun-browser HTTP client")?;
        Ok(Self {
            config: None,
            host_override,
            timeout_secs,
            config_path,
            secrets_encrypt,
            toml_token,
            refresh_token_from_disk: false,
            http,
        })
    }

    pub fn from_resolved(host_override: Option<&str>, timeout_secs: Option<u64>) -> Result<Self> {
        Self::new(BunBrowserConfig::resolve(
            host_override,
            timeout_secs,
            None,
        )?)
    }

    pub fn invalidate_auth(&mut self) {
        self.config = None;
        self.refresh_token_from_disk = true;
    }

    fn ensure_config(&mut self) -> Result<BunBrowserConfig> {
        if self.config.is_none() {
            let token = resolve_bun_browser_token(
                self.toml_token.as_deref(),
                self.config_path.as_deref(),
                self.secrets_encrypt,
                self.refresh_token_from_disk,
            )?;
            self.refresh_token_from_disk = false;
            let host = resolve_host(self.host_override.as_deref());
            let timeout = resolve_timeout(self.timeout_secs);
            self.config = Some(BunBrowserConfig {
                host,
                token,
                timeout,
            });
            self.http = Client::builder()
                .timeout(timeout)
                .build()
                .context("build bun-browser HTTP client")?;
        }
        Ok(self.config.as_ref().expect("config initialized").clone())
    }

    pub async fn run_site(&self, name: &str, args: Map<String, Value>) -> Result<Value> {
        let config = self.config.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "bun-browser client requires run_site_mut before deferred auth is resolved"
            )
        })?;
        Self::post_site_run(&self.http, config, name, args, RunSiteOptions::default()).await
    }

    pub async fn run_site_mut(&mut self, name: &str, args: Map<String, Value>) -> Result<Value> {
        let mut retried = false;
        loop {
            let config = self.ensure_config()?;
            match Self::post_site_run(
                &self.http,
                &config,
                name,
                args.clone(),
                RunSiteOptions::default(),
            )
            .await
            {
                Ok(value) => return Ok(value),
                Err(err) if !retried && err.downcast_ref::<BunBrowserAuthError>().is_some() => {
                    retried = true;
                    self.invalidate_auth();
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub async fn run_site_with_options(
        &mut self,
        name: &str,
        args: Map<String, Value>,
        options: RunSiteOptions,
    ) -> Result<Value> {
        let mut retried = false;
        loop {
            let config = self.ensure_config()?;
            match Self::post_site_run(&self.http, &config, name, args.clone(), options.clone())
                .await
            {
                Ok(value) => return Ok(value),
                Err(err) if !retried && err.downcast_ref::<BunBrowserAuthError>().is_some() => {
                    retried = true;
                    self.invalidate_auth();
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub async fn tab_list(&mut self) -> Result<Vec<TabInfo>> {
        let mut retried = false;
        loop {
            let config = self.ensure_config()?;
            match Self::post_command(
                &self.http,
                &config,
                CommandRequest {
                    id: uuid::Uuid::new_v4().to_string(),
                    action: "tab_list".into(),
                    url: None,
                },
                None,
            )
            .await
            {
                Ok(response) => {
                    let tabs = response.data.map(|d| d.tabs).unwrap_or_default();
                    return Ok(tabs
                        .into_iter()
                        .filter_map(|tab| {
                            let tab_id = tab.tabId.as_ref().and_then(value_to_tab_id)?;
                            Some(TabInfo {
                                tab_id,
                                url: tab.url.unwrap_or_default(),
                            })
                        })
                        .collect());
                }
                Err(err) if !retried && err.downcast_ref::<BunBrowserAuthError>().is_some() => {
                    retried = true;
                    self.invalidate_auth();
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub async fn tab_new(&mut self, url: &str) -> Result<String> {
        let mut retried = false;
        loop {
            let config = self.ensure_config()?;
            match Self::post_command(
                &self.http,
                &config,
                CommandRequest {
                    id: uuid::Uuid::new_v4().to_string(),
                    action: "tab_new".into(),
                    url: Some(url.to_string()),
                },
                None,
            )
            .await
            {
                Ok(response) => {
                    return response
                        .data
                        .and_then(|d| d.tabId)
                        .and_then(|v| value_to_tab_id(&v))
                        .ok_or_else(|| anyhow::anyhow!("tab_new returned no tabId"));
                }
                Err(err) if !retried && err.downcast_ref::<BunBrowserAuthError>().is_some() => {
                    retried = true;
                    self.invalidate_auth();
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub async fn ensure_grok_tab(&mut self, preferred: Option<&str>) -> Result<String> {
        if let Some(tab_id) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
            return Ok(tab_id.to_string());
        }
        let tabs = self.tab_list().await?;
        if let Some(tab) = tabs.iter().find(|t| tab_matches_grok(&t.url)) {
            return Ok(tab.tab_id.clone());
        }
        self.tab_new(GROK_ORIGIN).await
    }

    async fn post_site_run(
        http: &Client,
        config: &BunBrowserConfig,
        name: &str,
        args: Map<String, Value>,
        options: RunSiteOptions,
    ) -> Result<Value> {
        let url = format!("{}/site/run", config.host);
        let tab_ref = options.tab_id.as_deref();
        let payload = SiteRunRequest {
            name,
            args: if args.is_empty() { None } else { Some(&args) },
            tabId: tab_ref,
        };

        let timeout = options.timeout.unwrap_or(config.timeout);
        let response = http
            .post(&url)
            .timeout(timeout)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", config.token),
            )
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("bun-browser connection failed for site adapter '{name}'"))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("read bun-browser response body")?;

        if is_auth_error(status, &body_text) {
            return Err(BunBrowserAuthError {
                status: status.as_u16(),
            }
            .into());
        }

        let envelope: SiteRunEnvelope =
            serde_json::from_str(&body_text).unwrap_or(SiteRunEnvelope {
                success: status.is_success(),
                data: Value::Null,
                error: if status.is_success() {
                    None
                } else {
                    Some(format!("HTTP {}", status.as_u16()))
                },
                hint: Some(body_text.chars().take(200).collect()),
                tab: None,
            });

        if !envelope.success {
            let error = envelope
                .error
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Unknown adapter error".to_string());
            let combined = format_site_error(&error, envelope.hint.as_deref());
            if is_auth_error(status, &combined) {
                return Err(BunBrowserAuthError {
                    status: status.as_u16(),
                }
                .into());
            }
            anyhow::bail!(combined);
        }

        let mut data = envelope.data;
        if data.is_object() {
            if let Some(obj) = data.as_object_mut() {
                if !obj.contains_key("tab") {
                    if let Some(tab) = envelope.tab {
                        obj.insert("tab".into(), tab);
                    }
                }
            }
        }
        Ok(data)
    }

    async fn post_command(
        http: &Client,
        config: &BunBrowserConfig,
        request: CommandRequest,
        timeout: Option<Duration>,
    ) -> Result<CommandResponse> {
        let url = format!("{}/command", config.host);
        let response = http
            .post(&url)
            .timeout(timeout.unwrap_or(config.timeout))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", config.token),
            )
            .json(&request)
            .send()
            .await
            .with_context(|| format!("bun-browser command '{}' failed", request.action))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("read bun-browser command response")?;

        if is_auth_error(status, &body_text) {
            return Err(BunBrowserAuthError {
                status: status.as_u16(),
            }
            .into());
        }

        let parsed: CommandResponse = serde_json::from_str(&body_text).unwrap_or(CommandResponse {
            success: status.is_success(),
            data: None,
            error: if status.is_success() {
                None
            } else {
                Some(format!("HTTP {}", status.as_u16()))
            },
        });
        if !parsed.success {
            let error = parsed
                .error
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Unknown command error".to_string());
            if is_auth_error(status, &error) {
                return Err(BunBrowserAuthError {
                    status: status.as_u16(),
                }
                .into());
            }
            anyhow::bail!(error);
        }
        Ok(parsed)
    }
}

fn tab_matches_grok(url: &str) -> bool {
    url.contains("grok.com")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn resolve_host_prefers_override() {
        let host = resolve_host(Some("http://localhost:9999/"));
        assert_eq!(host, "http://localhost:9999");
    }

    #[test]
    fn resolve_host_from_env() {
        let _guard = env_lock();
        let orig = std::env::var(BUN_BROWSER_HOST_ENV).ok();
        std::env::set_var(BUN_BROWSER_HOST_ENV, "http://127.0.0.1:5555");
        assert_eq!(resolve_host(None), "http://127.0.0.1:5555");
        match orig {
            Some(v) => std::env::set_var(BUN_BROWSER_HOST_ENV, v),
            None => std::env::remove_var(BUN_BROWSER_HOST_ENV),
        }
    }

    #[test]
    fn resolve_token_from_env() {
        let _guard = env_lock();
        let orig = std::env::var(BUN_BROWSER_TOKEN_ENV).ok();
        std::env::set_var(BUN_BROWSER_TOKEN_ENV, "test-token-abc");
        assert_eq!(
            resolve_bun_browser_token(None, None, false, false).unwrap(),
            "test-token-abc"
        );
        match orig {
            Some(v) => std::env::set_var(BUN_BROWSER_TOKEN_ENV, v),
            None => std::env::remove_var(BUN_BROWSER_TOKEN_ENV),
        }
    }

    #[test]
    fn resolve_token_from_toml_before_daemon_json() {
        let _guard = env_lock();
        let orig = std::env::var(BUN_BROWSER_TOKEN_ENV).ok();
        std::env::remove_var(BUN_BROWSER_TOKEN_ENV);
        let orig_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());
        assert_eq!(
            resolve_bun_browser_token(Some("toml-token"), None, false, false).unwrap(),
            "toml-token"
        );
        if let Some(v) = orig {
            std::env::set_var(BUN_BROWSER_TOKEN_ENV, v);
        }
        match orig_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn resolve_token_from_config_file_on_disk_refresh() {
        let _guard = env_lock();
        let orig = std::env::var(BUN_BROWSER_TOKEN_ENV).ok();
        std::env::remove_var(BUN_BROWSER_TOKEN_ENV);
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "[grok_browser]\ntoken = \"disk-token\"\n").unwrap();
        assert_eq!(
            resolve_bun_browser_token(None, Some(&config_path), false, true).unwrap(),
            "disk-token"
        );
        if let Some(v) = orig {
            std::env::set_var(BUN_BROWSER_TOKEN_ENV, v);
        }
    }

    #[test]
    fn resolve_token_accepts_api_alias_in_config_file() {
        let _guard = env_lock();
        std::env::remove_var(BUN_BROWSER_TOKEN_ENV);
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "[grok_browser]\napi = \"legacy-token\"\n").unwrap();
        assert_eq!(
            resolve_bun_browser_token(None, Some(&config_path), false, true).unwrap(),
            "legacy-token"
        );
    }

    #[test]
    fn resolve_token_missing_errors() {
        let _guard = env_lock();
        let orig = std::env::var(BUN_BROWSER_TOKEN_ENV).ok();
        std::env::remove_var(BUN_BROWSER_TOKEN_ENV);
        let orig_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());
        let err = resolve_bun_browser_token(None, None, false, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Missing bun-browser token"));
        if let Some(v) = orig {
            std::env::set_var(BUN_BROWSER_TOKEN_ENV, v);
        }
        match orig_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn is_auth_error_detects_status_and_body() {
        assert!(is_auth_error(StatusCode::UNAUTHORIZED, ""));
        assert!(is_auth_error(StatusCode::FORBIDDEN, ""));
        assert!(is_auth_error(StatusCode::OK, "Invalid token for daemon"));
        assert!(!is_auth_error(
            StatusCode::OK,
            "timeout waiting for adapter"
        ));
    }

    #[test]
    fn format_site_error_includes_hint() {
        assert_eq!(
            format_site_error("Not logged in", Some("Log in to grok.com")),
            "Not logged in: Log in to grok.com"
        );
    }

    #[test]
    fn tab_matches_grok_origin() {
        assert!(tab_matches_grok("https://grok.com/c/abc"));
        assert!(!tab_matches_grok("https://example.com"));
    }

    #[tokio::test]
    async fn auth_failure_invalidates_and_rereads_disk_token() {
        let _guard = env_lock();
        std::env::remove_var(BUN_BROWSER_TOKEN_ENV);

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            "[grok_browser]\ntoken = \"first-token\"\nhost = \"http://127.0.0.1:1\"\n",
        )
        .unwrap();

        let server = wiremock::MockServer::start().await;
        let host = server.uri();

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/site/run"))
            .respond_with(wiremock::ResponseTemplate::new(401).set_body_string("unauthorized"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        std::fs::write(
            &config_path,
            format!("[grok_browser]\ntoken = \"second-token\"\nhost = \"{host}\"\n"),
        )
        .unwrap();

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/site/run"))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer second-token",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"success": true, "data": {"ok": true}})),
            )
            .mount(&server)
            .await;

        let mut client = BunBrowserClient::new_deferred(
            Some(host.clone()),
            Some(5),
            Some(config_path.clone()),
            false,
            Some("first-token".into()),
        )
        .unwrap();

        let result = client
            .run_site_mut("grok/modes", Map::new())
            .await
            .expect("retry after auth failure");
        assert_eq!(result.get("ok").and_then(|v| v.as_bool()), Some(true));
    }
}
