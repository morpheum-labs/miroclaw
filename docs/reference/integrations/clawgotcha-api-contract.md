# Clawgotcha HTTP API contract (Miroclaw client alignment)

This document is the **authoritative wire contract** for the Clawgotcha control plane as consumed by [`crates/clawgotcha`](../../../crates/clawgotcha) [`ClawgotchaHttpAdapter`](../../../crates/clawgotcha/src/client/http.rs).

## Base URL

The Miroclaw setting **`[clawgotcha].url`** is the **HTTP prefix** prepended to every path below (no automatic `/api` insertion).

Examples:

| `clawgotcha.url` value | Example full URL for register |
|------------------------|--------------------------------|
| `https://cp.example.com` | `https://cp.example.com/v1/instances/register` |
| `https://cp.example.com/api` | `https://cp.example.com/api/v1/instances/register` |

Operators who expose APIs under `/api/v1/...` should set `url` to `…/api`.

## Authentication

Phase 0 implementations may omit auth. Production deployments should require **`Authorization: Bearer <token>`** or **`X-API-Key`** on all mutating routes; Miroclaw can be extended later to send these headers.

## Instance identity on heartbeat

`POST /v1/instances/heartbeat` carries **`instance_name`** in the JSON body (same value as `[clawgotcha].instance_name`) so a single control-plane URL can serve many instances without path-per-instance URLs.

---

## `POST /v1/instances/register`

Registers or refreshes a runtime instance.

**Request JSON**

| Field | Type | Required | Notes |
|-------|------|----------|--------|
| `instance_name` | string | yes | Non-empty after trim |
| `callback_url` | string or null | no | Miroclaw webhook URL when using webhook/hybrid sync |

**Response:** `200` with empty body on success.

---

## `POST /v1/instances/heartbeat`

**Request JSON**

| Field | Type | Required |
|-------|------|----------|
| `instance_name` | string | yes |
| `loaded_agents_count` | number (usize) | yes |
| `cron_jobs_count` | number (usize) | yes |

**Response:** `200` empty body.

---

## `GET /v1/instances`

Lists registered instances (control-plane UI / ops).

**Response JSON**

```json
{
  "instances": [
    {
      "instance_name": "prod-gateway-1",
      "callback_url": "https://tunnel.example/webhook/clawgotcha",
      "online": true,
      "last_heartbeat_at": "2026-04-28T12:00:00Z",
      "loaded_agents_count": 3,
      "cron_jobs_count": 2,
      "registered_revision": 42
    }
  ]
}
```

Optional fields may be omitted when unknown.

---

## `GET /v1/instances/{instance_name}`

Returns one instance record (same shape as list elements).

---

## `GET /v1/agents`

Returns full agent list or **delta** when `since_revision` is present.

**Query**

| Param | Meaning |
|-------|---------|
| `since_revision` | Optional `u64`; when set, server may return only agents changed after this revision |

**Conditional GET:** Clients send **`If-None-Match: "<etag>"`**. Respond **`304 Not Modified`** when nothing changed; otherwise **`200`** with **`ETag`** header.

**Response JSON**

```json
{
  "revision_watermark": 100,
  "agents": [
    {
      "name": "researcher",
      "provider": "openrouter",
      "model": "anthropic/claude-sonnet-4",
      "system_prompt": null,
      "api_key": null,
      "temperature": null,
      "max_depth": 8,
      "agentic": false,
      "allowed_tools": [],
      "max_iterations": 10,
      "timeout_secs": null,
      "agentic_timeout_secs": null,
      "skills_directory": null,
      "memory_namespace": null,
      "tools": [],
      "current_revision": 7
    }
  ]
}
```

Field semantics match [`WireAgent`](../../../crates/clawgotcha/src/models/wire.rs).

---

## `GET /v1/cron`

Same pattern as agents: `since_revision`, `If-None-Match`, `revision_watermark`, `jobs` array per [`WireCronJob`](../../../crates/clawgotcha/src/models/wire.rs).

---

## `GET /v1/swarm/config`

Singleton swarm defaults.

**Conditional GET:** `If-None-Match` / `ETag` / `304`.

**Response JSON**

```json
{
  "default_provider": "openrouter",
  "default_model": "anthropic/claude-sonnet-4",
  "current_revision": 3
}
```

---

## `POST /v1/webhooks`

Registers outbound webhook fan-out from Clawgotcha into Miroclaw.

**Request JSON**

```json
{
  "callback_url": "https://tunnel.example/webhook/clawgotcha",
  "event_types": ["agent", "cron", "config"]
}
```

**Response:** `200` empty body.

---

## Optional: `GET /v1/events` (SSE)

Recommended for low-latency sync without polling. Emit **`text/event-stream`** with JSON payloads compatible with [`ChangeEvent`](../../../crates/clawgotcha/src/events.rs) (`kind` tag, snake_case). Not yet consumed by the Rust client by default.

---

## Related tables (reference schema)

| Logical table | Purpose |
|---------------|---------|
| `swarm_runtime_instances` | Instance registry + heartbeat timestamps + stats |
| `swarm_webhook_subscriptions` | Registered callbacks |
| `swarm_agents` / `swarm_cron_jobs` / `swarm_config` | Authoritative definitions with `current_revision` + `last_changed_at` |

Implementations may use different physical names; behavior must match this HTTP contract.
