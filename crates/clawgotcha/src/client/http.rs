//! HTTP transport for the Clawgotcha API (retries + ETag plumbing).

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, IF_NONE_MATCH};
use reqwest::StatusCode;

use crate::config::ClawgotchaRuntimeConfig;
use crate::error::ClawgotchaError;
use crate::models::domain::{AgentDefinition, CronJobDefinition, SwarmDefaults};
use crate::models::wire::{WireAgent, WireCronJob, WireInstanceRecord, WireSwarmDefaults};
use crate::traits::{
    AgentsDelta, ClawgotchaRegistration, ClawgotchaSyncRead, ClawgotchaWebhooks, CronDelta,
    FetchDelta, HeartbeatPayload, InstanceRegistration,
};

const DEFAULT_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 200;

/// HTTP client implementation with bounded retries and If-None-Match support.
pub struct ClawgotchaHttpAdapter {
    http: reqwest::Client,
    base: String,
}

impl ClawgotchaHttpAdapter {
    /// Build a client using `ClawgotchaRuntimeConfig::base_url` (trimmed, no trailing slash).
    pub fn new(cfg: &ClawgotchaRuntimeConfig) -> Result<Self, ClawgotchaError> {
        let base = cfg.base_url.trim_end_matches('/').to_string();
        if base.is_empty() {
            return Err(ClawgotchaError::Validation("base_url is empty".into()));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ClawgotchaError::Http(e.to_string()))?;
        Ok(Self { http, base })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn get_json_retry<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        etag: Option<&str>,
    ) -> Result<(StatusCode, Option<String>, Option<T>), ClawgotchaError> {
        let mut attempt: u32 = 0;
        loop {
            let mut req = self.http.get(self.url(path));
            if let Some(tag) = etag {
                let mut headers = HeaderMap::new();
                if let Ok(v) = HeaderValue::from_str(tag) {
                    headers.insert(IF_NONE_MATCH, v);
                    req = req.headers(headers);
                }
            }
            let send_result = req.send().await;
            match send_result {
                Ok(resp) => {
                    let status = resp.status();
                    let resp_etag = resp
                        .headers()
                        .get(reqwest::header::ETAG)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned);
                    if status == StatusCode::NOT_MODIFIED {
                        return Ok((status, resp_etag, None));
                    }
                    if status.is_success() {
                        let body = resp.json::<T>().await.map_err(|e| {
                            ClawgotchaError::InvalidResponse(format!("decode {path}: {e}"))
                        })?;
                        return Ok((status, resp_etag, Some(body)));
                    }
                    let body = resp.text().await.unwrap_or_default();
                    if attempt < DEFAULT_RETRIES && status.is_server_error() {
                        tracing::warn!(%status, path, attempt, "clawgotcha GET retry");
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_BASE_MS * (1u64 << attempt),
                        ))
                        .await;
                        attempt += 1;
                        continue;
                    }
                    return Err(ClawgotchaError::HttpStatus {
                        status: status.as_u16(),
                        body,
                    });
                }
                Err(e) => {
                    if attempt < DEFAULT_RETRIES {
                        tracing::warn!(error = %e, path, attempt, "clawgotcha GET transport retry");
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_BASE_MS * (1u64 << attempt),
                        ))
                        .await;
                        attempt += 1;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
    }

    async fn post_empty_retry(&self, path: &str, body: &[u8]) -> Result<(), ClawgotchaError> {
        let mut attempt: u32 = 0;
        loop {
            let send_result = self
                .http
                .post(self.url(path))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_vec())
                .send()
                .await;
            match send_result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(());
                    }
                    let body = resp.text().await.unwrap_or_default();
                    if attempt < DEFAULT_RETRIES && status.is_server_error() {
                        tracing::warn!(%status, path, attempt, "clawgotcha POST retry");
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_BASE_MS * (1u64 << attempt),
                        ))
                        .await;
                        attempt += 1;
                        continue;
                    }
                    return Err(ClawgotchaError::HttpStatus {
                        status: status.as_u16(),
                        body,
                    });
                }
                Err(e) => {
                    if attempt < DEFAULT_RETRIES {
                        tracing::warn!(error = %e, path, attempt, "clawgotcha POST transport retry");
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_BASE_MS * (1u64 << attempt),
                        ))
                        .await;
                        attempt += 1;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct AgentsEnvelope {
    #[serde(default)]
    revision_watermark: u64,
    #[serde(default)]
    agents: Vec<WireAgent>,
}

#[derive(serde::Deserialize)]
struct CronEnvelope {
    #[serde(default)]
    revision_watermark: u64,
    #[serde(default)]
    jobs: Vec<WireCronJob>,
}

#[async_trait]
impl ClawgotchaRegistration for ClawgotchaHttpAdapter {
    async fn register_instance(&self, reg: &InstanceRegistration) -> Result<(), ClawgotchaError> {
        let wire = WireInstanceRecord {
            instance_name: reg.instance.instance_name.clone(),
            callback_url: reg.instance.callback_url.clone(),
        };
        let bytes = serde_json::to_vec(&wire)?;
        self.post_empty_retry("/v1/instances/register", &bytes)
            .await
    }

    async fn send_heartbeat(&self, hb: &HeartbeatPayload) -> Result<(), ClawgotchaError> {
        let body = serde_json::json!({
            "instance_name": hb.instance_name,
            "loaded_agents_count": hb.loaded_agents_count,
            "cron_jobs_count": hb.cron_jobs_count,
        });
        let bytes = serde_json::to_vec(&body)?;
        self.post_empty_retry("/v1/instances/heartbeat", &bytes)
            .await
    }
}

#[async_trait]
impl ClawgotchaSyncRead for ClawgotchaHttpAdapter {
    async fn fetch_agents_since(
        &self,
        revision: Option<u64>,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<AgentsDelta>, Option<String>), ClawgotchaError> {
        let path = match revision {
            Some(r) => format!("/v1/agents?since_revision={r}"),
            None => "/v1/agents".to_string(),
        };
        let (status, out_etag, parsed) = self.get_json_retry::<AgentsEnvelope>(&path, etag).await?;
        if status == StatusCode::NOT_MODIFIED {
            return Ok((FetchDelta::NotModified, out_etag));
        }
        let env = parsed.ok_or_else(|| ClawgotchaError::InvalidResponse("agents body".into()))?;
        let mut agents = Vec::with_capacity(env.agents.len());
        for a in env.agents {
            agents.push(AgentDefinition::try_from(a)?);
        }
        Ok((
            FetchDelta::Modified(AgentsDelta {
                revision_watermark: env.revision_watermark,
                agents,
            }),
            out_etag,
        ))
    }

    async fn fetch_cron_jobs_since(
        &self,
        revision: Option<u64>,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<CronDelta>, Option<String>), ClawgotchaError> {
        let path = match revision {
            Some(r) => format!("/v1/cron?since_revision={r}"),
            None => "/v1/cron".to_string(),
        };
        let (status, out_etag, parsed) = self.get_json_retry::<CronEnvelope>(&path, etag).await?;
        if status == StatusCode::NOT_MODIFIED {
            return Ok((FetchDelta::NotModified, out_etag));
        }
        let env = parsed.ok_or_else(|| ClawgotchaError::InvalidResponse("cron body".into()))?;
        let mut jobs = Vec::with_capacity(env.jobs.len());
        for j in env.jobs {
            jobs.push(CronJobDefinition::try_from(j)?);
        }
        Ok((
            FetchDelta::Modified(CronDelta {
                revision_watermark: env.revision_watermark,
                jobs,
            }),
            out_etag,
        ))
    }

    async fn fetch_swarm_config(
        &self,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<SwarmDefaults>, Option<String>), ClawgotchaError> {
        let (status, out_etag, parsed) = self
            .get_json_retry::<WireSwarmDefaults>("/v1/swarm/config", etag)
            .await?;
        if status == StatusCode::NOT_MODIFIED {
            return Ok((FetchDelta::NotModified, out_etag));
        }
        let w = parsed.ok_or_else(|| ClawgotchaError::InvalidResponse("swarm config".into()))?;
        Ok((FetchDelta::Modified(w.into()), out_etag))
    }
}

#[async_trait]
impl ClawgotchaWebhooks for ClawgotchaHttpAdapter {
    async fn register_webhook(
        &self,
        callback_url: &str,
        event_types: &[&str],
    ) -> Result<(), ClawgotchaError> {
        let body = serde_json::json!({
            "callback_url": callback_url,
            "event_types": event_types,
        });
        let bytes = serde_json::to_vec(&body)?;
        self.post_empty_retry("/v1/webhooks", &bytes).await
    }
}
