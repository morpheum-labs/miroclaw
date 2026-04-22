# Machine usage — keep MiroClaw running

This guide is the operator checklist for a **host that runs MiroClaw** (laptop, workstation, or small server). Use it for first bring-up and day-to-day “is this machine good?” checks.

## Prerequisites

- **Rust** 1.87+ (see repository `README` for the pinned expectation).
- Network egress to your **LLM provider** (and to AgentFloor, if you use `[platform]`).
- A config workspace — default **`~/.miroclaw/`** — with enough disk for logs, sqlite memory, and any transcripts you keep.

## One-time setup

1. **Build** the binary (from a clone of this repo):

   ```bash
   cargo build --release
   # Binary: target/release/miroclaw
   ```

   Add `target/release` to your `PATH`, or run via full path.

2. **Onboard** so `config.toml` and workspace layout exist:

   ```bash
   miroclaw onboard
   # or non-interactive:
   miroclaw onboard --api-key "sk-..." --provider openrouter
   ```

3. **Provider credentials** — store API keys in **`~/.miroclaw/.env`** and/or provider-specific env vars (see [providers-reference.md](../reference/api/providers-reference.md)). Config file keys are supported, but env is often easier to rotate on shared machines.

4. **AgentFloor / Memory Vault (optional)** — if this worker should use the platform vault, set `[platform]` in `config.toml` (see [miroclaw-platform-integration.md](../miroclaw-platform-integration.md)). Without `memory_access_id`, memory behavior stays on local sqlite/markdown as documented for `[memory]`.

## Where config lives

Resolution order at startup:

1. `MIROCLAW_WORKSPACE` (if set) — points at the workspace root.
2. Else `~/.miroclaw/active_workspace.toml` marker (if present).
3. Else default `~/.miroclaw/config.toml`.

MiroClaw logs the resolved path at **INFO** on startup. If something “can’t find config,” confirm `MIROCLAW_WORKSPACE` and the file on disk.

## How to run workloads

| Goal | Command / pattern |
|------|-------------------|
| Local interactive dev | `miroclaw agent` |
| Editorial swarm (AgentFloor) | `miroclaw agent --mode=editorial-swarm --headline-ref=... --n-agents=...` (see [editorial-swarm-guide.md](../editorial-swarm-guide.md)) |
| HTTP/WebSocket API only | `miroclaw gateway start` (or `miroclaw gateway` subcommands for status/restart) |
| Always-on (gateway + channels + scheduler) | `miroclaw daemon` — for boot persistence, use `miroclaw service install` where supported |

Gateway bind address, port, pairing, and path prefix: **`[gateway]`** in [config-reference.md](../reference/api/config-reference.md). Default listen port is **42617**; override with `miroclaw daemon -p …` or config.

## Web dashboard

The gateway can serve a **web UI** from **embedded** build assets (when the `embedded-web-ui` feature is enabled) or from a **local Vite `dist/`** tree. Use **`[webui].external_path`** to point at a directory that contains `index.html` (and the usual `assets/`). If `external_path` is empty, the runtime prefers embedded assets when the binary was built with that feature. See [config-reference.md](../reference/api/config-reference.md) for `[webui]` keys.

## Health checks (do this after reboot or network changes)

```bash
miroclaw status
miroclaw doctor
```

If channels or the gateway fail, use [troubleshooting.md](../ops/troubleshooting.md) and, for long-running services, [operations-runbook.md](../ops/operations-runbook.md).

## Security habits on a shared or production host

- Keep **`[gateway].require_pairing`** on unless you have a deliberate, audited exception.
- Avoid **`[gateway].allow_public_bind = true`** unless a reverse proxy or firewall policy justifies it.
- Do not commit real **`[platform].memory_access_id`** values; treat it like a secret.

## Related

- [setup-guides/README.md](README.md) — other first-run paths
- [quick-start-command-reference.md](quick-start-command-reference.md) — flags and cheat sheet
- [../README.md](../README.md) — doc hub and deep links
