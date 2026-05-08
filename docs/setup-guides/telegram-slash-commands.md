# Chat slash commands (channels + gateway) and Telegram control hub

This guide reflects **current ZeroClaw behavior** (not third-party tutorials). **Where to find this:** `docs/setup-guides/telegram-slash-commands.md` (linked from [docs/README.md](../README.md)). Related: [channels-reference.md](../reference/api/channels-reference.md#in-chat-runtime-model-switching-telegram--discord) (runtime switching notes).

## Built-in runtime commands

Handled **before** the LLM on **Telegram, Discord, Matrix, Slack**, and **gateway WebSocket chat** (`/ws/chat`):

| Command | Purpose |
| --- | --- |
| `/new` | Clear session history for this sender/thread and start fresh. |
| `/models` | List providers or switch provider (`/models <provider>`). |
| `/model` | Show or set model id (`/model <id>`). |
| `/config` | Show routing / model summary for this chat. |

These do **not** require the control hub.

### Gateway WebSocket extras (`/ws/chat` only)

The browser/dashboard chat socket accepts the same runtime commands above, plus:

| Command | Purpose |
| --- | --- |
| `/reset`, `/fresh-session` | Same as `/new` (fresh session). |
| `/read` | Usage hint if bare; `/read <workspace-relative-path>` reads a file from disk immediately (subject to file-read policy). |
| `/refresh` | Clears the dynamic-context memo for this workspace; optional `/refresh all` or `/refresh <path>` re-reads one file after clearing. |
| `/webui` | `/webui status` or `/webui help` shows dashboard/source info; `/webui reload` reloads dashboard config from disk. |

### Machine-readable catalog

`GET /api/chat-slash-commands` on the gateway returns JSON describing the WebSocket slash commands (same descriptions as above). If `[gateway].path_prefix` is set, prepend it to the path (for example `/prefix/api/chat-slash-commands`).

## Optional control hub (`/z` by default)

When enabled in `config.toml`:

```toml
[channels_config.telegram]
bot_token = "…"
allowed_users = ["YOUR_TELEGRAM_USER_ID"]
control_hub_enabled = true
# control_hub_prefix = "z"   # optional; default is z
```

Messages such as `/z skills list` or `/z channel doctor` are handled **before** the LLM and run curated operations (see [telegram-control-hub-spec.md](../design/telegram-control-hub-spec.md)).

**Security:** Leave `control_hub_enabled = false` unless you explicitly want Telegram senders who pass `allowed_users` (and pairing, if used) to trigger host management commands.

## Bot command menu (`setMyCommands`)

When the Telegram channel connects, ZeroClaw registers a small default command list (`new`, `models`, `model`, `config`) and, if the hub is enabled, the hub prefix. Skills with `user_invocable = true` may add additional menu rows (names are sanitized for Telegram).

## Skills: `user_invocable` and `prompt_injection_mode`

- **`user_invocable`**: Declared in `SKILL.md` / `SKILL.toml` as described in the design spec. It affects Telegram menu registration only; it does **not** by itself add new hub verbs.
- **`prompt_injection_mode`**: Global setting under `[skills]` in `config.toml` (`full` or `compact`). It is **not** read from per-skill YAML for runtime behavior.

## Misconceptions (common external guides)

- There is **no** `/commands` runtime handler in ZeroClaw; use the Telegram command menu or this doc.
- Slash routing for the hub is **not** inferred from skill prose alone; it requires `control_hub_enabled` and a valid prefix.
