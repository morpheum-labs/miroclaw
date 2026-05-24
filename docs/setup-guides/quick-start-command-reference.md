# Quick start: detailed commands

This page expands on the short command examples in the repository [README.md](../../README.md#quick-start-tldr). Use it when you need flags, non-interactive install, port selection, profiling, auth flows, or a fuller CLI cheat sheet.

Last verified: **April 15, 2026**.

## Install and onboard

Non-interactive bootstrap (API key + provider during install):

```bash
bash scripts/install.sh --api-key "sk-..." --provider openrouter
```

Equivalent with environment variables (see also [one-click-bootstrap.md](one-click-bootstrap.md)):

```bash
MIROCLAW_API_KEY="sk-..." MIROCLAW_PROVIDER="openrouter" bash scripts/install.sh
```

Pre-built binaries and resource-constrained hosts:

```bash
bash scripts/install.sh --prefer-prebuilt
bash scripts/install.sh --prebuilt-only
```

More install modes (Docker, system deps, Rust bootstrap): [one-click-bootstrap.md](one-click-bootstrap.md).

## Gateway and runtime

Start the gateway (webhook server + web dashboard). `miroclaw gateway` is shorthand for `gateway start` and uses `[gateway]` host and port from config (default **127.0.0.1:42617**).

```bash
miroclaw gateway
miroclaw gateway start
```

Bind an ephemeral port (read the bound port from logs or `miroclaw status`):

```bash
miroclaw gateway start --port 0
```

Interactive agent session (REPL-style):

```bash
miroclaw agent
```

Full autonomous runtime (gateway + channels + cron + hands):

```bash
miroclaw daemon
```

**Multi-agent hub** (public WebSocket routes to one agent at a time; channels isolated per profile):

```bash
# See setup guide — enable [hub].enabled, create profiles, then:
miroclaw daemon
```

Setup: [multi-agent-profiles.md](multi-agent-profiles.md). Architecture: [agent-profile-hub.md](../architecture/agent-profile-hub.md).

## Agent profiles (CLI)

```bash
miroclaw agents list
miroclaw agents create researcher --from main
miroclaw agents use main
miroclaw migrate profiles --dry-run
miroclaw migrate profiles
```

## From source (development)

After pulling the latest sources, a normal release build is usually enough. If incremental artifacts confuse the compiler, use `cargo clean && cargo build`.

```bash
git clone https://github.com/morpheum-labs/miroclaw.git
cd miroclaw

cargo build --release --locked
cargo install --path . --force --locked

miroclaw onboard
```

**Dev fallback (no global install):** prefix CLI invocations with `cargo run --release --`, for example:

```bash
cargo run --release -- status
cargo run --release -- agent -m "hello"
```

## Benchmarking (local memory / startup)

```bash
cargo build --release
ls -lh target/release/miroclaw

/usr/bin/time -l target/release/miroclaw --help
/usr/bin/time -l target/release/miroclaw status
```

On Linux, use `time -v` instead of `/usr/bin/time -l` if you prefer GNU `time`.

## Subscription auth (OAuth / tokens)

Auth storage: `~/.zeroclaw/auth-profiles.json`; encryption key: `~/.zeroclaw/.secret_key`. Profile id format: `<provider>:<profile_name>` (example: `openai-codex:work`).

```bash
# OpenAI Codex OAuth (ChatGPT subscription)
miroclaw auth login --provider openai-codex --device-code

# Gemini OAuth
miroclaw auth login --provider gemini --profile default

# Anthropic setup-token
miroclaw auth paste-token --provider anthropic --profile default --auth-kind authorization

# Check / refresh / switch profile
miroclaw auth status
miroclaw auth refresh --provider openai-codex --profile default
miroclaw auth use --provider openai-codex --profile work

# Run the agent with subscription auth
miroclaw agent --provider openai-codex -m "hello"
miroclaw agent --provider anthropic -m "hello"
```

Provider tables and failover: [providers-reference.md](../reference/api/providers-reference.md).

## Skills

```bash
miroclaw skills list
miroclaw skills install https://github.com/user/my-skill.git
miroclaw skills audit https://github.com/user/my-skill.git
miroclaw skills remove my-skill
```

## CLI cheat sheet (common workflows)

Workspace and health:

```bash
miroclaw onboard
miroclaw status
miroclaw doctor
```

Gateway and daemon:

```bash
miroclaw gateway start
miroclaw gateway get-paircode
miroclaw daemon
```

Agent:

```bash
miroclaw agent
miroclaw agent -m "message"
```

Service (launchd / systemd):

```bash
miroclaw service install
miroclaw service start|stop|restart|status
```

Channels:

```bash
miroclaw channel list
miroclaw channel doctor
miroclaw channel bind-telegram 123456789
```

Cron:

```bash
miroclaw cron list
miroclaw cron add "*/5 * * * *" --agent "Check system health"
miroclaw cron remove <id>
```

Memory:

```bash
miroclaw memory list
miroclaw memory get <key>
miroclaw memory stats
```

Auth profiles (API-style):

```bash
miroclaw auth login --provider <name>
miroclaw auth status
miroclaw auth use --provider <name> --profile <profile>
```

Hardware:

```bash
miroclaw hardware discover
miroclaw peripheral list
miroclaw peripheral flash
```

Migration:

```bash
miroclaw migrate openclaw --dry-run
miroclaw migrate openclaw
miroclaw migrate profiles --dry-run
miroclaw migrate profiles
```

Shell completions:

```bash
source <(miroclaw completions bash)
miroclaw completions zsh > ~/.zfunc/_miroclaw
```

For every subcommand and flag, use [commands-reference.md](../reference/cli/commands-reference.md).

## Related

- [one-click-bootstrap.md](one-click-bootstrap.md)
- [commands-reference.md](../reference/cli/commands-reference.md)
- [operations-runbook.md](../ops/operations-runbook.md)
- [troubleshooting.md](../ops/troubleshooting.md)
