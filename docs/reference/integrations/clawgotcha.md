# Clawgotcha integration

This document describes how Miroclaw integrates with the Clawgotcha control plane: crate layout, merge policy with external agent lists, and hot-reload scope.

**HTTP wire contract (paths, bodies, ETags):** [clawgotcha-api-contract.md](clawgotcha-api-contract.md). A minimal test server lives in [`crates/clawgotcha-server`](../../../crates/clawgotcha-server).

## Crate layout

| Location | Role |
|----------|------|
| [`crates/clawgotcha`](../../../crates/clawgotcha) | HTTP client, sync orchestration, domain/wire models, trait ports (`ClawgotchaClient`, `ConfigReconciler`, etc.). |
| [`src/clawgotcha_host`](../../../src/clawgotcha_host) | Host glue: maps `[clawgotcha]` config into runtime settings, stub trait impls, [`DelegateAgentConfig`](../../config/schema.rs) mapping. Named `clawgotcha_host` to avoid clashing with the `clawgotcha` dependency crate. |

The gateway exposes `POST /webhook/clawgotcha` (respecting [`gateway.path_prefix`](../../config/schema.rs) when set). Events are JSON bodies matching [`ChangeEvent`](../../../crates/clawgotcha/src/events.rs) (`kind` tag, snake_case).

## Merge policy vs `agents_list_path` / `agents_list_url`

Loading order for delegate agents is normally: main `config.toml` → optional `agents_list_path` → optional `agents_list_url` (later keys override).

If **`[clawgotcha].enabled = true`** and **`[clawgotcha].authoritative_over_external_lists = true`**, the merge step for `agents_list_path` and `agents_list_url` is **skipped** after log merge starts, so Clawgotcha-driven definitions are not overwritten by static URL/file overlays.

When Clawgotcha is disabled, behavior is unchanged: external lists still merge as before.

Environment overrides (`MIROCLAW_AGENTS_LIST_URL`, etc.) still apply before merge logic runs; the authoritative flag only gates the file/URL merge implemented in `merge_external_agent_sources`.

## Hot-reload scope and guarantees

**Current wiring (stubs):** [`StubAgents`](../../../src/clawgotcha_host/glue.rs), [`StubCron`](../../../src/clawgotcha_host/glue.rs), and [`StubReconciler`](../../../src/clawgotcha_host/glue.rs) log work only. They are placeholders until:

1. **Agents:** delegate [`DelegateAgentConfig`](../../config/schema.rs) updates propagate into the same structures the gateway and `delegate` tool use (today built largely at gateway startup), with explicit coordination for `AppState` and tool registries.
2. **Cron:** upserts use the same persistence path as [`/api/cron`](../../../src/gateway/api.rs) / [`cron::store`](../../../src/cron/store.rs).

**Guarantees:**

- **Polling:** applies remote deltas via trait ports; revisions are persisted under `<workspace>/clawgotcha/revisions.json`.
- **Webhooks:** verified when `[clawgotcha].webhook_hmac_secret` is set (`X-Clawgotcha-Signature`, optional `sha256=` prefix); bodies must deserialize to `ChangeEvent`.
- **Best-effort:** in-flight gateway sessions and scheduler jobs are not drained by the stubs; full zero-downtime guarantees require the future hot-reload implementation above.

## Operational escape hatch

Full re-sync after local revision corruption is not automated here; operators can delete `<workspace>/clawgotcha/revisions.json` and restart (the HTTP client performs a fresh pull). A dedicated admin signal may be added later.
