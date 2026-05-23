//! HTTP client for the bun-browser daemon (`POST /site/run`).
//!
//! Auth and host resolution mirrors taxonomy-processor's `bun_browser.py`.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_HOST: &str = "http://127.0.0.1:19824";
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;

pub const BUN_BROWSER_HOST_ENV: &str = "BUN_BROWSER_HOST";
pub const BUN_BROWSER_TOKEN_ENV: &str = "BUN_BROWSER_TOKEN";
pub const BUN_BROWSER_TIMEOUT_ENV: &str = "BUN_BROWSER_TIMEOUT";

const DAEMON_CONFIG_REL: &str = ".bun-browser/daemon.json";

#[derive(Debug, Clone)]
pub struct BunBrowserConfig {
    pub host: String,
    pub token: String,
    pub timeout: Duration,
}

impl BunBrowserConfig {
    pub fn resolve(host_override: Option<&str>, timeout_secs: Option<u64>) -> Result<Self> {
        let host = resolve_host(host_override);
        let token = resolve_token()?;
        let timeout = resolve_timeout(timeout_secs);
        Ok(Self {
            host,
            token,
            timeout,
        })
    }
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

fn resolve_token() -> Result<String> {
    if let Ok(raw) = std::env::var(BUN_BROWSER_TOKEN_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
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
        "Missing bun-browser token. Set {BUN_BROWSER_TOKEN_ENV} or ensure {} exists with a token field.",
        daemon_config_path().display()
    )
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
}

pub fn format_site_error(error: &str, hint: Option<&str>) -> String {
    if let Some(h) = hint.filter(|s| !s.is_empty()) {
        format!("{error}: {h}")
    } else {
        error.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct BunBrowserClient {
    config: Option<BunBrowserConfig>,
    host_override: Option<String>,
    timeout_secs: Option<u64>,
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
            http,
        })
    }

    /// Create a client that resolves daemon auth on first use.
    pub fn new_deferred(host_override: Option<String>, timeout_secs: Option<u64>) -> Result<Self> {
        let timeout = resolve_timeout(timeout_secs);
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("build bun-browser HTTP client")?;
        Ok(Self {
            config: None,
            host_override,
            timeout_secs,
            http,
        })
    }

    pub fn from_resolved(host_override: Option<&str>, timeout_secs: Option<u64>) -> Result<Self> {
        Self::new(BunBrowserConfig::resolve(host_override, timeout_secs)?)
    }

    fn ensure_config(&mut self) -> Result<&BunBrowserConfig> {
        if self.config.is_none() {
            self.config = Some(BunBrowserConfig::resolve(
                self.host_override.as_deref(),
                self.timeout_secs,
            )?);
            self.http = Client::builder()
                .timeout(self.config.as_ref().expect("config just set").timeout)
                .build()
                .context("build bun-browser HTTP client")?;
        }
        Ok(self.config.as_ref().expect("config initialized"))
    }

    pub async fn run_site(&self, name: &str, args: Map<String, Value>) -> Result<Value> {
        let config = self.config.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "bun-browser client requires run_site_mut before deferred auth is resolved"
            )
        })?;
        Self::post_site_run(&self.http, config, name, args).await
    }

    pub async fn run_site_mut(&mut self, name: &str, args: Map<String, Value>) -> Result<Value> {
        let config = self.ensure_config()?.clone();
        Self::post_site_run(&self.http, &config, name, args).await
    }

    async fn post_site_run(
        http: &Client,
        config: &BunBrowserConfig,
        name: &str,
        args: Map<String, Value>,
    ) -> Result<Value> {
        let url = format!("{}/site/run", config.host);
        let payload = SiteRunRequest {
            name,
            args: if args.is_empty() { None } else { Some(&args) },
        };

        let response = http
            .post(&url)
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
            });

        if !envelope.success {
            let error = envelope
                .error
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Unknown adapter error".to_string());
            anyhow::bail!(format_site_error(&error, envelope.hint.as_deref()));
        }

        Ok(envelope.data)
    }
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
        assert_eq!(resolve_token().unwrap(), "test-token-abc");
        match orig {
            Some(v) => std::env::set_var(BUN_BROWSER_TOKEN_ENV, v),
            None => std::env::remove_var(BUN_BROWSER_TOKEN_ENV),
        }
    }

    #[test]
    fn resolve_token_missing_errors() {
        let _guard = env_lock();
        let orig = std::env::var(BUN_BROWSER_TOKEN_ENV).ok();
        std::env::remove_var(BUN_BROWSER_TOKEN_ENV);
        let orig_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());
        let err = resolve_token().unwrap_err().to_string();
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
    fn format_site_error_includes_hint() {
        assert_eq!(
            format_site_error("Not logged in", Some("Log in to grok.com")),
            "Not logged in: Log in to grok.com"
        );
    }

    #[test]
    fn format_site_error_without_hint() {
        assert_eq!(format_site_error("HTTP 500", None), "HTTP 500");
    }
}
