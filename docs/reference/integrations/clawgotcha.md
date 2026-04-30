# Clawgotcha integration

This document describes how Miroclaw integrates with the Clawgotcha control plane: crate layout, how to connect, merge policy with external agent lists, and what the host implements today.

**HTTP wire contract (paths, bodies, `ETag`s, agentbook OpenAPI alignment):** [clawgotcha-api-contract.md](clawgotcha-api-contract.md).

## Connecting

1. **Run a compatible Clawgotcha HTTP server** (e.g. agentbook; routes under `/api/v1/...` on the server).
2. **Set `[clawgotcha]` in Miroclaw config** (e.g. `~/.miroclaw/config.toml`):
   - `enabled = true`
   - `url` = HTTP **prefix before** `/v1/...` — if the server serves `/api/v1/...`, use a base ending in **`/api`** (e.g. `http://127.0.0.1:3477/api`).
   - `instance_name` = non-empty unique id for this runtime.
3. **Start the Miroclaw daemon** with that config. On success, logs include **“Registered with Clawgotcha; sync loop running”** after bootstrap.
4. **Optional — webhooks / hybrid:** set `sync_mode` to `webhook` or `hybrid` and set `callback_public_base_url` to the public base where the gateway is reachable; align `webhook_hmac_secret` with the control plane if the server signs callbacks. The client does not call a separate `POST …/webhooks` on agentbook-style servers (subscription is created at registration).

**Auth:** If the control plane requires `Authorization: Bearer` or `X-API-Key`, the Miroclaw HTTP client does not yet send those from config; use an unauthenticated dev instance or extend the adapter.

**Instance listing:** `GET /v1/instances` is for the control plane / ops tools. Miroclaw only registers **itself** and does not pull the full instance catalog (see [clawgotcha-api-contract.md](clawgotcha-api-contract.md)).

## Crate layout

| Location | Role |
|----------|------|
| [`crates/clawgotcha`](../../../crates/clawgotcha) | HTTP client, sync orchestration, domain/wire models, trait ports (`ClawgotchaClient`, `ConfigReconciler`, etc.). |
| [`src/clawgotcha_host`](../../../src/clawgotcha_host) | Host glue: maps `[clawgotcha]` into runtime settings, [`HostAgents`](../../../src/clawgotcha_host/glue.rs) / [`HostCron`](../../../src/clawgotcha_host/glue.rs) / [`HostReconciler`](../../../src/clawgotcha_host/glue.rs), and [`DelegateAgentConfig`](../../../src/config/schema.rs) mapping. Module name `clawgotcha_host` avoids clashing with the `clawgotcha` dependency crate. |

The gateway exposes `POST /webhook/clawgotcha` (respecting [`gateway.path_prefix`](../../../src/config/schema.rs) when set). Events are JSON bodies matching [`ChangeEvent`](../../../crates/clawgotcha/src/events.rs) (`kind` tag, snake_case).

## Host behavior (current)

| Component | Behavior |
|-----------|----------|
| [`HostAgents`](../../../src/clawgotcha_host/glue.rs) | Remote agent upserts/removes update `[agents]` in config, persist via `Config::save`, and refresh the in-memory delegate map when the daemon runs with a shared `delegate_agents` cell. |
| [`HostCron`](../../../src/clawgotcha_host/glue.rs) | Remote cron upserts/removes go through `cron::upsert_clawgotcha_agent_job` / `cron::remove_job` (same family as gateway cron storage). |
| [`HostReconciler`](../../../src/clawgotcha_host/glue.rs) | `apply_swarm_defaults` writes `default_provider` / `default_model` and saves config. `apply_batch` (webhook-driven event batches) is currently a **no-op** aside from debug logging; **polling** is what applies agent/cron deltas from the API. |

**Caveats:** Long-lived gateway sessions and in-process tool registries may not see every config change until reload or restart, depending on how those subsystems cache state. Zero-downtime hot-reload for all paths is not guaranteed.

## Merge policy vs `agents_list_path` / `agents_list_url`

Loading order for delegate agents is normally: main `config.toml` → optional `agents_list_path` → optional `agents_list_url` (later keys override).

If **`[clawgotcha].enabled = true`** and **`[clawgotcha].authoritative_over_external_lists = true`**, the merge step for `agents_list_path` and `agents_list_url` is **skipped** after log merge starts, so Clawgotcha-driven definitions are not overwritten by static URL/file overlays.

When Clawgotcha is disabled, behavior is unchanged: external lists still merge as before.

Environment overrides (`MIROCLAW_AGENTS_LIST_URL`, etc.) still apply before merge logic runs; the authoritative flag only gates the file/URL merge implemented in `merge_external_agent_sources`.

## Guarantees and sync artifacts

- **Polling:** The sync loop pulls remote agents, cron, and swarm config; revision cursors are persisted under `<workspace>/clawgotcha/revisions.json`.
- **Webhooks:** Inbound `POST` bodies are verified when `[clawgotcha].webhook_hmac_secret` is set (`X-Clawgotcha-Signature`, optional `sha256=` prefix); bodies must deserialize to `ChangeEvent`. Webhook **batches** do not yet drive full reconciliation through `apply_batch` (see table above).

## Operational escape hatch

Full re-sync after local revision corruption is not automated here; operators can delete `<workspace>/clawgotcha/revisions.json` and restart (the HTTP client performs a fresh pull). A dedicated admin signal may be added later.
