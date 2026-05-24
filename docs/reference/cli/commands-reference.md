# Miroclaw Commands Reference

This reference is derived from the current CLI surface (`miroclaw --help`).

Last verified: **April 15, 2026**.

## Global options

- `--config-dir <DIR>` — alternate Miroclaw config/workspace root (accepted on the top-level command and propagated to subcommands).

## Top-Level Commands

Order matches `miroclaw --help`.

| Command | Purpose |
|---|---|
| `onboard` | Initialize workspace/config quickly or interactively |
| `agent` | Run interactive chat or single-message mode |
| `gateway` | Start or manage the HTTP/WebSocket gateway (webhooks, pairing, websockets) |
| `daemon` | Start supervised runtime (gateway + channels + optional heartbeat/scheduler) |
| `service` | Manage user-level OS service lifecycle |
| `doctor` | Run diagnostics for daemon, scheduler, and channel freshness |
| `status` | Print current configuration and system summary |
| `estop` | Engage/resume emergency stop levels and inspect estop state |
| `cron` | Manage scheduled tasks |
| `models` | Refresh provider model catalogs |
| `providers` | List provider IDs, aliases, and active provider |
| `channel` | Manage channels and channel health checks |
| `integrations` | Browse 50+ integrations |
| `skills` | List/install/remove skills |
| `migrate` | Import from external runtimes (OpenClaw) or migrate layout (`profiles`) |
| `auth` | Manage provider subscription authentication profiles (OAuth, tokens, profiles) |
| `hardware` | Discover and introspect USB hardware |
| `peripheral` | Configure and flash peripherals |
| `memory` | List, get, clear, or summarize stored agent memory |
| `shell` | Set the shell execution profile in `config.toml` (restart required) |
| `mcp` | Run Miroclaw as an MCP tool server (stdio or HTTP) |
| `config` | Export machine-readable config schema |
| `update` | Check for and install binary releases |
| `self-test` | Run installation self-tests (optional `--quick` to skip network) |
| `completions` | Generate shell completion scripts to stdout |
| `hands` | List or run autonomous hand packages under `~/.miroclaw/hands/` |
| `agents` | Manage agent profiles (registry): list, create, use, show, worker |
| `desktop` | Launch or install the companion desktop app |

Builds compiled with the **`plugins-wasm`** Cargo feature also expose `plugin` (WASM plugin lifecycle); it is omitted from stock release help.

## Command Groups

### `onboard`

- `miroclaw onboard`
- `miroclaw onboard --channels-only`
- `miroclaw onboard --force`
- `miroclaw onboard --reinit`
- `miroclaw onboard --api-key <KEY> --provider <ID> --memory <sqlite|lucid|markdown|none>`
- `miroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none>`
- `miroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none> --force`

`onboard` safety behavior:

- If `config.toml` already exists, onboarding offers two modes:
  - Full onboarding (overwrite `config.toml`)
  - Provider-only update (update provider/model/API key while preserving existing channels, tunnel, memory, hooks, and other settings)
- In non-interactive environments, existing `config.toml` causes a safe refusal unless `--force` is passed.
- Use `miroclaw onboard --channels-only` when you only need to rotate channel tokens/allowlists.
- Use `miroclaw onboard --reinit` to start fresh. This backs up your existing config directory with a timestamp suffix and creates a new configuration from scratch.

### `agent`

- `miroclaw agent`
- `miroclaw agent -m "Hello"`
- `miroclaw agent --provider <ID> --model <MODEL> --temperature <0.0-2.0>`
- `miroclaw agent --peripheral <board:path>`

Tip:

- In interactive chat, you can ask for route changes in natural language (for example “conversation uses kimi, coding uses gpt-5.3-codex”); the assistant can persist this via tool `model_routing_config`.

### `gateway` / `daemon`

Gateway:

- `miroclaw gateway` — if no subcommand is given, starts the gateway using **`[gateway].host` / `[gateway].port`** from config only (no extra CLI flags on the bare command).
- `miroclaw gateway start [--port <PORT>] [--host <HOST>]`
- `miroclaw gateway restart [--port <PORT>] [--host <HOST>]` — tries graceful shutdown of an existing instance on that address, then starts.
- `miroclaw gateway get-paircode [--new]` — read or rotate pairing code from a **running** gateway.

Daemon:

- `miroclaw daemon [--host <HOST>] [--port <PORT>]` — long-running runtime. When `[hub].enabled = true`, starts **hub supervisor** (public gateway + all enabled agent workers). Otherwise runs legacy single-process gateway + channels.

### `agents`

Agent profile registry (`registry.toml`). See [multi-agent-profiles.md](../../setup-guides/multi-agent-profiles.md).

- `miroclaw agents list` — list registered profiles ( `*` marks active)
- `miroclaw agents create <name> [--from <existing>]` — scaffold profile + registry entry
- `miroclaw agents use <name>` — set `active_agent.toml` for CLI default config dir
- `miroclaw agents show <name>` — profile paths and internal port
- `miroclaw agents worker --profile <name> [--port <PORT>]` — run one worker (debug)

### `migrate`

- `miroclaw migrate openclaw [--dry-run]` — import OpenClaw layout into current profile
- `miroclaw migrate profiles [--dry-run]` — move flat `~/.miroclaw/` layout to `profiles/main/` + write registry

### `mcp`

- `miroclaw mcp serve` — stdio MCP (default; newline-delimited JSON-RPC)
- `miroclaw mcp serve --allow-tool <NAME>` — add a tool to the allowlist (repeatable; merged with `[mcp_serve].allowed_tools`)
- `miroclaw mcp serve --transport http [--bind <ADDR>] [--port <PORT>]` — HTTP `POST /mcp` (see [`mcp-serve.md`](../../mcp-serve.md) and `[mcp_serve]` in config)

### `shell`

- `miroclaw shell profile safe` — set `[shell].profile` to `safe` (writes `config.toml`)
- `miroclaw shell profile balanced` / `miroclaw shell profile autonomous` — same for built-in tiers
- Custom ids must exist under `[[shell.profiles]]` in config (validated before save)

Restart the gateway or agent after changing profile; the engine reads the profile at process start only.

### `estop`

- `miroclaw estop` (engage `kill-all`)
- `miroclaw estop --level network-kill`
- `miroclaw estop --level domain-block --domain "*.chase.com" [--domain "*.paypal.com"]`
- `miroclaw estop --level tool-freeze --tool shell [--tool browser]`
- `miroclaw estop status`
- `miroclaw estop resume`
- `miroclaw estop resume --network`
- `miroclaw estop resume --domain "*.chase.com"`
- `miroclaw estop resume --tool shell`
- `miroclaw estop resume --otp <123456>`

Notes:

- `estop` commands require `[security.estop].enabled = true`.
- When `[security.estop].require_otp_to_resume = true`, `resume` requires OTP validation.
- OTP prompt appears automatically if `--otp` is omitted.

### `service`

- `miroclaw service install`
- `miroclaw service start`
- `miroclaw service stop`
- `miroclaw service restart`
- `miroclaw service status`
- `miroclaw service uninstall`

Service commands accept `--service-init auto|systemd|openrc` (default `auto`) to pin the init backend.

### `cron`

- `miroclaw cron list`
- `miroclaw cron add <expr> [--tz <IANA_TZ>] [--agent] [--allowed-tool <NAME> …] <command-or-prompt>`
- `miroclaw cron add-at <rfc3339_timestamp> [--agent] [--allowed-tool <NAME> …] <command-or-prompt>`
- `miroclaw cron add-every <every_ms> [--agent] [--allowed-tool <NAME> …] <command-or-prompt>`
- `miroclaw cron once <delay> [--agent] [--allowed-tool <NAME> …] <command-or-prompt>` — delay examples: `30m`, `2h`, `1d`
- `miroclaw cron update <id> [--expression <EXPR>] [--tz <TZ>] [--command <CMD>] [--name <NAME>] [--allowed-tool <NAME> …]`
- `miroclaw cron remove <id>`
- `miroclaw cron pause <id>`
- `miroclaw cron resume <id>`

Notes:

- Mutating schedule/cron actions require `cron.enabled = true`.
- Use `--agent` so the payload is treated as an **agent prompt** instead of a shell string (repeatable `--allowed-tool` applies only to agent jobs).
- Shell command payloads for schedule creation (`add` / `add-at` / `add-every` / `once`) are validated by security command policy before job persistence. The scheduler also applies **`[shell]` Safe-tier** string checks (`[shell.safe].forbidden_paths` + null-byte rule), independent of `shell.profile`, while using `shell.timeout_secs` and `shell.login_shell` when the job runs.

### `models`

- `miroclaw models refresh`
- `miroclaw models refresh --provider <ID>`
- `miroclaw models refresh --force`

`models refresh` currently supports live catalog refresh for provider IDs: `openrouter`, `openai`, `anthropic`, `groq`, `mistral`, `deepseek`, `xai`, `together-ai`, `gemini`, `ollama`, `llamacpp`, `sglang`, `vllm`, `astrai`, `venice`, `fireworks`, `cohere`, `moonshot`, `glm`, `zai`, `qwen`, and `nvidia`.

### `doctor`

- `miroclaw doctor`
- `miroclaw doctor query-engine` — in-process QueryEngine transition tail, last system-prompt assembly, layered-memory selector stats (when enabled), last post-compaction **memory injection** timestamp, and a short preview of the latest **session-memory summary** from consolidation (process-local).
- `miroclaw doctor models [--provider <ID>] [--use-cache]`
- `miroclaw doctor traces [--limit <N>] [--event <TYPE>] [--contains <TEXT>]`
- `miroclaw doctor traces --id <TRACE_ID>`
- `miroclaw doctor long-run [HAND]` — optional `HAND` is the TOML stem under `~/.miroclaw/hands` (omit to scan every hand). For each selected hand, checks coordinator scratchpad freshness (`decisions.md` / `final_summary.md`), workspace AutoMemory index age when `[memory.layered]` is on, and whether the assembled hand system prompt still contains `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` (Phase 1 cache split).

`doctor traces` reads runtime tool/model diagnostics from `observability.runtime_trace_path`.

### `channel`

- `miroclaw channel list`
- `miroclaw channel start`
- `miroclaw channel doctor`
- `miroclaw channel bind-telegram <IDENTITY>`
- `miroclaw channel add <type> <json>`
- `miroclaw channel remove <name>`

Runtime in-chat commands (Telegram/Discord while channel server is running):

- `/models`
- `/models <provider>`
- `/model`
- `/model <model-id>`
- `/new`

Channel runtime also watches `config.toml` and hot-applies updates to:
- `default_provider`
- `default_model`
- `default_temperature`
- `api_key` / `api_url` (for the default provider)
- `reliability.*` provider retry settings

`add/remove` currently route you back to managed setup/manual config paths (not full declarative mutators yet).

### `integrations`

- `miroclaw integrations info <name>`

### `skills`

- `miroclaw skills list`
- `miroclaw skills audit <source_or_name>`
- `miroclaw skills install <source>`
- `miroclaw skills remove <name>`

`<source>` accepts git remotes (`https://...`, `http://...`, `ssh://...`, and `git@host:owner/repo.git`) or a local filesystem path.

`skills install` always runs a built-in static security audit before the skill is accepted. The audit blocks:
- symlinks inside the skill package
- script-like files (`.sh`, `.bash`, `.zsh`, `.ps1`, `.bat`, `.cmd`)
- high-risk command snippets (for example pipe-to-shell payloads)
- markdown links that escape the skill root, point to remote markdown, or target script files

Use `skills audit` to manually validate a candidate skill directory (or an installed skill by name) before sharing it.

Skill manifests (`SKILL.toml`) support `prompts` and `[[tools]]`; both are injected into the agent system prompt at runtime, so the model can follow skill instructions without manually reading skill files.

### `migrate`

- `miroclaw migrate openclaw [--source <path>] [--dry-run]`

### `auth`

- `miroclaw auth login --provider <openai-codex|gemini> [--profile <NAME>] [--device-code]`
- `miroclaw auth login --provider openai-codex --import <PATH>` (import existing `auth.json`; conflicts with `--device-code`)
- `miroclaw auth paste-redirect --provider openai-codex [--profile <NAME>] [--input <URL_OR_CODE>]`
- `miroclaw auth paste-token --provider anthropic [--profile <NAME>] [--token <VALUE>] [--auth-kind <authorization|api-key>]` (token omitted → interactive prompt)
- `miroclaw auth setup-token --provider anthropic [--profile <NAME>]` — alias for `paste-token` oriented at interactive setup
- `miroclaw auth refresh --provider openai-codex [--profile <NAME>]`
- `miroclaw auth logout --provider <NAME> [--profile <NAME>]`
- `miroclaw auth use --provider <NAME> --profile <NAME>`
- `miroclaw auth list`
- `miroclaw auth status`

Use `miroclaw auth <subcommand> --help` for the full flag set.

### `memory`

- `miroclaw memory list` (filters: see `miroclaw memory list --help`)
- `miroclaw memory get <key>`
- `miroclaw memory stats`
- `miroclaw memory clear` (scopes and `--yes`; see `--help`)

### `config`

- `miroclaw config schema`

`config schema` prints a JSON Schema (draft 2020-12) for the full `config.toml` contract to stdout.

### `update`

- `miroclaw update` — download and install latest release (with confirmation)
- `miroclaw update --check` — check only
- `miroclaw update --force` — skip confirmation
- `miroclaw update --version <SEMVER>` — install a specific version

### `self-test`

- `miroclaw self-test` — full suite (includes network-oriented checks when available)
- `miroclaw self-test --quick` — skip network checks

### `hands`

- `miroclaw hands list`
- `miroclaw hands run <name>` — `name` is the `name` field from a hand TOML under `~/.miroclaw/hands/`

### `desktop`

- `miroclaw desktop` — launch the companion app
- `miroclaw desktop --install` — download and install the pre-built app for this platform

### `completions`

- `miroclaw completions bash`
- `miroclaw completions fish`
- `miroclaw completions zsh`
- `miroclaw completions powershell`
- `miroclaw completions elvish`

`completions` is stdout-only by design so scripts can be sourced directly without log/warning contamination.

### `hardware`

- `miroclaw hardware discover`
- `miroclaw hardware introspect <path>`
- `miroclaw hardware info [--chip <chip_name>]`

### `peripheral`

- `miroclaw peripheral list`
- `miroclaw peripheral add <board> <path>`
- `miroclaw peripheral flash [--port <serial_port>]`
- `miroclaw peripheral setup-uno-q [--host <ip_or_host>]`
- `miroclaw peripheral flash-nucleo`

## Validation Tip

To verify docs against your current binary quickly:

```bash
miroclaw --help
miroclaw --config-dir /path/to/workspace --help
miroclaw <command> --help
```
