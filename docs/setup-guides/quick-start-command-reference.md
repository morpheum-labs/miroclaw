# Quick start: detailed commands

This page expands on the short command examples in the repository [README.md](../../README.md#quick-start-tldr). Use it when you need flags, non-interactive install, port selection, profiling, auth flows, or a fuller CLI cheat sheet.

Last verified: **April 15, 2026**.

## Install and onboard

Non-interactive bootstrap (API key + provider during install):

```bash
./install.sh --api-key "sk-..." --provider openrouter
```

Equivalent with environment variables (see also [one-click-bootstrap.md](one-click-bootstrap.md)):

```bash
MIROCLAW_API_KEY="sk-..." MIROCLAW_PROVIDER="openrouter" ./install.sh
```

Pre-built binaries and resource-constrained hosts:

```bash
./install.sh --prefer-prebuilt
./install.sh --prebuilt-only
```

More install modes (Docker, system deps, Rust bootstrap): [one-click-bootstrap.md](one-click-bootstrap.md).

## Gateway and runtime

Start the gateway (webhook server + web dashboard). `zeroclaw gateway` is shorthand for `gateway start` and uses `[gateway]` host and port from config (default **127.0.0.1:42617**).

```bash
zeroclaw gateway
zeroclaw gateway start
```

Bind an ephemeral port (read the bound port from logs or `zeroclaw status`):

```bash
zeroclaw gateway start --port 0
```

Interactive agent session (REPL-style):

```bash
zeroclaw agent
```

Full autonomous runtime (gateway + channels + cron + hands):

```bash
zeroclaw daemon
```

## From source (development)

After pulling the latest sources, a normal release build is usually enough. If incremental artifacts confuse the compiler, use `cargo clean && cargo build`.

```bash
git clone https://github.com/zeroclaw-labs/zeroclaw.git
cd zeroclaw

cargo build --release --locked
cargo install --path . --force --locked

zeroclaw onboard
```

**Dev fallback (no global install):** prefix CLI invocations with `cargo run --release --`, for example:

```bash
cargo run --release -- status
cargo run --release -- agent -m "hello"
```

## Benchmarking (local memory / startup)

```bash
cargo build --release
ls -lh target/release/zeroclaw

/usr/bin/time -l target/release/zeroclaw --help
/usr/bin/time -l target/release/zeroclaw status
```

On Linux, use `time -v` instead of `/usr/bin/time -l` if you prefer GNU `time`.

## Subscription auth (OAuth / tokens)

Auth storage: `~/.zeroclaw/auth-profiles.json`; encryption key: `~/.zeroclaw/.secret_key`. Profile id format: `<provider>:<profile_name>` (example: `openai-codex:work`).

```bash
# OpenAI Codex OAuth (ChatGPT subscription)
zeroclaw auth login --provider openai-codex --device-code

# Gemini OAuth
zeroclaw auth login --provider gemini --profile default

# Anthropic setup-token
zeroclaw auth paste-token --provider anthropic --profile default --auth-kind authorization

# Check / refresh / switch profile
zeroclaw auth status
zeroclaw auth refresh --provider openai-codex --profile default
zeroclaw auth use --provider openai-codex --profile work

# Run the agent with subscription auth
zeroclaw agent --provider openai-codex -m "hello"
zeroclaw agent --provider anthropic -m "hello"
```

Provider tables and failover: [providers-reference.md](../reference/api/providers-reference.md).

## Skills

```bash
zeroclaw skills list
zeroclaw skills install https://github.com/user/my-skill.git
zeroclaw skills audit https://github.com/user/my-skill.git
zeroclaw skills remove my-skill
```

## CLI cheat sheet (common workflows)

Workspace and health:

```bash
zeroclaw onboard
zeroclaw status
zeroclaw doctor
```

Gateway and daemon:

```bash
zeroclaw gateway start
zeroclaw gateway get-paircode
zeroclaw daemon
```

Agent:

```bash
zeroclaw agent
zeroclaw agent -m "message"
```

Service (launchd / systemd):

```bash
zeroclaw service install
zeroclaw service start|stop|restart|status
```

Channels:

```bash
zeroclaw channel list
zeroclaw channel doctor
zeroclaw channel bind-telegram 123456789
```

Cron:

```bash
zeroclaw cron list
zeroclaw cron add "*/5 * * * *" --agent "Check system health"
zeroclaw cron remove <id>
```

Memory:

```bash
zeroclaw memory list
zeroclaw memory get <key>
zeroclaw memory stats
```

Auth profiles (API-style):

```bash
zeroclaw auth login --provider <name>
zeroclaw auth status
zeroclaw auth use --provider <name> --profile <profile>
```

Hardware:

```bash
zeroclaw hardware discover
zeroclaw peripheral list
zeroclaw peripheral flash
```

Migration:

```bash
zeroclaw migrate openclaw --dry-run
zeroclaw migrate openclaw
```

Shell completions:

```bash
source <(zeroclaw completions bash)
zeroclaw completions zsh > ~/.zfunc/_zeroclaw
```

For every subcommand and flag, use [commands-reference.md](../reference/cli/commands-reference.md).

## Related

- [one-click-bootstrap.md](one-click-bootstrap.md)
- [commands-reference.md](../reference/cli/commands-reference.md)
- [operations-runbook.md](../ops/operations-runbook.md)
- [troubleshooting.md](../ops/troubleshooting.md)
