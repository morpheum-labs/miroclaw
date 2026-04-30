//! HTTP transport for the Clawgotcha API (retries + ETag plumbing).
//!
//! Wire contract follows the agentbook clawgotcha OpenAPI (`RegisterInstanceRequest`, `HeartbeatRequest`,
//! agent/cron list envelopes, etc.). See `docs/reference/integrations/clawgotcha-api-contract.md`.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, IF_NONE_MATCH};
use reqwest::StatusCode;
use urlencoding::encode;

use crate::config::ClawgotchaRuntimeConfig;
use crate::error::ClawgotchaError;
use crate::models::domain::{AgentDefinition, CronJobDefinition, SwarmDefaults};
use crate::models::wire::{
    AgentbookAgentListEnvelope, AgentbookCronListEnvelope, AgentbookRevisionSummary,
    RegisterInstanceBody, WireAgent, WireCronJob, WireSwarmDefaults,
};
use crate::traits::{
    AgentsDelta, ClawgotchaRegistration, ClawgotchaSyncRead, ClawgotchaWebhooks, CronDelta,
    FetchDelta, HeartbeatPayload, InstanceRegistration,
};

const DEFAULT_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 200;

async fn retry_backoff(attempt: u32) {
    tokio::time::sleep(Duration::from_millis(RETRY_BASE_MS * (1u64 << attempt))).await;
}

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

    fn watermark_from_summary(
        summary: &Option<AgentbookRevisionSummary>,
        field: WatermarkField,
    ) -> u64 {
        summary
            .as_ref()
            .map(|s| match field {
                WatermarkField::Agents => s.agents_max_revision,
                WatermarkField::Cron => s.cron_jobs_max_revision,
            })
            .unwrap_or(0)
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
                        retry_backoff(attempt).await;
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
                        tracing::warn!(
                            error = %crate::error::classify_reqwest_transport(&e),
                            path,
                            attempt,
                            "clawgotcha GET transport retry"
                        );
                        retry_backoff(attempt).await;
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
                        retry_backoff(attempt).await;
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
                        tracing::warn!(
                            error = %crate::error::classify_reqwest_transport(&e),
                            path,
                            attempt,
                            "clawgotcha POST transport retry"
                        );
                        retry_backoff(attempt).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
    }
}

enum WatermarkField {
    Agents,
    Cron,
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
        let body = RegisterInstanceBody {
            instance_name: reg.instance.instance_name.clone(),
            hostname: reg.instance.hostname.clone(),
            version: reg.instance.version.clone(),
            callback_url: reg.instance.callback_url.clone().unwrap_or_default(),
            instance_type: Some("miroclaw".to_string()),
        };
        let bytes = serde_json::to_vec(&body)?;
        self.post_empty_retry("/v1/instances/register", &bytes)
            .await
    }

    async fn send_heartbeat(&self, hb: &HeartbeatPayload) -> Result<(), ClawgotchaError> {
        let enc = encode(hb.instance_name.trim());
        let path = format!("/v1/instances/{enc}/heartbeat");
        let body = serde_json::json!({
            "status": "online",
            "metadata": {
                "loaded_agents_count": hb.loaded_agents_count,
                "cron_jobs_count": hb.cron_jobs_count,
            }
        });
        let bytes = serde_json::to_vec(&body)?;
        self.post_empty_retry(&path, &bytes).await
    }
}

#[async_trait]
impl ClawgotchaSyncRead for ClawgotchaHttpAdapter {
    async fn fetch_agents_since(
        &self,
        revision: Option<u64>,
        etag: Option<&str>,
    ) -> Result<(FetchDelta<AgentsDelta>, Option<String>), ClawgotchaError> {
        let _ = revision;
        let path = "/v1/agents".to_string();
        let (status, out_etag, parsed) = self
            .get_json_retry::<serde_json::Value>(&path, etag)
            .await?;
        if status == StatusCode::NOT_MODIFIED {
            return Ok((FetchDelta::NotModified, out_etag));
        }

        let value = parsed.ok_or_else(|| ClawgotchaError::InvalidResponse("agents body".into()))?;

        // Prefer agentbook `AgentListResponse`; fall back to legacy snake_case envelope.
        let (revision_watermark, agents): (u64, Vec<AgentDefinition>) = if let Ok(env) =
            serde_json::from_value::<AgentbookAgentListEnvelope>(value.clone())
        {
            let wm = Self::watermark_from_summary(&env.revision_summary, WatermarkField::Agents)
                .max(
                    env.agents
                        .iter()
                        .filter(|a| !a.deleted)
                        .map(|a| a.current_revision)
                        .max()
                        .unwrap_or(0),
                );
            let mut agents = Vec::with_capacity(env.agents.len());
            for a in env.agents {
                if a.deleted {
                    continue;
                }
                agents.push(AgentDefinition::try_from(a)?);
            }
            (wm, agents)
        } else {
            let env: AgentsEnvelope = serde_json::from_value(value)
                .map_err(|e| ClawgotchaError::InvalidResponse(format!("agents envelope: {e}")))?;
            let mut agents = Vec::with_capacity(env.agents.len());
            for a in env.agents {
                agents.push(AgentDefinition::try_from(a)?);
            }
            (env.revision_watermark, agents)
        };

        Ok((
            FetchDelta::Modified(AgentsDelta {
                revision_watermark,
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
        let _ = revision;
        let path = "/v1/cron-jobs".to_string();
        let (status, out_etag, parsed) = self
            .get_json_retry::<serde_json::Value>(&path, etag)
            .await?;
        if status == StatusCode::NOT_MODIFIED {
            return Ok((FetchDelta::NotModified, out_etag));
        }

        let value = parsed.ok_or_else(|| ClawgotchaError::InvalidResponse("cron body".into()))?;

        let (revision_watermark, jobs): (u64, Vec<CronJobDefinition>) = if let Ok(env) =
            serde_json::from_value::<AgentbookCronListEnvelope>(value.clone())
        {
            let wm = Self::watermark_from_summary(&env.revision_summary, WatermarkField::Cron).max(
                env.cron_jobs
                    .iter()
                    .map(|j| j.current_revision)
                    .max()
                    .unwrap_or(0),
            );
            let mut jobs = Vec::with_capacity(env.cron_jobs.len());
            for j in env.cron_jobs {
                jobs.push(CronJobDefinition::try_from(j)?);
            }
            (wm, jobs)
        } else {
            let env: CronEnvelope = serde_json::from_value(value)
                .map_err(|e| ClawgotchaError::InvalidResponse(format!("cron envelope: {e}")))?;
            let mut jobs = Vec::with_capacity(env.jobs.len());
            for j in env.jobs {
                jobs.push(CronJobDefinition::try_from(j)?);
            }
            (env.revision_watermark, jobs)
        };

        Ok((
            FetchDelta::Modified(CronDelta {
                revision_watermark,
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
            .get_json_retry::<WireSwarmDefaults>("/v1/config", etag)
            .await?;
        let (status, out_etag, parsed) = if status == StatusCode::NOT_FOUND {
            self.get_json_retry::<WireSwarmDefaults>("/v1/swarm/config", etag)
                .await?
        } else {
            (status, out_etag, parsed)
        };
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
        _callback_url: &str,
        _event_types: &[&str],
    ) -> Result<(), ClawgotchaError> {
        tracing::debug!(
            "clawgotcha: skipping POST …/webhooks; agentbook registers callbacks at instance registration"
        );
        Ok(())
    }
}
