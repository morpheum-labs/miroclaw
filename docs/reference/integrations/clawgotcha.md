# Clawgotcha integration

This document describes how Miroclaw integrates with the Clawgotcha control plane: crate layout, how to connect, merge policy with external agent lists, **multi-agent profile sync**, and what the host implements today.

**HTTP wire contract (paths, bodies, `ETag`s, agentbook OpenAPI alignment):** [clawgotcha-api-contract.md](clawgotcha-api-contract.md).

## Connecting

1. **Run a compatible Clawgotcha HTTP server** (e.g. agentbook; routes under `/api/v1/...` on the server).
2. **Set `[clawgotcha]` in the Miroclaw config** for each **agent profile** that participates (e.g. `~/.miroclaw/profiles/main/config.toml`), or in hub root config when running single-profile mode:
   - `enabled = true`
   - `url` = HTTP **prefix before** `/v1/...` — if the server serves `/api/v1/...`, use a base ending in **`/api`** (e.g. `http://127.0.0.1:3477/api`).
   - `instance_name` = **non-empty unique id per profile** (required when multiple agents sync to the same control plane).
3. **Start the Miroclaw daemon** (or hub supervisor). On success, logs include **“Registered with Clawgotcha; sync loop running”** after bootstrap for each enabled profile worker.
4. **Optional — webhooks / hybrid:** set `sync_mode` to `webhook` or `hybrid` and set `callback_public_base_url` to the public base where the **hub** gateway is reachable; align `webhook_hmac_secret` with the control plane if the server signs callbacks. The client does not call a separate `POST …/webhooks` on agentbook-style servers (subscription is created at registration).

**Multi-agent / hub:** Clawgotcha agent upserts create **profile directories and `registry.toml` entries** — not `[agents]` blocks in a shared hub `config.toml`. Run one sync loop per profile worker with distinct `instance_name`. Setup: [multi-agent-profiles.md](../../setup-guides/multi-agent-profiles.md).

**Auth:** If the control plane requires `Authorization: Bearer` or `X-API-Key`, the Miroclaw HTTP client does not yet send those from config; use an unauthenticated dev instance or extend the adapter.

**Instance listing:** `GET /v1/instances` is for the control plane / ops tools. Miroclaw only registers **itself** and does not pull the full instance catalog (see [clawgotcha-api-contract.md](clawgotcha-api-contract.md)).

## Crate layout

| Location | Role |
|----------|------|
| [`crates/clawgotcha`](../../../crates/clawgotcha) | HTTP client, sync orchestration, domain/wire models, trait ports (`ClawgotchaClient`, `ConfigReconciler`, etc.). |
| [`src/clawgotcha_host`](../../../src/clawgotcha_host) | Host glue: maps `[clawgotcha]` into runtime settings, [`HostAgents`](../../../src/clawgotcha_host/glue.rs) / [`HostCron`](../../../src/clawgotcha_host/glue.rs) / [`HostReconciler`](../../../src/clawgotcha_host/glue.rs), and profile/registry persistence. Module name `clawgotcha_host` avoids clashing with the `clawgotcha` dependency crate. |

The gateway exposes `POST /webhook/clawgotcha` (respecting [`gateway.path_prefix`](../../../src/config/schema.rs) when set). Events are JSON bodies matching [`ChangeEvent`](../../../crates/clawgotcha/src/events.rs) (`kind` tag, snake_case). In hub mode, webhooks hit the **public hub** gateway.

## Host behavior (current)

| Component | Behavior |
|-----------|----------|
| [`HostAgents`](../../../src/clawgotcha_host/glue.rs) | Remote agent upserts/removes create or update **profile directories** (`profiles/<name>/config.toml` + `workspace/`), **`registry.toml` entries**, and refresh the in-memory delegate map when the daemon runs with a shared `delegate_agents` cell. Does **not** write hub-level `[agents]` into root `config.toml`. |
| [`HostCron`](../../../src/clawgotcha_host/glue.rs) | Upserts via `cron::upsert_clawgotcha_agent_job`; removes via `cron::retire_job_for_clawgotcha_remove`; after each cron list pull implements **`reconcile_jobs_present`** → `cron::retire_clawgotcha_jobs_not_in_remote` (soft-remove rows missing from the snapshot). Cron jobs are scoped to the **profile config** passed to the worker. |
| [`HostReconciler`](../../../src/clawgotcha_host/glue.rs) | `apply_swarm_defaults` writes `default_provider` / `default_model` on the **active profile config** and saves. **`apply_batch`** handles **`CronDeleted`** (retire via `HostCron`) and **`AgentDeleted`** (remove profile from registry via `HostAgents`); other event kinds are logged and rely on the next poll for full rows. |

### Cron deletion propagation (control plane vs Miroclaw)

**Soft retirement:** Scheduling stops and FK-safe run history is preserved whenever retirement runs (`retired_at` set, `enabled = 0`).

**How removals reach the host today:**

1. **Polling:** After each modified `GET` cron response, [`SyncService::pull_cron_delta`](../../../crates/clawgotcha/src/sync/service.rs) upserts every job in the payload, then calls **`reconcile_jobs_present`**. [`HostCron`](../../../src/clawgotcha_host/glue.rs) implements this as **`cron::retire_clawgotcha_jobs_not_in_remote`**: active rows with `source = clawgotcha` whose ids are **not** in that snapshot are retired. An **empty** remote list retires **all** active Clawgotcha jobs locally. This assumes the HTTP envelope is the **full desired set** for the instance (typical list endpoint); if the control plane ever returns partial deltas only, reconciliation must be disabled server-side or the contract extended. If the server responds **`304 Not Modified`**, no body is available — local cron is unchanged until the next **`200`** with a fresh list (ensure deletes bump **`ETag`** / revision semantics on the control plane).
2. **Webhooks:** Signed `POST /webhook/clawgotcha` batches deserialized to [`ChangeEvent`](../../../crates/clawgotcha/src/events.rs) invoke **`apply_batch`**: **`CronDeleted`** retires the job; **`AgentDeleted`** removes the agent profile from the registry.
3. **Heartbeat `cron_jobs_count`:** Still derived from **`cron::list_jobs`** (**active** jobs only). If the control plane needs archived totals, add a separate metric or field — **product decision**.

Production verification: [Testing guide — Production checklist: cron soft-retire and Clawgotcha sync](../../contributing/testing.md#production-checklist-cron-soft-retire-and-clawgotcha-sync).

**Caveats:** Long-lived gateway sessions and in-process tool registries may not see every config change until reload or restart, depending on how those subsystems cache state. Zero-downtime hot-reload for all paths is not guaranteed. After registry/profile changes, restart the affected agent worker or full hub supervisor when tools do not pick up new delegate targets.

## Merge policy vs `agents_list_path` / `agents_list_url`

Loading order for delegate agents in **single-profile** mode is normally: main `config.toml` → optional `agents_list_path` → optional `agents_list_url` (later keys override).

If **`[clawgotcha].enabled = true`** and **`[clawgotcha].authoritative_over_external_lists = true`**, the merge step for `agents_list_path` and `agents_list_url` is **skipped** after log merge starts, so Clawgotcha-driven definitions are not overwritten by static URL/file overlays.

**Multi-agent:** prefer registry profiles and Clawgotcha profile CRUD over hub-level `agents_list_*` keys. External list merge is a legacy single-config path.

When Clawgotcha is disabled, behavior is unchanged: external lists still merge as before.

Environment overrides (`MIROCLAW_AGENTS_LIST_URL`, etc.) still apply before merge logic runs; the authoritative flag only gates the file/URL merge implemented in `merge_external_agent_sources`.

## Guarantees and sync artifacts

- **Polling:** The sync loop pulls remote agents, cron, and swarm config; revision cursors are persisted under `<profile-workspace>/clawgotcha/revisions.json`.
- **Webhooks:** Inbound `POST` bodies are verified when `[clawgotcha].webhook_hmac_secret` is set (`X-Clawgotcha-Signature`, optional `sha256=` prefix); bodies must deserialize to `ChangeEvent`. Verified batches run through **`apply_batch`** (cron/agent deletes); upserts still follow the periodic cron/agent pulls unless the control plane sends sufficient inline payload (today, polls carry full definitions).

## Operational escape hatch

Full re-sync after local revision corruption is not automated here; operators can delete `<workspace>/clawgotcha/revisions.json` for the affected profile and restart (the HTTP client performs a fresh pull). A dedicated admin signal may be added later.

## Related

- Architecture: [agent-profile-hub.md](../../architecture/agent-profile-hub.md)
- Setup: [multi-agent-profiles.md](../../setup-guides/multi-agent-profiles.md)
- Wire contract: [clawgotcha-api-contract.md](clawgotcha-api-contract.md)
