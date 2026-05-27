# Development: Configuration and Grok Browser Token

**Status:** Implemented  
**Last updated:** 2026-05-26  
**Risk tier:** Medium (`src/config/schema.rs`, `src/providers/bun_browser.rs`, `src/providers/grok_browser/`)

This document is the implementation contract for operator configuration in Miroclaw/ZeroClaw, with a focused spec for **`[grok_browser].token`** (bun-browser daemon credential) and refresh-on-auth-failure behavior.

---

## Goals

1. **Single on-disk operator config** — `config.toml` (or profile `profiles/<name>/config.toml`) holds non-interactive settings: providers, models, URLs, channel flags, and secrets when appropriate.
2. **Environment overrides** — Production and containers can override secrets and provider selection without editing files (`MIROCLAW_*`, provider-specific `*_API_KEY`, `BUN_BROWSER_TOKEN`, etc.).
3. **CLI tools stay session-based** — `[claude_code]`, `[codex_cli]`, `[gemini_cli]`, `[opencode_cli]` do not require API keys in TOML unless `env_passthrough` is used for API-key billing; auth comes from each CLI’s own login/OAuth.
4. **Grok-browser credential clarity** — The grok-browser provider’s “API” credential is the **bun-browser daemon token**, stored as **`[grok_browser].token`**, not an xAI API key.
5. **Rotating bun tokens** — When an in-memory token fails, re-read **`[grok_browser].token` from disk** and retry once so scripts like `scripts/tokenupdate.sh` take effect without restarting the process.

---

## Configuration model (general)

### File resolution

Startup uses `Config::load_or_init()` (`src/config/schema.rs`):

| Priority | Source |
|----------|--------|
| 1 | `MIROCLAW_CONFIG` — explicit file path |
| 2 | Under config dir: `config.toml` → `configuration.yaml` → `config.yaml` |
| 3 | Default path `…/config.toml` (created on first run) |

Config directory: `~/.miroclaw` (or `~/miroclaw` if present), overridable via `MIROCLAW_CONFIG_DIR` / `--config-dir`.

Hub/multi-agent: per-profile `profiles/<name>/config.toml` for runtime provider settings; hub root config for `[hub]` and public `[gateway]` only.

### HTTP LLM providers (Anthropic, Gemini API, xAI, OpenRouter, …)

| Setting | Typical location |
|---------|------------------|
| `default_provider`, `default_model`, `api_url`, `api_path` | Top-level `config.toml` |
| `api_key` | `config.toml` and/or env (see precedence below) |
| Named endpoints | `[model_providers.<id>]` |
| Routing | `model_routes`, `embedding_routes` |
| Delegate sub-agents | `[agents.<name>]` (in-profile); registry profiles preferred for isolation |

**Credential precedence** (after TOML load, `apply_env_overrides()`, then `create_provider` — `src/providers/mod.rs`):

1. Provider-specific env (e.g. `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `XAI_API_KEY`) — for well-known providers, often preferred over a generic config `api_key` when both are set.
2. Top-level `api_key` in config (decrypted if `[secrets] encrypt = true`).
3. `MIROCLAW_API_KEY` / `API_KEY` (applied in `apply_env_overrides`).

**Provider / model env overrides:**

1. `MIROCLAW_PROVIDER`
2. `MIROCLAW_MODEL_PROVIDER` / `MODEL_PROVIDER`
3. `PROVIDER` (legacy; only when config still uses default `openrouter`)
4. `default_provider` in `config.toml`

### What not to put in TOML

| Concern | Where it lives |
|---------|----------------|
| Claude Code / Codex / Gemini CLI / OpenCode auth | CLI OAuth/session (optional keys via `env_passthrough`) |
| grok.com user session | Browser profile behind bun-browser |
| xAI HTTP API | `default_provider = "xai"` + `XAI_API_KEY` / `api_key` — **not** `[grok_browser]` |

### Security for `config.toml`

- Treat the file as **secret-bearing** when it contains `api_key`, `[grok_browser].token`, or channel tokens.
- **Permissions:** `chmod 600` on Unix; runtime warns if world-readable; new files created with `0600`.
- **`[secrets] encrypt = true`:** encrypt sensitive fields at rest via `SecretStore`; reload path must decrypt on re-read.
- **Do not commit** real configs; prefer env-only secrets in CI/Docker/shared hosts.
- **Backups:** copying `~/.miroclaw` copies secrets — use encryption or env-only in production.

---

## Grok browser: `[grok_browser].token`

### Terminology

| Name | Meaning |
|------|---------|
| **grok-browser** (`default_provider = "grok-browser"`) | LLM via bun-browser + logged-in grok.com (site adapters) |
| **bun-browser token** | Bearer token for the local bun-browser daemon (`POST /site/run`, `/command`) |
| **`[grok_browser].token`** | Canonical TOML field for that daemon token |
| **xAI API** (`xai` / `grok` provider) | Separate stack; uses `XAI_API_KEY`, not `[grok_browser]` |

Do **not** use field name `api` in docs or new scripts; optional serde alias `api` may be supported only for backward compatibility with existing `tokenupdate.sh` output.

### Example `config.toml`

```toml
default_provider = "grok-browser"
default_model = "auto"

[grok_browser]
# Bun-browser daemon bearer token (grok-browser credential).
token = "your-bun-browser-token"
host = "http://127.0.0.1:19824"   # optional; default 127.0.0.1:19824
model = "auto"
session_mode = "follow"           # or "stateless"
disable_search = true
max_parallel_tabs = 8
request_timeout_secs = 2700
# agent_id / agent_name — optional Grok project agent
```

### Token resolution precedence

When resolving the bun-browser token (initial load and after auth refresh):

| Order | Source |
|-------|--------|
| 1 | `BUN_BROWSER_TOKEN` (env) — always wins when non-empty |
| 2 | `[grok_browser].token` from config file (decrypt if `secrets.encrypt`) |
| 3 | `~/.bun-browser/daemon.json` → `token` |

`BUN_BROWSER_HOST` / `[grok_browser].host` / `BUN_BROWSER_TIMEOUT` continue to control host and timeouts as today.

### Auth failure and refresh (required behavior)

**Problem today:** `BunBrowserClient` resolves the token once in `ensure_config()` and caches it; TOML has no `token` field; `scripts/tokenupdate.sh` writes ignored `[grok_browser] api`.

**Required behavior after implementation:**

1. On each grok-browser HTTP call, use the cached `BunBrowserConfig` when valid.
2. If the daemon returns an **auth error** (HTTP `401`/`403`, or adapter/body hints such as `unauthorized`, `invalid token`, `forbidden` — tune against real bun-browser responses):
   - **Invalidate** cached auth on the client.
   - **Re-read** `[grok_browser].token` from the on-disk config file (`config_path` known from `ProviderRuntimeOptions` / `Config::config_path`).
   - Decrypt if `[secrets] encrypt = true`.
   - Rebuild `BunBrowserConfig` and **retry the request once**.
3. Do not loop retries beyond **one** refresh per logical request (avoid storms).
4. If env `BUN_BROWSER_TOKEN` is set, refresh still re-resolves with env first; TOML-only rotation workflows should leave env unset.

**Not required for v1:** Re-parse full config on every grok request without failure; channel hot-reload of `provider_runtime_options.grok_browser` (auth-failure disk reload covers `tokenupdate.sh`).

### Distinction from other credentials

```text
config.toml
  [grok_browser].token     → bun-browser daemon (local)
  api_key                  → default HTTP LLM provider (e.g. openrouter, anthropic, xai)
  [agents.*].api_key       → delegate overrides

Environment
  BUN_BROWSER_TOKEN        → overrides [grok_browser].token
  XAI_API_KEY              → xAI HTTP API only (provider "xai")

External
  ~/.bun-browser/daemon.json → fallback token
  grok.com in browser        → user session (not in TOML)
```

---

## Implementation checklist

Use this as the agent/PR task list. Check off in PR description.

### Schema and load path

- [x] Add `token: Option<String>` to `GrokBrowserConfig` in `src/config/schema.rs`.
- [x] Optional: `#[serde(alias = "api")]` for backward compatibility.
- [x] On `load_or_init`, `decrypt_optional_secret` for `config.grok_browser.token` when `secrets.encrypt`.
- [x] Add `read_grok_browser_token_from_config(path, secrets_encrypt) -> Result<Option<String>>` (parse TOML/YAML, decrypt) for disk refresh.

### Bun-browser client

- [x] Extend `BunBrowserClient` with `config_path`, `secrets_encrypt`, and initial token from options.
- [x] Update `resolve_token()` to use precedence: env → config token (initial or disk) → `daemon.json`.
- [x] Add `invalidate_auth()` (`self.config = None`).
- [x] Add `is_auth_error(status, body) -> bool`.
- [x] In `post_site_run` / `post_command`: on auth error → invalidate → re-resolve (disk read) → single retry.
- [x] Pass new fields from `GrokBrowserProvider::new` / `new_deferred`.

### Provider wiring

- [x] Pass `config.config_path` into `ProviderRuntimeOptions` or bun client (today only `zeroclaw_dir` parent is set).
- [x] `provider_runtime_options_from_config`: include path needed for reload.

### Scripts and docs

- [x] `scripts/tokenupdate.sh`: write `token` not `api` (alias still accepts `api` if implemented).
- [x] `docs/reference/api/providers-reference.md`: Grok Browser auth = `[grok_browser].token`, env override, refresh behavior.
- [x] `docs/reference/api/config-reference.md`: `[grok_browser]` table includes `token`.

### Tests

- [x] Unit: token precedence (env > toml > daemon.json).
- [x] Unit: auth failure triggers invalidate + disk re-read + retry.
- [x] Unit/component: `create_provider("grok-browser")` with token only in `[grok_browser]`.
- [x] Existing `grok_browser` tests still pass.

### Validation

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

---

## Operator workflows

### Rotate bun-browser token (TOML as source of truth)

1. Update token in bun-browser / run `bash scripts/buntoken.sh` and `bash scripts/tokenupdate.sh` (after script writes `token`).
2. Ensure `BUN_BROWSER_TOKEN` is **unset** if TOML should win on refresh.
3. Next grok-browser call that fails with old token should refresh from disk; no daemon restart required.

### Production (secrets not on disk)

- Omit `token` from TOML; set `BUN_BROWSER_TOKEN` in the environment or rely on `daemon.json` on the host.

### Local dev (single file)

- Keep provider/model and `[grok_browser]` in `~/.miroclaw/config.toml` (or profile config).
- `chmod 600` the file; enable `[secrets] encrypt = true` if storing keys in file.

---

## Files to touch (reference)

| Area | Path |
|------|------|
| Config schema | `src/config/schema.rs` |
| Bun-browser HTTP | `src/providers/bun_browser.rs` |
| Grok-browser provider | `src/providers/grok_browser/mod.rs` |
| Provider factory / options | `src/providers/mod.rs` |
| Token sync script | `scripts/tokenupdate.sh` |
| Provider docs | `docs/reference/api/providers-reference.md` |
| Config docs | `docs/reference/api/config-reference.md` |

---

## Out of scope (this spec)

- Storing grok.com cookies in TOML.
- Per-request full config reload without auth failure.
- Channel runtime hot-reload of entire `[grok_browser]` section (optional follow-up).
- Replacing `~/.bun-browser/daemon.json` as fallback (keep for compatibility).

---

## PR template snippet

**Title:** `feat(config): add [grok_browser].token with auth refresh from disk`

**Summary:**

- Document and implement bun-browser token in `[grok_browser].token`.
- Resolve token: env → TOML → daemon.json; decrypt on load/reload.
- On bun-browser auth errors, re-read TOML token and retry once.

**Test plan:**

- [ ] Unit tests for precedence and refresh
- [ ] Manual: set `default_provider = "grok-browser"`, token in TOML, run agent turn
- [ ] Manual: change token on disk after invalidating daemon auth; confirm recovery without restart
