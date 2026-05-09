# Testing Guide

ZeroClaw uses a five-level testing taxonomy with filesystem-based organization.

## Testing Taxonomy

| Level | What it tests | External boundaries | Directory |
|-------|--------------|-------------------|-----------|
| **Unit** | Single function/struct | Everything mocked | `#[cfg(test)]` blocks in `src/**/*.rs` or separate `src/**/tests.rs` files |
| **Component** | One subsystem within its own boundary | Subsystem real, everything else mocked | `tests/component/` |
| **Integration** | Multiple internal components wired together | Real internals, external APIs mocked | `tests/integration/` |
| **System** | Full request→response across ALL internal boundaries | Only external APIs mocked | `tests/system/` |
| **Live** | Full stack with real external services | Nothing mocked, `#[ignore]` | `tests/live/` |

## Directory Structure

| Directory | Level | Description | Run command |
|-----------|-------|-------------|-------------|
| `src/**/*.rs` | Unit | Co-located `#[cfg(test)]` blocks or separate `tests.rs` files alongside source | `cargo test --lib` |
| `tests/component/` | Component | One subsystem, real impl, mocked boundaries | `cargo test --test component` |
| `tests/integration/` | Integration | Multiple components wired together | `cargo test --test integration` |
| `tests/system/` | System | Full channel→agent→channel flow | `cargo test --test system` |
| `tests/live/` | Live | Real external services, `#[ignore]` | `cargo test --test live -- --ignored` |
| `tests/manual/` | — | Human-driven test scripts (shell, Python) | Run directly |
| `tests/support/` | — | Shared mock infrastructure (not a test binary) | — |
| `tests/fixtures/` | — | Test data files (JSON traces, media) | — |

## How to Run Tests

```bash
# Run all tests (unit + component + integration + system)
cargo test

# Run only unit tests
cargo test --lib

# Run component tests
cargo test --test component

# Run integration tests
cargo test --test integration

# Run system tests
cargo test --test system

# Run live tests (requires API credentials)
cargo test --test live -- --ignored

# Filter within a level
cargo test --test integration agent

# Full CI validation
./dev/ci.sh all

# Level-specific CI commands
./dev/ci.sh test-component
./dev/ci.sh test-integration
./dev/ci.sh test-system
```

## How to Add a New Test

1. **Testing one subsystem in isolation?** → `tests/component/`
2. **Testing multiple components together?** → `tests/integration/`
3. **Testing full message flow?** → `tests/system/`
4. **Requires real API keys?** → `tests/live/` with `#[ignore]`

After creating a test file, add it to the appropriate `mod.rs` and use shared infrastructure from `tests/support/`.

## Shared Infrastructure (`tests/support/`)

All test binaries include `mod support;` making shared mocks available via `crate::support::*`.

| Module | Contents |
|--------|----------|
| `mock_provider.rs` | `MockProvider` (FIFO scripted), `RecordingProvider` (captures requests), `TraceLlmProvider` (JSON fixture replay) |
| `mock_tools.rs` | `EchoTool`, `CountingTool`, `FailingTool`, `RecordingTool` |
| `mock_channel.rs` | `TestChannel` (captures sends, records typing events) |
| `helpers.rs` | `make_memory()`, `make_observer()`, `build_agent()`, `text_response()`, `tool_response()`, `StaticMemoryLoader` |
| `trace.rs` | `LlmTrace`, `TraceTurn`, `TraceStep` types + `LlmTrace::from_file()` |
| `assertions.rs` | `verify_expects()` for declarative trace assertion |

### Usage

```rust
use crate::support::{MockProvider, EchoTool, CountingTool};
use crate::support::helpers::{build_agent, text_response, tool_response};
```

## JSON Trace Fixtures

Trace fixtures are canned LLM response scripts stored as JSON files in `tests/fixtures/traces/`. They replace inline mock setup with declarative conversation scripts.

### How it works

1. `TraceLlmProvider` loads a fixture and implements the `Provider` trait
2. Each `provider.chat()` call returns the next step from the fixture in FIFO order
3. Real tools execute normally (e.g., `EchoTool` processes arguments)
4. After all turns, `verify_expects()` checks declarative assertions
5. If the agent calls the provider more times than there are steps, the test fails

### Fixture format

```json
{
  "model_name": "test-name",
  "turns": [
    {
      "user_input": "User message",
      "steps": [
        {
          "response": {
            "type": "text",
            "content": "LLM response",
            "input_tokens": 20,
            "output_tokens": 10
          }
        }
      ]
    }
  ],
  "expects": {
    "response_contains": ["expected text"],
    "tools_used": ["echo"],
    "max_tool_calls": 1
  }
}
```

**Response types**: `"text"` (plain text) or `"tool_calls"` (LLM requests tool execution).

**Expects fields**: `response_contains`, `response_not_contains`, `tools_used`, `tools_not_used`, `max_tool_calls`, `all_tools_succeeded`, `response_matches` (regex).

## Live Test Conventions

- All live tests must be `#[ignore]`
- Use `env::var("MIROCLAW_TEST_*")` for credentials
- Run with `cargo test --test live -- --ignored --nocapture`

## Manual Tests (`tests/manual/`)

Scripts for human-driven testing that can't be automated via `cargo test`:

| Directory/File | What it does |
|---|---|
| `manual/telegram/` | Telegram integration test suite, smoke tests, message generator |
| `manual/test_dockerignore.sh` | Validates `.dockerignore` excludes sensitive paths |

For Telegram-specific testing details, see [testing-telegram.md](./testing-telegram.md).

## Production checklist: cron soft-retire and Clawgotcha sync

Use this before enabling the behavior in production or after changes under `src/cron/store.rs`, `src/cron/scheduler.rs`, `src/clawgotcha_host/glue.rs`, or `[clawgotcha]` wiring.

### Objectives

- Remote cron removal **never hard-deletes** a row while correctness of **`cron_runs`** (FK to `cron_jobs`) and **in-flight** execution matter.
- **`cron_list` / list APIs** show **active** jobs only (`retired_at IS NULL`); retired rows remain for history until explicitly purged or superseded by sync.
- Imperative **`cron remove`** and declarative sync removals **defer** to **retire** when `run_in_progress = 1`.

### Automated regression (CI)

Run the merge gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Targeted filters:

```bash
cargo test cron::
cargo test clawgotcha_retire
cargo test remove_job_defers_when_run_in_progress
cargo test due_jobs_skips_run_in_progress
cargo test record_run_survives_when_job_retired_mid_run
cargo test upsert_clawgotcha_clears_retirement
cargo test retire_clawgotcha_jobs_not_in_remote
cargo test reset_stale_cron_run_in_progress_flags
cargo test apply_batch_cron_deleted_retires_job
```

### Staging / pre-release manual scenarios

| ID | Scenario | Steps | Expected |
|----|-----------|-------|----------|
| S1 | Clawgotcha remove while idle | Enable `[clawgotcha]`, sync a cron job, delete job in control plane, wait for poll/webhook path | Job disappears from **`cron_list`** / gateway list; `get_job` or DB row still exists with `retired_at` set and `enabled = 0` |
| S2 | Clawgotcha remove **during** agent cron run | Start a long-running agent cron (or slow prompt), remove job in CP while run continues | Run completes; **`cron_runs`** has a row for that `job_id`; logs show no silent `record_run` failure; **`next_run`** is **not** advanced for recurring retired jobs |
| S3 | Imperative remove during run | `run_in_progress` set (job executing), `zeroclaw cron remove <id>` or gateway DELETE | Row **retired** (`user_remove_while_running`), not deleted; run can finish persisting |
| S4 | Revival after remote re-add | Retire via CP, then recreate same job id in CP | **`upsert_clawgotcha_agent_job`** clears `retired_at` / `retired_reason`; job visible in **`cron_list`** again |
| S5 | `cron_run` on retired job | Call `cron_run` with retired job id | Tool returns clear error (job retired) |
| S6 | Heartbeat metadata | Inspect heartbeat payload / CP logs | **`cron_jobs_count`** reflects **active** jobs only (`list_jobs` length), **excluding** retired rows — confirm this matches control-plane UX expectations |
| S7 | Stuck `run_in_progress` | Kill scheduler mid-job, restart daemon | On scheduler startup, stale **`run_in_progress`** flags are cleared (warn log with count); job becomes eligible again — verify no duplicate overlapping execution if the old process somehow survived (single-scheduler assumption) |

### Chaos / concurrency

- Two overlapping triggers for the same job (scheduler vs `cron_run`): both paths should set/clear `run_in_progress`; verify under load that **`due_jobs`** does not return the same id twice while flag is set.
- SQLite WAL + concurrent sync: Clawgotcha poll updating rows while scheduler persists — no panics; eventual consistency acceptable.

### Observability

- grep logs for `cron record_run failed`, `cron mark_run_finish failed`, `cron reschedule_after_run updated 0 rows`, `cron record_last_run updated 0 rows` during soak.
- Confirm **no** silent drops of `record_run` after deploy.

### Sign-off

- [ ] CI green on release branch  
- [ ] S1–S5 executed on staging with real `[clawgotcha]` URL  
- [ ] Ops agrees on heartbeat **`cron_jobs_count`** semantics (active-only)  
- [ ] S7 validated: scheduler restart clears stale **`run_in_progress`** (or ops runbook documents rare manual SQL if multi-writer DB)

### Related docs

- [Clawgotcha integration](../reference/integrations/clawgotcha.md) (behavior + deletion propagation notes)  
- [Clawgotcha API contract](../reference/integrations/clawgotcha-api-contract.md)
