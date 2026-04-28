# Clawgotcha server (minimal control plane)

HTTP API aligned with [Clawgotcha API contract](../../docs/reference/integrations/clawgotcha-api-contract.md) and the Miroclaw client in [`crates/clawgotcha`](../clawgotcha).

## Run locally

```bash
cargo run -p clawgotcha-server
```

Default bind: `0.0.0.0:9847` (override with `CLAWGOTCHA_SERVER_PORT`).

## Integration with Miroclaw

Set in `config.toml`:

```toml
[clawgotcha]
enabled = true
url = "http://127.0.0.1:9847"   # or http://127.0.0.1:9847/api if you mount under /api
instance_name = "dev-box"
```

Then start `miroclaw daemon`. The runtime registers on startup and polls `/v1/agents`, `/v1/cron`, and `/v1/swarm/config`.

This crate is a **reference / test** implementation: no authentication, in-memory state only.
