# MiroClaw — tools reference

MiroClaw agents call **tools** implemented in Rust on the `Tool` trait (`src/tools/traits.rs`: `name`, `description`, `parameters_schema`, `execute`). There are three layers:

1. **`default_tools`** (`src/tools/mod.rs`) — the smallest set (shell + workspace file I/O and search), always policy-gated.
2. **`all_tools` / `all_tools_with_runtime`** — the main in-process registry: each tool is added only when config and runtime allow (API keys, `browser` enabled, `agents` for `delegate`, SOPs dir, `plugins-wasm`, etc.).
3. **Merged at runtime** — `tool_search`, MCP wrappers, skill- and node-derived tools, optional hardware tools (see *Runtime & dynamic tools*).

The summaries match each tool’s `description()` in code unless noted. Tools can appear in more than one conceptual area (e.g. `read_skill` is both *skills* and *context*); the table below lists the **primary** category for navigation.

---

## Category map

| Category | What it covers |
|----------|----------------|
| [A. Workspace, shell, Git & files](#a-workspace-shell-git--files) | Codebase file I/O, search, Git, PDF, projects on disk. |
| [B. Backups, retention & multi-workspace](#b-backups-retention--multi-workspace) | Archiving, purge/stats, isolated client workspaces. |
| [C. Long-term memory & knowledge](#c-long-term-memory--knowledge) | Markdown memory, knowledge graph, Discord history search. |
| [D. Skills](#d-skills) | Loading skill bodies and user-defined skill tools. |
| [E. Model routing, LLM & HTTP stack](#e-model-routing-llm--http-stack) | Model switch/routes, proxy, isolated LLM calls, Verifiable Intent. |
| [F. Scheduling & cron](#f-scheduling--cron) | `cron_*`, shell `schedule` vs agent-scheduled work. |
| [G. Web, HTTP, search & browsers](#g-web-http-search--browsers) | `http_request`, fetch, search, text browser, full browser automation. |
| [H. Media, vision & live canvas](#h-media-vision--live-canvas) | Screenshots, image metadata, image gen, canvas. |
| [I. Channel UX & sessions](#i-channel-ux--sessions) | Ask user, reactions, polls, cross-session messaging, push notifications. |
| [J. SaaS & work apps](#j-saas--work-apps) | Notion, Jira, M365, Google, LinkedIn, Composio. |
| [K. Project, cloud & security advisory](#k-project-cloud--security-advisory) | Project intel, cloud review, SOPs, MCSS/ops, security tooling. |
| [L. Orchestration & external CLIs](#l-orchestration--external-clis) | Delegate, swarm, Claude Code / Codex / Gemini / OpenCode. |
| [M. Productivity & misc](#m-productivity--misc) | Calculator, weather. |
| [N. Runtime & dynamic](#n-runtime--dynamic) | `tool_search`, MCP, skills-as-tools, node tools, WASM. |
| [O. Hardware & devices](#o-hardware--devices) | GPIO, I2C/SPI, boards, embedded helpers. |

---

## `default_tools` (six tools)

`default_tools` / `default_tools_with_runtime` is the **minimal** registry. Only security policy and runtime (e.g. shell availability) apply.

| Name | Summary |
|------|---------|
| `shell` | Run a shell command in the workspace directory. |
| `file_read` | Read file contents with line numbers; optional range; PDF text extraction; other binaries as lossy UTF-8. |
| `file_write` | Create or overwrite a file in the workspace. |
| `file_edit` | Apply a single exact `old_string` → `new_string` replacement in a file. |
| `glob_search` | List file paths under the workspace matching a glob pattern. |
| `content_search` | Search file contents by regex (ripgrep with grep fallback) inside the workspace. |

### `shell`

**Description:** Execute a shell command in the workspace directory.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `command` | yes | The shell command to run. |
| `approved` | no | Set `true` to mark medium/high-risk commands as explicitly approved in supervised mode (default `false`). |

### `file_read`

**Description:** Read file contents with line numbers. Supports partial reading via offset and limit. Extracts text from PDF; other binary files are read with lossy UTF-8 conversion.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `path` | yes | File path; relative paths resolve from the workspace; outside paths need policy allowlist. |
| `offset` | no | Start line (1-based; default from line 1). |
| `limit` | no | Max lines to return (default: entire file, subject to size limits). |

### `file_write`

**Description:** Write contents to a file in the workspace.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `path` | yes | Target path; same resolution rules as `file_read`. |
| `content` | yes | Full file body to write. |

### `file_edit`

**Description:** Edit a file by replacing an exact string match with new content. `old_string` must appear **exactly once** (zero matches = not found; more than one = ambiguous).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `path` | yes | File to modify. |
| `old_string` | yes | Exact text to find. |
| `new_string` | yes | Replacement; may be empty to delete the matched span. |

### `glob_search`

**Description:** Search for files matching a glob pattern within the workspace. Returns a sorted list of matching paths relative to the workspace root (e.g. `**/*.rs`, `src/**/mod.rs`).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `pattern` | yes | Glob pattern. |

### `content_search`

**Description:** Search file contents by regex within the workspace. Uses `rg` when available, otherwise `grep -rn -E`. Modes: `content` (lines + optional context), `files_with_matches`, or `count`.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `pattern` | yes | Regular expression. |
| `path` | no | Directory under the workspace to search (default `.`). |
| `output_mode` | no | `content` (default), `files_with_matches`, or `count`. |
| `include` | no | File glob filter (e.g. `*.rs`). |
| `case_sensitive` | no | Default `true`. |
| `context_before` / `context_after` | no | Context lines around matches in `content` mode. |
| `multiline` | no | Multiline regex (`rg` only; errors on grep fallback). |
| `max_results` | no | Cap (default 1000). |

---

## Categorized tool catalog (`all_tools`)

Not every process registers every tool — see `all_tools_with_runtime` in `src/tools/mod.rs` and `src/config/schema.rs`.

### A. Workspace, shell, Git & files

| Name | Summary |
|------|---------|
| `shell` | Execute a shell command in the workspace. |
| `file_read` | Read files with line numbers, PDF text, or lossy binary read. |
| `file_write` | Create or overwrite a workspace file. |
| `file_edit` | Single exact-string replacement edit. |
| `glob_search` | Glob match file paths under the workspace. |
| `content_search` | ripgrep/grep regex over file contents. |
| `git_operations` | Structured Git (status, diff, log, branch, commit, stash, …) with policy-aware JSON-style output. |
| `pdf_read` | Extract plain text from PDFs in the workspace (`rag-pdf` build feature). |

### B. Backups, retention & multi-workspace

| Name | Summary |
|------|---------|
| `backup` | Create, list, verify, and restore workspace backups. |
| `data_management` | Data retention, purge, and storage statistics. |
| `workspace` | Multi-client workspace isolation: list, switch, create, export, info. |

### C. Long-term memory & knowledge

| Name | Summary |
|------|---------|
| `memory_store` | Store a fact or note in long-term memory (categories such as `core`, `daily`, `conversation`). |
| `memory_recall` | Search memory with relevance scoring. |
| `memory_forget` | Delete one memory by key. |
| `memory_purge` | Bulk-delete by namespace or session. |
| `knowledge` | Knowledge graph: capture, search, relate, suggest, expert and lesson mining, graph stats. |
| `discord_search` | Search Discord message history in the local index (when the Discord history channel + DB are configured). |

### D. Skills

| Name | Summary |
|------|---------|
| `read_skill` | Read a skill’s full source by name (used with *compact* skills injection mode). |

*User-defined tools from `[[tools]]` in skills (shell/HTTP) are created at load time — see [N. Runtime & dynamic](#n-runtime--dynamic).*

### E. Model routing, LLM & HTTP stack

| Name | Summary |
|------|---------|
| `model_switch` | Get, set, or list current models and providers for the running session. |
| `model_routing_config` | Change default model, per-scenario routes, classification, delegate sub-agent profiles. |
| `proxy_config` | Apply proxy policy (environment vs Miroclaw vs per-service). |
| `llm_task` | One-off LLM completion with no tools; optional JSON Schema on the response. |
| `vi_verify` | Verifiable Intent: verify SD-hash binding and evaluate constraint checks on credentials. |

### F. Scheduling & cron

| Name | Summary |
|------|---------|
| `cron_add` | Create a scheduled job: shell or `agent` prompt; cron / at / human intervals; optional delivery to a channel. |
| `cron_list` | List scheduled cron jobs. |
| `cron_remove` | Remove a job by id. |
| `cron_update` | Patch schedule, command, delivery, model, etc. |
| `cron_run` | Force an immediate run and log history. |
| `cron_runs` | Inspect run history for a job. |
| `schedule` | Shell-only schedules whose output is logged (not the same as channel-delivered agent `cron` jobs). |

### G. Web, HTTP, search & browsers

| Name | Summary |
|------|---------|
| `http_request` | Outbound HTTP to an allowlisted host; SSRF protections, size and timeout limits. |
| `web_fetch` | Fetch a URL, return clean plain text (HTML stripped; optional Firecrawl fallback). |
| `web_search_tool` | Web search (provider from config, e.g. DuckDuckGo, Brave, SearXNG). |
| `text_browser` | Render a URL as plain text (lynx / links / w3m). |
| `browser` | Full browser automation: DOM + optional computer-use; snapshot/refs; domain allowlist. |
| `browser_open` | Open a single HTTPS URL in the system browser (no local/private hosts). |
| `browser_delegate` | Delegate a browser task to a browser-capable CLI. |

### H. Media, vision & live canvas

| Name | Summary |
|------|---------|
| `screenshot` | Capture a screenshot; path + base64 PNG. |
| `image_info` | Read image format, dimensions, size; optional base64. |
| `image_gen` | Text-to-image (e.g. fal.ai / Flux) into the workspace. |
| `canvas` | Push content to a live user-visible canvas: render, snapshot, clear, eval. |

### I. Channel UX & sessions

| Name | Summary |
|------|---------|
| `ask_user` | Ask a question in a channel and block until a reply (or timeout); optional choices. |
| `reaction` | Add or remove an emoji reaction on a specific platform message. |
| `poll` | Create a poll (native on some channels, text + emoji on others). |
| `sessions_list` | List active chat sessions. |
| `sessions_history` | Read recent messages for a session id. |
| `sessions_send` | Post a “user” message into another session (inter-session handoff). |
| `pushover` | Send a mobile notification via Pushover. |

### J. SaaS & work apps

| Name | Summary |
|------|---------|
| `composio` | Third-party app actions (list, execute, OAuth connect) through Composio. |
| `notion` | Databases, pages, search. |
| `jira` | Issues, JQL, comments, configurable detail. |
| `microsoft365` | Microsoft Graph: mail, Teams, calendar, OneDrive, SharePoint search, etc. |
| `google_workspace` | Google Workspace through the `gws` CLI. |
| `linkedin` | Posts, engagement, profile, content strategy (with credentials). |

### K. Project, cloud & security advisory

| Name | Summary |
|------|---------|
| `project_intel` | Read-only project/delivery analysis: status, risk, client updates, sprints, effort. |
| `cloud_ops` | Read-only cloud migration / IaC / cost / WAF-style advisory. |
| `cloud_patterns` | Suggest cloud architecture patterns for a described workload. |
| `security_ops` | MCSS-style ops: triage, playbooks, vulnerability parsing, reports, alert stats. |
| `sop_list` | List Standard Operating Procedures. |
| `sop_execute` | Start a SOP by name. |
| `sop_advance` | Record step result and move to the next SOP step. |
| `sop_approve` | Approve a step waiting for human approval. |
| `sop_status` | Query SOP run state. |

### L. Orchestration & external CLIs

| Name | Summary |
|------|---------|
| `delegate` | Hand a subtask to a configured sub-model / sub-agent. |
| `swarm` | Run multiple sub-agents in a pipeline, parallel, or router strategy. |
| `claude_code` | Run Claude Code (`claude -p`) in the workspace. |
| `claude_code_runner` | Spawn Claude Code in tmux; Slack + SSH handoff. |
| `codex_cli` | Delegate to Codex CLI. |
| `gemini_cli` | Delegate to Gemini CLI. |
| `opencode_cli` | Delegate to OpenCode CLI. |

### M. Productivity & misc

| Name | Summary |
|------|---------|
| `calculator` | Arithmetic and common statistics. |
| `weather` | Current conditions and short forecast for a place or coordinates. |

---

## N. Runtime & dynamic

These are not limited to a single `all_tools` registration list.

| Mechanism | Names & behavior |
|-----------|------------------|
| **WASM plugins** | `plugins-wasm` + config: **0..N** tools, each **name and description** from the plugin manifest (`src/plugins/wasm_tool.rs`). |
| **tool_search** | Resolves **deferred MCP** tool schemas so the model can call them (`src/tools/tool_search.rs`). |
| **MCP** | Each remote tool: `server_name__tool_name` (`src/tools/mcp_tool.rs`); description from the MCP `tools/list` payload. |
| **Skill `[[tools]]`** | Shell/HTTP: names like `skillname.tool` (`src/tools/skill_tool.rs`, `src/tools/skill_http.rs`). |
| **Node** | `NodeTool`: per-node **prefixed** names and descriptions from config (`src/tools/node_tool.rs`). |

---

## O. Hardware & devices

When hardware is enabled and a device is present, extra tools may be merged. **Fixed-name** examples (see `src/hardware/`, `src/peripherals/`); some tools use **dynamic** names from a manifest or board.

| Sub-area | Names (examples) |
|----------|------------------|
| **Boards & context** | `hardware_board_info`, `hardware_capabilities`, `hardware_memory_map`, `hardware_memory_read`, `rpi_system_info`, `datasheet` |
| **Buses** | `i2c_scan`, `i2c_read`, `i2c_write`, `spi_transfer`, `gpio_aardvark` |
| **Pico / device code** | `pico_flash`, `device_read_code`, `device_write_code`, `device_exec` |
| **Raspberry Pi GPIO** | `gpio_rpi_read`, `gpio_rpi_write`, `gpio_rpi_blink` |
| **Generic GPIO** | `gpio_read`, `gpio_write` (which implementation depends on the active stack) |
| **Arduino / upload** | `arduino_upload` |
| **Subprocess** | `SubprocessTool` — `name` from the hardware manifest. |

---

## Alphabetical name index

Quick lookup of **built-in `all_tools` names** (same strings as `name()`):

`ask_user`, `backup`, `browser`, `browser_delegate`, `browser_open`, `calculator`, `canvas`, `claude_code`, `claude_code_runner`, `cloud_ops`, `cloud_patterns`, `codex_cli`, `composio`, `content_search`, `cron_add`, `cron_list`, `cron_remove`, `cron_run`, `cron_runs`, `cron_update`, `data_management`, `delegate`, `discord_search`, `file_edit`, `file_read`, `file_write`, `gemini_cli`, `git_operations`, `glob_search`, `google_workspace`, `http_request`, `image_gen`, `image_info`, `jira`, `knowledge`, `linkedin`, `llm_task`, `memory_forget`, `memory_purge`, `memory_recall`, `memory_store`, `microsoft365`, `model_routing_config`, `model_switch`, `notion`, `opencode_cli`, `pdf_read`, `poll`, `project_intel`, `proxy_config`, `pushover`, `reaction`, `read_skill`, `schedule`, `screenshot`, `security_ops`, `sessions_history`, `sessions_list`, `sessions_send`, `shell`, `sop_advance`, `sop_approve`, `sop_execute`, `sop_list`, `sop_status`, `swarm`, `text_browser`, `vi_verify`, `weather`, `web_fetch`, `web_search_tool`, `workspace`

---

## See also

- Registry construction: `src/tools/mod.rs` (`default_tools`, `all_tools`, `all_tools_with_runtime`).
- Tool trait: `src/tools/traits.rs`.
