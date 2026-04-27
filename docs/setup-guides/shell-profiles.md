# Shell profiles (user guide)

ZeroClaw routes **every shell invocation** (the main `shell` tool, skill-driven shell, and related paths) through a single **shell engine**. The engine picks validators and rewriters from your active **profile**, then applies the global **security policy** (allowlists, workspace rules, rate limits, and so on). This guide explains how to choose a profile, switch it safely, and what to expect from each tier.

For exact TOML keys and defaults, see [config-reference.md](../reference/api/config-reference.md). For the CLI surface, see [commands-reference.md](../reference/cli/commands-reference.md) (`zeroclaw shell`).

## Why profiles exist

Profiles trade **freedom vs. guardrails** for the *shell string* the model sends. They do **not** replace your security policy: both layers apply. A stricter profile reduces risky command shapes; a looser profile is for trusted setups where you accept more agent flexibility.

## Built-in profiles

| Profile | Role |
|--------|------|
| **safe** | Default. Minimal extra checks beyond your security policy and global rules (for example extra forbidden paths from `[shell.safe]`). |
| **balanced** | Adds a **256 KiB** cap on the raw command string and keeps the balanced pipeline ready for future options (see `[shell.balanced]` in config reference). |
| **autonomous** | Same baseline as balanced, plus autonomous-only tuning in `[shell.autonomous]`. With a build that enables the **`shell-full`** Cargo feature, an extra pattern-based validator may run (see below). |

Profile names are matched case-insensitively after trim.

## Custom profiles

You can define profiles that **extend** a built-in tier:

```toml
[shell]
profile = "my-tier"

[[shell.profiles]]
id = "my-tier"
extends = "balanced"
```

`extends` must be exactly `safe`, `balanced`, or `autonomous`. The active `profile` must be one of those three **or** match a `[[shell.profiles]]` entry `id`.

## Configure in `config.toml`

Typical shape:

```toml
[shell]
profile = "safe"
timeout_secs = 60
login_shell = false

[shell.safe]
forbidden_paths = []

[shell.balanced]
# snapshot_enabled reserved for future use

[shell.autonomous]
max_validators = 64
spill_threshold_bytes = 0  # reserved; engine still truncates at ~1 MB
```

- **`timeout_secs`** — Wall-clock limit per shell run (must be greater than zero).
- **`login_shell`** — When true, Unix uses `sh -lc` instead of `sh -c` so login-style environment is loaded (same flag affects cron helper spawn style on Unix).
- **`[shell.safe].forbidden_paths`** — Extra path substrings rejected in shell commands (in addition to policy).

Invalid combinations (unknown profile id, empty profile, bad `extends`) fail **config validation** at load time.

## Switching profile

### Option A: CLI (recommended)

```bash
zeroclaw shell profile safe
zeroclaw shell profile balanced
zeroclaw shell profile autonomous
zeroclaw shell profile my-tier
```

The command updates `config.toml` and checks that the profile exists. **Restart** the gateway or agent afterward; the engine reads the resolved profile **at process start**, not on every message.

### Option B: Edit `config.toml`

Set `[shell].profile` to `safe`, `balanced`, `autonomous`, or your custom id, save, then restart.

### Option C: Environment override

`MIROCLAW_SHELL_PROFILE` overrides the configured profile for that process (useful in tests or one-off launches). Restart still applies when you change env for a long-running service.

## Migration from `[shell_tool]`

Older configs used `[shell_tool]`. On load, if **`[shell]` is missing** and `[shell_tool]` is present, ZeroClaw **merges** legacy values into `[shell]` and may write a backup `config.toml.bak.<unix_ts>`. New installs should use **`[shell]`** only.

## What runs through the engine

- The **`shell`** tool used by the agent.
- **Skill** shell execution (same engine instance).
- **Cron** scheduling uses a helper that always applies **Safe-tier** shell string checks (`[shell.safe].forbidden_paths` + null-byte rule) **regardless of `shell.profile`**, while still using `[shell].login_shell` and `[shell].timeout_secs` when the job runs. The scheduler still enforces the broader security model for jobs—do not assume cron is “weaker” than the rest of the stack.

## `shell-full` builds

Some autonomous validators are compiled only when ZeroClaw is built with the **`shell-full`** feature. If you run a stock binary without that feature, autonomous mode still applies balanced-style limits and autonomous config, but not the optional extra validators tied to that feature.

## Operational checklist

1. Pick **safe** unless you know you need more headroom.
2. Set profile via **`zeroclaw shell profile …`** or TOML.
3. Run **`zeroclaw doctor`** / **`zeroclaw status`** if something refuses to start after edits.
4. **Restart** the process that hosts the agent/gateway.
5. If a command is blocked, read the tool output and your **[security policy](../reference/api/config-reference.md)** (allowlists, workspace-only paths, etc.)—the profile is only one layer.

## Troubleshooting

| Symptom | What to check |
|--------|----------------|
| Startup error about `shell.profile` | Typo, custom profile missing from `[[shell.profiles]]`, or empty `profile`. |
| Change has no effect | Forgot restart; or override env still set. |
| Command blocked unexpectedly | Policy allowlists / forbidden paths first; then profile tier. |
| Autonomous “feels” same as without extras | Your binary may not include **`shell-full`**. |

## Related docs

- [config-reference.md](../reference/api/config-reference.md) — `[shell]` schema and migration notes.
- [commands-reference.md](../reference/cli/commands-reference.md) — `zeroclaw shell profile`.
- [toolings.md](../toolings.md) — High-level tool stack and shell wiring.
