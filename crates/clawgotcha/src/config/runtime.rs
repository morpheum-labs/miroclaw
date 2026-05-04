//! Values needed to construct HTTP clients and sync loops (filled by the host from `[clawgotcha]`).

/// Copy-friendly settings independent of host `Config` types.
#[derive(Debug, Clone)]
pub struct ClawgotchaRuntimeConfig {
    pub base_url: String,
    pub instance_name: String,
    /// Sent on `POST …/instances/register` (agentbook / OpenAPI contract).
    pub hostname: String,
    /// Runtime version string for registration (e.g. crate version).
    pub version: String,
    pub sync_mode: SyncMode,
    pub heartbeat_interval_secs: u64,
    pub poll_interval_secs: u64,
    pub callback_public_base_url: Option<String>,
    /// Hex-encoded shared secret for inbound webhook HMAC (optional).
    pub webhook_hmac_secret: Option<String>,
    /// Control-plane API key (`Authorization: Bearer` / `X-API-Key`) when the server sets `CLAWGOTCHA_API_KEY`.
    pub control_plane_api_key: Option<String>,
    /// Per-instance secret for `X-Instance-Secret` (MCP credential reveal and future instance-scoped routes).
    pub instance_api_secret: Option<String>,
}

/// Sync strategy (polling, webhook push hints, or both).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    #[default]
    Poll,
    Webhook,
    Hybrid,
}
