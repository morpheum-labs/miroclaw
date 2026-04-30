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

When the server enables API keys, requests should send **`Authorization: Bearer <token>`** or **`X-API-Key: <key>`** on mutating routes; Miroclaw can be extended later to send these headers from config.

### What the Miroclaw client implements

[`ClawgotchaHttpAdapter`](../../../crates/clawgotcha/src/client/http.rs) and [`ClawgotchaSyncRead`](../../../crates/clawgotcha/src/traits.rs) only drive **registration**, **heartbeat**, and pulls of **agents**, **cron jobs**, and **swarm config**. They do **not** call **`GET /v1/instances`** or **`GET /v1/instances/{instance_name}`**.

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
| `callback_url` | string | yes | Webhook URL when using webhook/hybrid sync; use `""` if unset |
| `instance_type` | string | no | Miroclaw sends `"miroclaw"` |

**Response:** `200` JSON (`RegisterInstanceResponse` on agentbook); the client treats any **2xx** as success.

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

## Related tables (reference schema)

| Logical table | Purpose |
|---------------|---------|
| `swarm_runtime_instances` | Instance registry + heartbeat |
| Webhook subscriptions | Registered callbacks (created at registration on agentbook) |
| `swarm_agents` / `swarm_cron_jobs` / `swarm_config` | Authoritative definitions with revisions |

Implementations may use different physical names; HTTP behavior matches the OpenAPI where listed above.
