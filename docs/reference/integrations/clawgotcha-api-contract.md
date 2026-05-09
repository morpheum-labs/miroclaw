# Clawgotcha HTTP API contract (Miroclaw client alignment)

This document describes the wire contract implemented by [`crates/clawgotcha`](../../../crates/clawgotcha) [`ClawgotchaHttpAdapter`](../../../crates/clawgotcha/src/client/http.rs).

The **reference surface** is the agentbook clawgotcha OpenAPI (`internal/api/openapi.json` in that repository): routes live under **`/api/v1/...`** on the HTTP server. The client prepends **`[clawgotcha].url`** to paths such as **`/v1/instances/register`**, so operators expose **`…/api`** as the prefix when the server mounts **`/api/v1`** (see table below).

Legacy control planes that used snake_case JSON (`revision_watermark`, `jobs`, **`GET /v1/cron`**, **`GET /v1/swarm/config`**) are still accepted where the adapter implements a fallback.

## Base URL

The Miroclaw setting **`[clawgotcha].url`** is the **HTTP prefix** prepended to every path below (no automatic `/api` insertion).

| `clawgotcha.url` value | Example full URL for register |
|------------------------|--------------------------------|
| `https://cp.example.com` | `https://cp.example.com/v1/instances/register` |
| `https://cp.example.com/api` | `https://cp.example.com/api/v1/instances/register` |

## Authentication

When the server enables API keys, requests send **`Authorization: Bearer <token>`** or **`X-API-Key: <key>`** on **`/api/v1/*`**. Miroclaw fills this from **`[clawgotcha].api_key`** or **`MIROCLAW_CLAWGOTCHA_API_KEY`**.

**Per-instance secret:** On first registration (or when a legacy row had no secret), **`POST /v1/instances/register`** returns **`instance_api_secret`** once. Store it outside **`config.toml`** (recommended: **`[clawgotcha].instance_api_secret_file`** or **`MIROCLAW_CLAWGOTCHA_INSTANCE_SECRET`**). The adapter sends it as **`X-Instance-Secret`** on every request when configured (required for **`GET …/mcp-credentials`**).

### What the Miroclaw client implements

[`ClawgotchaHttpAdapter`](../../../crates/clawgotcha/src/client/http.rs) drives **registration**, **heartbeat**, pulls of **agents**, **cron jobs**, **swarm config**, and optional **`GET /v1/instances/{instance}/agents/by-name/{agent}/mcp-credentials`** for delegate MCP credential overlays. It does **not** call **`GET /v1/instances`** or **`GET /v1/instances/{instance_name}`** for discovery.

So **runtime-instance discovery** (listing every registered Miroclaw/zero-claw peer on the control plane) is **not** surfaced in this repo: that data is for **Clawgotcha’s own UI/API** (or other ops tools) that query the server directly. A running Miroclaw node only registers **itself** under `[clawgotcha].instance_name`; it never downloads the full instance catalog.

---

## `POST /v1/instances/register`

Registers or upserts a runtime instance (agentbook **`RegisterInstanceRequest`**).

**Request JSON**

| Field | Type | Required | Notes |
|-------|------|----------|--------|
| `instance_name` | string | yes | Same as `[clawgotcha].instance_name` |
| `hostname` | string | yes | Miroclaw sends `hostname::get()` or `"unknown"` |
| `version` | string | yes | Miroclaw sends the running crate version (`CARGO_PKG_VERSION`) |
| `callback_url` | string | yes | Full webhook URL (`…/webhook/clawgotcha`). **Agentbook rejects empty/whitespace** (`400`), stores it `NOT NULL`, and creates an HMAC webhook subscription targeting this URL. Miroclaw only fills this when `[clawgotcha].callback_public_base_url` is set — **poll-only setups against agentbook must still supply a non-empty reachable URL** (or relax validation server-side). |
| `instance_type` | string | no | Miroclaw sends `"miroclaw"` |

**Response:** `200` JSON **`RegisterInstanceResponse`**: `instance` (runtime row; agentbook/OpenAPI uses PascalCase fields on `SwarmRuntimeInstance`) and `revision_summary` (`RevisionSummary`: revision watermarks). The Miroclaw [`ClawgotchaHttpAdapter`](../../../crates/clawgotcha/src/client/http.rs) **does not parse this JSON** — it treats **any HTTP 2xx** as success (`post_empty_retry` returns `Ok(())` and drops the body). Initial sync still follows with separate `GET /v1/agents` (etc.) calls.

---

## `POST /v1/instances/{instance_name}/heartbeat`

Runtime heartbeat (agentbook **`HeartbeatRequest`**). The instance key is in the **path** (URL-encoded); **`instance_name` is not duplicated** in the JSON body.

**Request JSON**

| Field | Type | Required |
|-------|------|----------|
| `status` | string | no | Miroclaw sends `"online"` |
| `metadata` | object | no | Miroclaw sends `loaded_agents_count` and `cron_jobs_count` as integers |

**Response:** `200` (agentbook may return `revision_summary`; the client ignores the body).

---

## `GET /v1/agents`

**Primary (agentbook):** **`AgentListResponse`**: `agents` ( **`SwarmAgent`** rows, mixed PascalCase/snake_case JSON per OpenAPI) plus **`revision_summary`** (`agents_max_revision`, etc.).

**Legacy fallback:** snake_case envelope with **`revision_watermark`** and **`agents`** shaped like [`WireAgent`](../../../crates/clawgotcha/src/models/wire.rs).

**Conditional GET:** clients send **`If-None-Match`**; **`304`** is handled.

---

## `GET /v1/cron-jobs`

**Primary (agentbook):** **`CronJobListResponse`**: **`cron_jobs`** (**`SwarmCronJob`**, PascalCase fields in JSON) plus **`revision_summary`**.

**Legacy fallback:** **`GET /v1/cron`** with **`revision_watermark`** and **`jobs`** shaped like [`WireCronJob`](../../../crates/clawgotcha/src/models/wire.rs).

**Miroclaw host semantics:** After a successful parse of the cron list body, the sync layer upserts each job, then runs a **snapshot reconcile**: local DB rows with `source = clawgotcha` that are **not** listed in that response are **soft-retired**. Operators should treat each successful response as the **full desired cron set** for the instance (empty list = retire all Clawgotcha cron locally). Partial-delta-only APIs are incompatible unless reconciliation is relaxed.

---

## `GET /v1/config`

Singleton swarm defaults (**`SwarmConfig`** on agentbook).

If **`GET /v1/config`** returns **`404`**, the client retries **`GET /v1/swarm/config`** (legacy path).

**Conditional GET:** `If-None-Match` / `ETag` / `304`.

---

## Webhooks

Agentbook registers webhook subscriptions during **`POST /v1/instances/register`**; there is **no** separate **`POST /v1/webhooks`** in the OpenAPI document. Miroclaw’s **`register_webhook`** call is a **no-op** when using this adapter.

---

## Optional: `GET /api/v1/events` (SSE)

Low-latency change notifications; payloads align with **`ChangeEvent`** where applicable. Not consumed by the Rust client by default.

---

## `GET /v1/instances/{instance}/agents/by-name/{agent}/mcp-credentials`

Returns decrypted MCP credential payloads for bindings that set **`mcp_server_name`**, when **`X-Instance-Secret`** matches the instance API secret and (when enabled) the global API key is present. **`revision`** mirrors the agent’s **`current_revision`** (incremented on credential create/rotate/delete on agentbook).

Miroclaw uses this only during **agentic delegate** tool loops to merge vault auth into scoped MCP transports; secrets are cached in memory and invalidated when Clawgotcha applies agent updates.

---

## Related tables (reference schema)

| Logical table | Purpose |
|---------------|---------|
| `swarm_runtime_instances` | Instance registry + heartbeat |
| Webhook subscriptions | Registered callbacks (created at registration on agentbook) |
| `swarm_agents` / `swarm_cron_jobs` / `swarm_config` | Authoritative definitions with revisions |

Implementations may use different physical names; HTTP behavior matches the OpenAPI where listed above.
