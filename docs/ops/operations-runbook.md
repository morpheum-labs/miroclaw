# Miroclaw Operations Runbook

This runbook is for operators who maintain availability, security posture, and incident response.

Last verified: **February 18, 2026**.

## Scope

Use this document for day-2 operations:

- starting and supervising runtime
- health checks and diagnostics
- safe rollout and rollback
- incident triage and recovery

For first-time installation, start from [one-click-bootstrap.md](../setup-guides/one-click-bootstrap.md).

## Runtime Modes

| Mode | Command | When to use |
|---|---|---|
| Foreground runtime | `miroclaw daemon` | local debugging, short-lived sessions |
| Foreground gateway only | `miroclaw gateway` or `miroclaw gateway start` | webhook endpoint testing (same bind; use `start` when passing `--host` / `--port`) |
| User service | `miroclaw service install && miroclaw service start` | persistent operator-managed runtime |
| Docker / Podman | manual container or install script (see below) | containerized deployment |

## Docker / Podman Runtime

If you installed via `bash scripts/install.sh --docker`, the container exits after onboarding. To run
Miroclaw as a long-lived gateway container, start a container manually against the persisted data directory.

The repository root `docker-compose.yml` runs the **Clawgotcha** reference server (not the gateway). For gateway hosting in Docker, use the bootstrap flow or the manual run below; see [one-click-bootstrap.md](../setup-guides/one-click-bootstrap.md#repository-docker-composeyml-clawgotcha).

### Manual container lifecycle (gateway)

```bash
# Start a new container from the bootstrap image
docker run -d --name miroclaw \
  --restart unless-stopped \
  -v "$PWD/.zeroclaw-docker/.zeroclaw:/zeroclaw-data/.zeroclaw" \
  -v "$PWD/.zeroclaw-docker/workspace:/zeroclaw-data/workspace" \
  -e HOME=/zeroclaw-data \
  -e MIROCLAW_WORKSPACE=/zeroclaw-data/workspace \
  -p 42617:42617 \
  miroclaw-bootstrap:local \
  gateway

# Stop (preserves config and workspace)
docker stop miroclaw

# Restart a stopped container
docker start miroclaw

# View logs
docker logs -f miroclaw

# Health check
docker exec miroclaw miroclaw status
```

For Podman, add `--userns keep-id --user "$(id -u):$(id -g)"` and append `:Z` to volume mounts.

### Key detail: do not re-run install.sh to restart

Re-running `bash scripts/install.sh --docker` rebuilds the image and re-runs onboarding. To simply
restart, use `docker start`, `docker compose up -d`, or `podman start`.

For full setup instructions, see [one-click-bootstrap.md](../setup-guides/one-click-bootstrap.md#stopping-and-restarting-a-dockerpodman-container).

## Baseline Operator Checklist

1. Validate configuration:

```bash
miroclaw status
```

2. Verify diagnostics:

```bash
miroclaw doctor
miroclaw channel doctor
```

3. Start runtime:

```bash
miroclaw daemon
```

4. For persistent user session service:

```bash
miroclaw service install
miroclaw service start
miroclaw service status
```

## Health and State Signals

| Signal | Command / File | Expected |
|---|---|---|
| Config validity | `miroclaw doctor` | no critical errors |
| Channel connectivity | `miroclaw channel doctor` | configured channels healthy |
| Runtime summary | `miroclaw status` | expected provider/model/channels |
| Daemon heartbeat/state | `~/.zeroclaw/daemon_state.json` | file updates periodically |

## Logs and Diagnostics

### macOS / Windows (service wrapper logs)

- `~/.zeroclaw/logs/daemon.stdout.log`
- `~/.zeroclaw/logs/daemon.stderr.log`

### Linux (systemd user service)

```bash
journalctl --user -u zeroclaw.service -f
```

## Incident Triage Flow (Fast Path)

1. Snapshot system state:

```bash
miroclaw status
miroclaw doctor
miroclaw channel doctor
```

2. Check service state:

```bash
miroclaw service status
```

3. If service is unhealthy, restart cleanly:

```bash
miroclaw service stop
miroclaw service start
```

4. If channels still fail, verify allowlists and credentials in `~/.zeroclaw/config.toml`.

5. If gateway is involved, verify bind/auth settings (`[gateway]`) and local reachability.

## Safe Change Procedure

Before applying config changes:

1. backup `~/.zeroclaw/config.toml`
2. apply one logical change at a time
3. run `miroclaw doctor`
4. restart daemon/service
5. verify with `status` + `channel doctor`

## Rollback Procedure

If a rollout regresses behavior:

1. restore previous `config.toml`
2. restart runtime (`daemon` or `service`)
3. confirm recovery via `doctor` and channel health checks
4. document incident root cause and mitigation

## Related Docs

- [one-click-bootstrap.md](../setup-guides/one-click-bootstrap.md)
- [troubleshooting.md](./troubleshooting.md)
- [config-reference.md](../reference/api/config-reference.md)
- [commands-reference.md](../reference/cli/commands-reference.md)
