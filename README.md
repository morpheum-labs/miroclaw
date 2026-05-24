<p align="center">
  <img src="https://raw.githubusercontent.com/morpheum-labs/miroclaw/master/docs/assets/miroclaw-banner.png" alt="Miroclaw" width="600" />
</p>

<h1 align="center">🦀 Miroclaw — Personal AI Assistant</h1>

<p align="center">
  <strong>Lean runtime. Full control. 100% Rust. 100% Agnostic.</strong><br>
  ⚡️ <strong>Runs on $10 hardware with <5MB RAM: That's 99% less memory than OpenClaw and 98% cheaper than a Mac mini!</strong>
</p>

<p align="center">
  <a href="LICENSE-APACHE"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache%202.0-blue.svg" alt="License: MIT OR Apache-2.0" /></a>
  <a href="https://github.com/morpheum-labs/miroclaw/graphs/contributors"><img src="https://img.shields.io/github/contributors/morpheum-labs/miroclaw?color=green" alt="Contributors" /></a>
  <a href="https://buymeacoffee.com/argenistherose"><img src="https://img.shields.io/badge/Buy%20Me%20a%20Coffee-Donate-yellow.svg?style=flat&logo=buy-me-a-coffee" alt="Buy Me a Coffee" /></a>
  <a href="https://x.com/miroclawlabs?s=21"><img src="https://img.shields.io/badge/X-%40miroclawlabs-000000?style=flat&logo=x&logoColor=white" alt="X: @miroclawlabs" /></a>
  <a href="https://www.facebook.com/groups/miroclawlabs"><img src="https://img.shields.io/badge/Facebook-Group-1877F2?style=flat&logo=facebook&logoColor=white" alt="Facebook Group" /></a>
  <a href="https://discord.com/invite/wDshRVqRjx"><img src="https://img.shields.io/badge/Discord-Join-5865F2?style=flat&logo=discord&logoColor=white" alt="Discord" /></a>
  <a href="https://www.instagram.com/therealmiroclaw"><img src="https://img.shields.io/badge/Instagram-%40therealmiroclaw-E4405F?style=flat&logo=instagram&logoColor=white" alt="Instagram: @therealmiroclaw" /></a>
  <a href="https://www.tiktok.com/@miroclawlabs"><img src="https://img.shields.io/badge/TikTok-%40miroclawlabs-000000?style=flat&logo=tiktok&logoColor=white" alt="TikTok: @miroclawlabs" /></a>
  <a href="https://www.rednote.com/user/profile/69b735e6000000002603927e"><img src="https://img.shields.io/badge/RedNote-Official-FF2442?style=flat" alt="RedNote" /></a>
  <a href="https://www.reddit.com/r/miroclawlabs/"><img src="https://img.shields.io/badge/Reddit-r%2Fmiroclawlabs-FF4500?style=flat&logo=reddit&logoColor=white" alt="Reddit: r/miroclawlabs" /></a>
</p>

<p align="center">
Built by students and members of the Harvard, MIT, and Sundai.Club communities.
</p>

Miroclaw is a personal AI assistant you run on your own devices. It answers you on the channels you already use (WhatsApp, Telegram, Slack, Discord, Signal, iMessage, Matrix, IRC, Email, Bluesky, Nostr, Mattermost, Nextcloud Talk, DingTalk, Lark, QQ, Reddit, LinkedIn, Twitter, MQTT, WeChat Work, and more). It has a web dashboard for real-time control and can connect to hardware peripherals (ESP32, STM32, Arduino, Raspberry Pi). The Gateway is just the control plane — the product is the assistant.

If you want a personal, single-user assistant that feels local, fast, and always-on, this is it.

<p align="center">
  <a href="https://miroclawlabs.ai">Website</a> ·
  <a href="docs/README.md">Docs</a> ·
  <a href="docs/architecture/agent-profile-hub.md">Architecture</a> ·
  <a href="#quick-start">Getting Started</a> ·
  <a href="#setup-guide">Setup guide</a> ·
  <a href="#multi-agent-profiles--hub">Multi-agent</a> ·
  <a href="#migrating-from-openclaw">Migrating from OpenClaw</a> ·
  <a href="docs/ops/troubleshooting.md">Troubleshoot</a> ·
  <a href="https://discord.com/invite/wDshRVqRjx">Discord</a>
</p>

> **Preferred setup:** run `miroclaw onboard` in your terminal. Miroclaw Onboard guides you step by step through setting up the gateway, workspace, channels, and provider. It is the recommended setup path and works on macOS, Linux, and Windows (via WSL2). New install? Start here: [Getting started](#quick-start)

### Subscription Auth (OAuth)

- **OpenAI Codex** (ChatGPT subscription)
- **Gemini** (Google OAuth)
- **Anthropic** (API key or auth token)

Model note: while many providers/models are supported, for the best experience use the strongest latest-generation model available to you. See [Onboarding](#quick-start).

Models config + CLI: [Providers reference](docs/reference/api/providers-reference.md)
Auth profile rotation (OAuth vs API keys) + failover: [Model failover](docs/reference/api/providers-reference.md)

## Install (recommended)

Runtime: Rust stable toolchain. Single binary, no runtime dependencies.

### Homebrew (macOS/Linuxbrew)

```bash
brew install miroclaw
```

### One-click bootstrap

```bash
git clone https://github.com/morpheum-labs/miroclaw.git
cd miroclaw
bash scripts/install.sh
```

`miroclaw onboard` runs automatically after install to configure your workspace and provider.

## Quick start (TL;DR)

Full beginner guide (auth, pairing, channels): [Getting started](docs/setup-guides/one-click-bootstrap.md)

Detailed command lines (non-interactive install, gateway ports, daemon, auth, completions): [Quick start command reference](docs/setup-guides/quick-start-command-reference.md)

```bash
bash scripts/install.sh
miroclaw gateway
miroclaw agent -m "Hello, Miroclaw!"
miroclaw status
miroclaw doctor
```

Upgrading? Run `miroclaw doctor` after updating.

### From source (development)

```bash
git clone https://github.com/morpheum-labs/miroclaw.git
cd miroclaw

cargo build --release --locked
cargo install --path . --force --locked

miroclaw onboard
```

Build notes, `cargo clean`, and running without a global install (`cargo run --release --`): [Quick start command reference](docs/setup-guides/quick-start-command-reference.md#from-source-development).

## Setup guide

Use this table to pick a path; each link goes deeper.

| Goal | What to do |
|------|------------|
| **First install (recommended)** | Run `miroclaw onboard` after [install](#install-recommended). It creates your workspace, provider, channels, security, memory, and optional **Clawgotcha** control-plane URL + instance name. |
| **Guided walkthrough** | [Getting started](docs/setup-guides/one-click-bootstrap.md) — auth, pairing, channels, gateway. |
| **Commands-only reference** | [Quick start command reference](docs/setup-guides/quick-start-command-reference.md) — ports, daemon, non-interactive install, completions, `cargo run`. |
| **Run the gateway + assistant** | After onboarding: `miroclaw gateway` (or `miroclaw daemon` for gateway + channels + scheduler). With `[hub].enabled`, `daemon` runs hub + per-profile workers — see [multi-agent setup](docs/setup-guides/multi-agent-profiles.md). |
| **Clawgotcha (optional)** | Configure `[clawgotcha]` per profile; upserts create registry profiles. Overview: [Clawgotcha integration](docs/reference/integrations/clawgotcha.md). Wire contract: [clawgotcha-api-contract.md](docs/reference/integrations/clawgotcha-api-contract.md). |
| **Docker** | Build the Miroclaw binary image from the repo root [`Dockerfile`](Dockerfile) when you need a containerized gateway (bring your own workspace/config volume). Run any Clawgotcha-compatible control plane separately if you use `[clawgotcha]`. |

## Migrating from OpenClaw

Miroclaw can import your OpenClaw workspace, memory, and configuration:

```bash
# Preview what will be migrated (safe, read-only)
miroclaw migrate openclaw --dry-run

# Run the migration
miroclaw migrate openclaw
```

This migrates your memory entries, workspace files, and configuration from `~/.openclaw/` to `~/.miroclaw/`. Config is converted from JSON to TOML automatically.

## Multi-agent profiles & hub

Run multiple isolated agents (separate workspaces, channels, and models) under one home directory with a **single public WebSocket** on the hub:

```bash
miroclaw migrate profiles --dry-run   # flat layout → profiles/main/
miroclaw agents create researcher --from main
miroclaw agents list
# Enable [hub].enabled = true in ~/.miroclaw/config.toml, then:
miroclaw daemon
```

Documentation: [Multi-agent setup](docs/setup-guides/multi-agent-profiles.md) · [Architecture](docs/architecture/agent-profile-hub.md) · [Config `[hub]`](docs/reference/api/config-reference.md#hub)

## Security defaults (DM access)

Miroclaw connects to real messaging surfaces. Treat inbound DMs as untrusted input.

Full security guide: [docs/security/README.md](docs/security/README.md)

Default behavior on all channels:

- **DM pairing** (default): unknown senders receive a short pairing code and the bot does not process their message.
- Approve with: `miroclaw pairing approve <channel> <code>` (then the sender is added to a local allowlist).
- Public inbound DMs require an explicit opt-in in `config.toml`.
- Run `miroclaw doctor` to surface risky or misconfigured DM policies.

**Autonomy levels:**

| Level | Behavior |
|-------|----------|
| `ReadOnly` | Agent can observe but not act |
| `Supervised` (default) | Agent acts with approval for medium/high risk operations |
| `Full` | Agent acts autonomously within policy bounds |

**Sandboxing layers:** workspace isolation, path traversal blocking, command allowlisting, forbidden paths (`/etc`, `/root`, `~/.ssh`), rate limiting (max actions/hour, cost/day caps).

<!-- BEGIN:WHATS_NEW -->
<!-- END:WHATS_NEW -->

### 📢 Announcements

Use this board for important notices (breaking changes, security advisories, maintenance windows, and release blockers).

| Date (UTC) | Level       | Notice                                                                                                                                                                                                                                                                                                                                                 | Action                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-02-19 | _Critical_  | We are **not affiliated** with `openagen/zeroclaw`, `zeroclaw.org` or `zeroclaw.net`. The `zeroclaw.org` and `zeroclaw.net` domains previously pointed to third-party forks impersonating this project.                                                                                       | Do not trust information, binaries, fundraising, or announcements from those sources. Use only [this repository](https://github.com/morpheum-labs/miroclaw) and our verified social accounts.                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-02-21 | _Important_ | Our official website is now live: [miroclawlabs.ai](https://miroclawlabs.ai). Thanks for your patience while we prepared the launch. We are still seeing impersonation attempts, so do **not** join any investment or fundraising activity claiming the Miroclaw name unless it is published through our official channels.                            | Use [this repository](https://github.com/morpheum-labs/miroclaw) as the single source of truth. Follow [X (@miroclawlabs)](https://x.com/miroclawlabs?s=21), [Facebook (Group)](https://www.facebook.com/groups/miroclawlabs), and [Reddit (r/miroclawlabs)](https://www.reddit.com/r/miroclawlabs/) for official updates. |
| 2026-02-19 | _Important_ | Anthropic updated the Authentication and Credential Use terms on 2026-02-19. Claude Code OAuth tokens (Free, Pro, Max) are intended exclusively for Claude Code and Claude.ai; using OAuth tokens from Claude Free/Pro/Max in any other product, tool, or service (including Agent SDK) is not permitted and may violate the Consumer Terms of Service. | Please temporarily avoid Claude Code OAuth integrations to prevent potential loss. Original clause: [Authentication and Credential Use](https://code.claude.com/docs/en/legal-and-compliance#authentication-and-credential-use).                                                                                                                                                                                                                                                                                                                                                                                    |

## Highlights

- **Lean Runtime by Default** — common CLI and status workflows run in a few-megabyte memory envelope on release builds.
- **Cost-Efficient Deployment** — designed for $10 boards and small cloud instances, no heavyweight runtime dependencies.
- **Fast Cold Starts** — single-binary Rust runtime keeps command and daemon startup near-instant.
- **Portable Architecture** — one binary across ARM, x86, and RISC-V with swappable providers/channels/tools.
- **Local-first Gateway** — single control plane for sessions, channels, tools, cron, SOPs, and events.
- **Multi-agent profiles & hub** — run isolated agents (separate workspaces, channels, models) under one home directory with a single public WebSocket on the hub.
- **Multi-channel inbox** — WhatsApp, Telegram, Slack, Discord, Signal, iMessage, Matrix, IRC, Email, Bluesky, Nostr, Mattermost, Nextcloud Talk, DingTalk, Lark, QQ, Reddit, LinkedIn, Twitter, MQTT, WeChat Work, WebSocket, and more.
- **Multi-agent orchestration (Hands)** — autonomous agent swarms that run on schedule and grow smarter over time.
- **Standard Operating Procedures (SOPs)** — event-driven workflow automation with MQTT, webhook, cron, and peripheral triggers.
- **Web Dashboard** — React 19 + Vite web UI with real-time chat, memory browser, config editor, cron manager, and tool inspector.
- **Hardware peripherals** — ESP32, STM32 Nucleo, Arduino, Raspberry Pi GPIO via the `Peripheral` trait.
- **First-class tools** — shell, file I/O, browser, git, web fetch/search, MCP, Jira, Notion, Google Workspace, and 70+ more.
- **Lifecycle hooks** — intercept and modify LLM calls, tool executions, and messages at every stage.
- **Skills platform** — bundled, community, and workspace skills with security auditing.
- **Tunnel support** — Cloudflare, Tailscale, ngrok, OpenVPN, and custom tunnels for remote access.

### Why teams pick Miroclaw

- **Lean by default:** small Rust binary, fast startup, low memory footprint.
- **Secure by design:** pairing, strict sandboxing, explicit allowlists, workspace scoping.
- **Fully swappable:** core systems are traits (providers, channels, tools, memory, tunnels).
- **No lock-in:** OpenAI-compatible provider support + pluggable custom endpoints.

## Benchmark Snapshot (Miroclaw vs OpenClaw, Reproducible)

Local machine quick benchmark (macOS arm64, Feb 2026) normalized for 0.8GHz edge hardware.

|                           | OpenClaw      | NanoBot        | PicoClaw        | Miroclaw 🦀          |
| ------------------------- | ------------- | -------------- | --------------- | -------------------- |
| **Language**              | TypeScript    | Python         | Go              | **Rust**             |
| **RAM**                   | > 1GB         | > 100MB        | < 10MB          | **< 5MB**            |
| **Startup (0.8GHz core)** | > 500s        | > 30s          | < 1s            | **< 10ms**           |
| **Binary Size**           | ~28MB (dist)  | N/A (Scripts)  | ~8MB            | **~8.8 MB**          |
| **Cost**                  | Mac Mini $599 | Linux SBC ~$50 | Linux Board $10 | **Any hardware $10** |

> Notes: Miroclaw results are measured on release builds using `/usr/bin/time -l`. OpenClaw requires Node.js runtime (typically ~390MB additional memory overhead), while NanoBot requires Python runtime. PicoClaw and Miroclaw are static binaries. The RAM figures above are runtime memory; build-time compilation requirements are higher.

<p align="center">
  <img src="docs/assets/zeroclaw-comparison.jpeg" alt="Miroclaw vs OpenClaw Comparison" width="800" />
</p>

### Reproducible local measurement

```bash
cargo build --release
ls -lh target/release/miroclaw
```

`/usr/bin/time -l` profiling for the binary: [Quick start command reference](docs/setup-guides/quick-start-command-reference.md#benchmarking-local-memory--startup).

## Everything we built so far

### Core platform

- Gateway HTTP/WS/SSE control plane with sessions, presence, config, cron, webhooks, web dashboard, and pairing.
- Hub supervisor (`[hub].enabled`) — public gateway + per-profile agent workers on localhost internal ports.
- CLI surface: `gateway`, `agent`, `agents`, `onboard`, `doctor`, `status`, `service`, `migrate`, `auth`, `cron`, `channel`, `skills`.
- Agent orchestration loop with tool dispatch, prompt construction, message classification, and memory loading.
- Session model with security policy enforcement, autonomy levels, and approval gating.
- Resilient provider wrapper with failover, retry, and model routing across 20+ LLM backends.

### Channels

Channels: WhatsApp (native), Telegram, Slack, Discord, Signal, iMessage, Matrix, IRC, Email, Bluesky, DingTalk, Lark, Mattermost, Nextcloud Talk, Nostr, QQ, Reddit, LinkedIn, Twitter, MQTT, WeChat Work, WATI, Mochat, Linq, Notion, WebSocket, ClawdTalk.

Feature-gated: Matrix (`channel-matrix`), Lark (`channel-lark`), Nostr (`channel-nostr`).

### Web dashboard

React 19 + Vite 6 + Tailwind CSS 4 web dashboard served directly from the Gateway:

- **Dashboard** — system overview, health status, uptime, cost tracking
- **Agent Chat** — interactive chat with the agent
- **Memory** — browse and manage memory entries
- **Config** — view and edit configuration
- **Cron** — manage scheduled tasks
- **Tools** — browse available tools
- **Logs** — view agent activity logs
- **Cost** — token usage and cost tracking
- **Doctor** — system health diagnostics
- **Integrations** — integration status and setup
- **Pairing** — device pairing management

**Dashboard:** set `[webui].external_path` in `config.toml` (or `MIROCLAW_WEBUI_EXTERNAL_PATH`, or `miroclaw gateway start --webui-external-path /path/to/web/dist`) to serve the built UI from disk. Use `POST /api/webui/reload` or `/webui reload` in gateway chat to re-scan the path without restarting. To run API-only, set `[webui].disabled` or `MIROCLAW_WEBUI_DISABLED`.

### Firmware targets

| Target | Platform | Purpose |
|--------|----------|---------|
| ESP32 | Espressif ESP32 | Wireless peripheral agent |
| ESP32-UI | ESP32 + Display | Agent with visual interface |
| STM32 Nucleo | STM32 (ARM Cortex-M) | Industrial peripheral |
| Arduino | Arduino | Basic sensor/actuator bridge |
| Uno Q Bridge | Arduino Uno | Serial bridge to agent |

### Tools + automation

- **Core:** shell, file read/write/edit, git operations, glob search, content search
- **Web:** browser control, web fetch, web search, screenshot, image info, PDF read
- **Integrations:** Jira, Notion, Google Workspace, Microsoft 365, LinkedIn, Composio, Pushover, Weather (wttr.in)
- **MCP:** Model Context Protocol tool wrapper + deferred tool sets
- **Scheduling:** cron add/remove/update/run, schedule tool
- **Memory:** recall, store, forget, knowledge, project intel
- **Advanced:** delegate (agent-to-agent), swarm, model switch/routing, security ops, cloud ops
- **Hardware:** board info, memory map, memory read (feature-gated)

### Runtime + safety

- **Autonomy levels:** ReadOnly, Supervised (default), Full.
- **Sandboxing:** workspace isolation, path traversal blocking, command allowlists, forbidden paths, Landlock (Linux), Bubblewrap.
- **Rate limiting:** max actions per hour, max cost per day (configurable).
- **Approval gating:** interactive approval for medium/high risk operations.
- **E-stop:** emergency shutdown capability.
- **129+ security tests** in automated CI.

### Ops + packaging

- Web dashboard served directly from the Gateway.
- Tunnel support: Cloudflare, Tailscale, ngrok, OpenVPN, custom command.
- Docker runtime adapter for containerized execution.
- CI/CD: beta (auto on push) → stable (manual dispatch) → Docker, crates.io, Scoop, AUR, Homebrew, tweet.
- Pre-built binaries for Linux (x86_64, aarch64, armv7), macOS (x86_64, aarch64), Windows (x86_64).


## Configuration

Minimal `~/.miroclaw/config.toml` (single-profile install):

```toml
default_provider = "anthropic"
api_key = "sk-ant-..."
```

With multiple agents, each profile has its own `profiles/<name>/config.toml`; the hub root holds `[hub]` and public gateway bind settings. See [multi-agent setup](docs/setup-guides/multi-agent-profiles.md).

Full configuration reference: [docs/reference/api/config-reference.md](docs/reference/api/config-reference.md).

### Channel configuration

**Telegram:**
```toml
[channels.telegram]
bot_token = "123456:ABC-DEF..."
```

**Discord:**
```toml
[channels.discord]
token = "your-bot-token"
```

**Slack:**
```toml
[channels.slack]
bot_token = "xoxb-..."
app_token = "xapp-..."
```

**WhatsApp:**
```toml
[channels.whatsapp]
enabled = true
```

**Matrix:**
```toml
[channels.matrix]
homeserver_url = "https://matrix.org"
username = "@bot:matrix.org"
password = "..."
```

**Signal:**
```toml
[channels.signal]
phone_number = "+1234567890"
```

### Tunnel configuration

```toml
[tunnel]
kind = "cloudflare"  # or "tailscale", "ngrok", "openvpn", "custom", "none"
```

Details: [Channel reference](docs/reference/api/channels-reference.md) · [Config reference](docs/reference/api/config-reference.md)

### Runtime support (current)

- **`native`** (default) — direct process execution, fastest path, ideal for trusted environments.
- **`docker`** — full container isolation, enforced security policies, requires Docker.

Set `runtime.kind = "docker"` for strict sandboxing or network isolation.

## Subscription Auth (OpenAI Codex / Claude Code / Gemini)

Miroclaw supports subscription-native auth profiles (multi-account, encrypted at rest).

- Store file: `~/.miroclaw/auth-profiles.json`
- Encryption key: `~/.miroclaw/.secret_key`
- Profile id format: `<provider>:<profile_name>` (example: `openai-codex:work`)

Login, refresh, and `miroclaw agent --provider …` examples: [Quick start command reference](docs/setup-guides/quick-start-command-reference.md#subscription-auth-oauth--tokens).

## Agent workspace + skills

Each agent profile owns its workspace and config. Layout depends on install mode:

| Mode | Config | Workspace |
|------|--------|-----------|
| **Single profile (default)** | `~/.miroclaw/config.toml` | `~/.miroclaw/workspace/` |
| **Multi-agent profiles** | `~/.miroclaw/profiles/<name>/config.toml` | `~/.miroclaw/profiles/<name>/workspace/` |

Hub root (`~/.miroclaw/config.toml` with `[hub].enabled`) holds supervisor settings; `registry.toml` indexes profiles and internal ports. Set the CLI default with `miroclaw agents use <name>`.

Injected prompt files (under the active profile workspace):
- `IDENTITY.md` — agent personality and role
- `USER.md` — user context and preferences
- `MEMORY.md` — long-term facts and lessons
- `AGENTS.md` — session conventions and initialization rules
- `SOUL.md` — core identity and operating principles

Skills: `<workspace>/skills/<skill>/SKILL.md` or `SKILL.toml`.

```bash
miroclaw agents list
miroclaw skills list
```

Multi-agent setup: [multi-agent-profiles.md](docs/setup-guides/multi-agent-profiles.md). Install, audit, and remove skills: [Quick start command reference](docs/setup-guides/quick-start-command-reference.md#skills).

## CLI commands

Common invocations (gateway, agent, service, channels, cron, memory, migration): [Quick start command reference](docs/setup-guides/quick-start-command-reference.md#cli-cheat-sheet-common-workflows).

Agent profiles:

```bash
miroclaw agents list
miroclaw agents create researcher --from main
miroclaw agents use researcher
miroclaw migrate profiles --dry-run
```

Full commands reference: [docs/reference/cli/commands-reference.md](docs/reference/cli/commands-reference.md)

## Prerequisites

<details>
<summary><strong>Windows</strong></summary>

#### Required

1. **Visual Studio Build Tools** (provides the MSVC linker and Windows SDK):

    ```powershell
    winget install Microsoft.VisualStudio.2022.BuildTools
    ```

    During installation (or via the Visual Studio Installer), select the **"Desktop development with C++"** workload.

2. **Rust toolchain:**

    ```powershell
    winget install Rustlang.Rustup
    ```

    After installation, open a new terminal and run `rustup default stable` to ensure the stable toolchain is active.

3. **Verify** both are working:
    ```powershell
    rustc --version
    cargo --version
    ```

#### Optional

- **Docker Desktop** — required only if using the [Docker sandboxed runtime](#runtime-support-current) (`runtime.kind = "docker"`). Install via `winget install Docker.DockerDesktop`.

</details>

<details>
<summary><strong>Linux / macOS</strong></summary>

#### Required

1. **Build essentials:**
    - **Linux (Debian/Ubuntu):** `sudo apt install build-essential pkg-config`
    - **Linux (Fedora/RHEL):** `sudo dnf group install development-tools && sudo dnf install pkg-config`
    - **macOS:** Install Xcode Command Line Tools: `xcode-select --install`

2. **Rust toolchain:**

    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```

    See [rustup.rs](https://rustup.rs) for details.

3. **Verify** both are working:
    ```bash
    rustc --version
    cargo --version
    ```

#### One-Line Installer

Or skip the steps above and install everything (system deps, Rust, Miroclaw) in a single command:

```bash
curl -LsSf https://raw.githubusercontent.com/morpheum-labs/miroclaw/master/scripts/install.sh | bash
```

#### Compilation resource requirements

Building from source needs more resources than running the resulting binary:

| Resource       | Minimum | Recommended |
| -------------- | ------- | ----------- |
| **RAM + swap** | 2 GB    | 4 GB+       |
| **Free disk**  | 6 GB    | 10 GB+      |

Pre-built install flags (`--prefer-prebuilt`, `--prebuilt-only`, and more): [one-click bootstrap](docs/setup-guides/one-click-bootstrap.md#resource-preflight-and-pre-built-flow).

#### Optional

- **Docker** — required only if using the [Docker sandboxed runtime](#runtime-support-current) (`runtime.kind = "docker"`). Install via your package manager or [docker.com](https://docs.docker.com/engine/install/).

> **Note:** The default `cargo build --release` uses `codegen-units=1` to lower peak compile pressure. For faster builds on powerful machines, use `cargo build --profile release-fast`.

</details>

### Pre-built binaries

Release assets are published for:

- Linux: `x86_64`, `aarch64`, `armv7`
- macOS: `x86_64`, `aarch64`
- Windows: `x86_64`

Download the latest assets from:
<https://github.com/morpheum-labs/miroclaw/releases/latest>

## Docs

Use these when you're past the onboarding flow and want the deeper reference.

- Start with the [docs index](docs/README.md) for navigation and "what's where."
- Read the [agent-profile hub architecture](docs/architecture/agent-profile-hub.md) for multi-agent layout and the public WebSocket hub.
- See the [repository map](docs/maintainers/repo-map.md) and [architecture diagrams](docs/assets/architecture-diagrams.md) for runtime flow and component structure.
- Use the [configuration reference](docs/reference/api/config-reference.md) when you need every key and example.
- Run the Gateway by the book with the [operational runbook](docs/ops/operations-runbook.md).
- Follow [Miroclaw Onboard](#quick-start) for a guided setup.
- Debug common failures with the [troubleshooting guide](docs/ops/troubleshooting.md).
- Review [security guidance](docs/security/README.md) before exposing anything.

### Reference docs

- Documentation hub: [docs/README.md](docs/README.md)
- Agent-profile hub architecture: [docs/architecture/agent-profile-hub.md](docs/architecture/agent-profile-hub.md)
- Commands reference: [docs/reference/cli/commands-reference.md](docs/reference/cli/commands-reference.md)
- Config reference: [docs/reference/api/config-reference.md](docs/reference/api/config-reference.md)
- Providers reference: [docs/reference/api/providers-reference.md](docs/reference/api/providers-reference.md)
- Channels reference: [docs/reference/api/channels-reference.md](docs/reference/api/channels-reference.md)
- Operations runbook: [docs/ops/operations-runbook.md](docs/ops/operations-runbook.md)
- Troubleshooting: [docs/ops/troubleshooting.md](docs/ops/troubleshooting.md)

### Collaboration docs

- Contributor hub: [docs/contributing/README.md](docs/contributing/README.md)
- PR workflow policy: [docs/contributing/pr-workflow.md](docs/contributing/pr-workflow.md)
- CI workflow guide: [docs/contributing/ci-map.md](docs/contributing/ci-map.md)
- Reviewer playbook: [docs/contributing/reviewer-playbook.md](docs/contributing/reviewer-playbook.md)
- Security documentation: [docs/security/README.md](docs/security/README.md)
- Documentation template: [docs/contributing/doc-template.md](docs/contributing/doc-template.md)

### Deployment + operations

- Network deployment guide: [docs/ops/network-deployment.md](docs/ops/network-deployment.md)
- Proxy agent playbook: [docs/ops/proxy-agent-playbook.md](docs/ops/proxy-agent-playbook.md)
- Hardware guides: [docs/hardware/README.md](docs/hardware/README.md)

## Smooth Crab 🦀

Miroclaw was built for the smooth crab 🦀, a fast and efficient AI assistant. Built by Argenis De La Rosa and the community.

- [miroclawlabs.ai](https://miroclawlabs.ai)
- [@miroclawlabs](https://x.com/miroclawlabs)

## Support Miroclaw

If Miroclaw helps your work and you want to support ongoing development, you can donate here:

<a href="https://buymeacoffee.com/argenistherose"><img src="https://img.shields.io/badge/Buy%20Me%20a%20Coffee-Donate-yellow.svg?style=for-the-badge&logo=buy-me-a-coffee" alt="Buy Me a Coffee" /></a>

### 🙏 Special Thanks

A heartfelt thank you to the communities and institutions that inspire and fuel this open-source work:

- **Harvard University** — for fostering intellectual curiosity and pushing the boundaries of what's possible.
- **MIT** — for championing open knowledge, open source, and the belief that technology should be accessible to everyone.
- **Sundai Club** — for the community, the energy, and the relentless drive to build things that matter.
- **The World & Beyond** 🌍✨ — to every contributor, dreamer, and builder out there making open source a force for good. This is for you.

We're building in the open because the best ideas come from everywhere. If you're reading this, you're part of it. Welcome. 🦀❤️

## Contributing

New to Miroclaw? Look for issues labeled [`good first issue`](https://github.com/morpheum-labs/miroclaw/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) — start with [the contributor documentation](docs/contributing/README.md#suggested-reading-order). AI/vibe-coded PRs welcome! 🤖

See [docs/contributing/README.md](docs/contributing/README.md) and the [CLA](docs/contributing/cla.md). Implement a trait, submit a PR:

- CI workflow guide: [docs/contributing/ci-map.md](docs/contributing/ci-map.md)
- New `Provider` → `src/providers/`
- New `Channel` → `src/channels/`
- New `Observer` → `src/observability/`
- New `Tool` → `src/tools/`
- New `Memory` → `src/memory/`
- New `Tunnel` → `src/tunnel/`
- New `Peripheral` → `src/peripherals/`
- New `Skill` → `<profile workspace>/skills/<name>/` (e.g. `~/.miroclaw/profiles/main/workspace/skills/`)

<!-- BEGIN:RECENT_CONTRIBUTORS -->
<!-- END:RECENT_CONTRIBUTORS -->

## ⚠️ Official Repository & Impersonation Warning

**This is the only official Miroclaw repository:**

> https://github.com/morpheum-labs/miroclaw

Any other repository, organization, domain, or package claiming to be "Miroclaw" or implying affiliation with Miroclaw Labs is **unauthorized and not affiliated with this project**. Known unauthorized forks will be listed in [TRADEMARK.md](docs/maintainers/trademark.md).

If you encounter impersonation or trademark misuse, please [open an issue](https://github.com/morpheum-labs/miroclaw/issues).

---

## License

Miroclaw is dual-licensed for maximum openness and contributor protection:

| License | Use case |
|---|---|
| [MIT](LICENSE-MIT) | Open-source, research, academic, personal use |
| [Apache 2.0](LICENSE-APACHE) | Patent protection, institutional, commercial deployment |

You may choose either license. **Contributors automatically grant rights under both** — see [CLA.md](docs/contributing/cla.md) for the full contributor agreement.

### Trademark

The **Miroclaw** name and logo are trademarks of Miroclaw Labs. This license does not grant permission to use them to imply endorsement or affiliation. See [TRADEMARK.md](docs/maintainers/trademark.md) for permitted and prohibited uses.

### Contributor Protections

- You **retain copyright** of your contributions
- **Patent grant** (Apache 2.0) shields you from patent claims by other contributors
- Your contributions are **permanently attributed** in commit history and the repository’s contributor records
- No trademark rights are transferred by contributing

---

**Miroclaw** — Lean runtime. Full control. Deploy anywhere. Swap anything. 🦀

## Contributors

<a href="https://github.com/morpheum-labs/miroclaw/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=morpheum-labs/miroclaw" alt="Miroclaw contributors" />
</a>

This list is generated from the GitHub contributors graph and updates automatically.

## Star History

<p align="center">
  <a href="https://www.star-history.com/#morpheum-labs/miroclaw&type=date&legend=top-left">
    <picture>
     <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=morpheum-labs/miroclaw&type=date&theme=dark&legend=top-left" />
     <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=morpheum-labs/miroclaw&type=date&legend=top-left" />
     <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=morpheum-labs/miroclaw&type=date&legend=top-left" />
    </picture>
  </a>
</p>
