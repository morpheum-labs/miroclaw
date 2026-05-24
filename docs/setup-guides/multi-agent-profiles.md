# Multi-Agent Profiles & Hub Setup

Step-by-step guide for running multiple isolated agents under one Miroclaw home directory with a public WebSocket hub.

Last verified: **May 24, 2026**.

## Prerequisites

- Miroclaw installed (`miroclaw --version`)
- At least one provider API key or subscription auth profile
- Optional: separate Telegram/Discord tokens per agent profile

## 1. Migrate or scaffold profiles

**Existing flat install** (`~/.miroclaw/config.toml` + `workspace/`):

```bash
miroclaw migrate profiles --dry-run
miroclaw migrate profiles
```

**Fresh install:**

```bash
miroclaw onboard
miroclaw agents create main   # if registry is empty
```

Verify:

```bash
miroclaw agents list
miroclaw agents show main
```

## 2. Add more agents

```bash
miroclaw agents create researcher --from main
miroclaw agents create support --from main
```

Edit each profile:

- `~/.miroclaw/profiles/researcher/config.toml` — model, system behavior
- `~/.miroclaw/profiles/researcher/workspace/` — `IDENTITY.md`, skills, memory
- Channel tokens live **in that profile's config** (`[channels.telegram]`, etc.)

Set active CLI target:

```bash
miroclaw agents use researcher
```

## 3. Enable hub supervisor

Edit `~/.miroclaw/config.toml` (hub root — not a profile file):

```toml
[hub]
enabled = true

[gateway]
host = "127.0.0.1"
port = 8080
require_pairing = true
allow_public_bind = false
```

Profile configs under `profiles/*/config.toml` should **not** duplicate public gateway bind when using hub mode; workers force `127.0.0.1` and `internal_port` from `registry.toml`.

## 4. Start supervisor

```bash
miroclaw daemon
```

Expected log summary:

- Public gateway URL (hub)
- List of agent workers with internal ports (`main:18080`, …)

Install as OS service (optional):

```bash
miroclaw service install
miroclaw service start
```

## 5. Connect clients

**REST discovery:**

```bash
curl -s http://127.0.0.1:8080/api/agents
curl -s http://127.0.0.1:8080/api/health
```

**WebSocket (hub):**

1. Open `ws://127.0.0.1:8080/ws/chat` with pairing token if required.
2. Send connect frame **with `agent_id`**:

   ```json
   {"type":"connect","agent_id":"main","session_id":"my-session-1","last_event_seq":0}
   ```

3. Send chat messages: `{"type":"message","content":"Hello"}`.
4. Switch agent: `{"type":"switch_agent","agent_id":"researcher"}`.

See [agent-profile-hub.md](../architecture/agent-profile-hub.md) for full protocol and attach/monitor flows.

## 6. Clawgotcha (optional)

Each profile needs a unique `[clawgotcha].instance_name` in its profile `config.toml`. Remote agent definitions from Clawgotcha create registry entries and profile directories automatically.

Hub-level Clawgotcha coordinator config (if used) stays in root `config.toml`; agent runtime sync runs per profile. See [clawgotcha.md](../reference/integrations/clawgotcha.md).

## Troubleshooting

| Symptom | Check |
|---|---|
| `hub enabled but registry has no agents` | `miroclaw agents create main` |
| `AGENT_ID_REQUIRED` on WebSocket | Send `agent_id` in connect frame |
| Channel messages on wrong agent | Channel config must be in the **owning** profile only |
| Delegate unknown agent | Name must exist in registry or profile `[agents]` |
| Port conflict on startup | Edit `internal_port` in `registry.toml` (unique per agent) |

More: [troubleshooting.md](../ops/troubleshooting.md).

## Related

- Architecture: [agent-profile-hub.md](../architecture/agent-profile-hub.md)
- Commands: [commands-reference.md](../reference/cli/commands-reference.md#agents)
- Config: [config-reference.md](../reference/api/config-reference.md)
