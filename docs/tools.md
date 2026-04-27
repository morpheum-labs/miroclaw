# MiroClaw — tools reference

MiroClaw agents call **tools** implemented in Rust on the `Tool` trait (`src/tools/traits.rs`: `name`, `description`, `parameters_schema`, `execute`). There are three layers:

1. **`default_tools`** (`src/tools/mod.rs`) — the smallest set (shell + workspace file I/O and search), always policy-gated.
2. **`all_tools` / `all_tools_with_runtime`** — the main in-process registry: each tool is added when config and runtime allow (API keys, `browser` enabled, `agents` for `delegate`, SOPs dir, `plugins-wasm`, etc.).
3. **Merged at runtime** — `tool_search`, MCP wrappers, skill- and node-derived tools, optional hardware tools.

The **per-tool** sections below list the tool `description` and a **parameter table** derived from the JSON Schema in `parameters_schema()`. Wording comes from the Rust sources; regen with `python3 scripts/generate_tool_param_docs.py`.

---

## Category map

| Category | What it covers |
|----------|----------------|
| [A–O below](#a-workspace-shell-git--files) | In-process tools, grouped (same as the previous high-level map). |
| [Runtime & dynamic](#runtime--dynamic-tools) | MCP, skills-as-tools, WASM, node — not a fixed `all_tools` name list. |
| [Hardware (extra)](#hardware--peripherals-extra) | Optional device tools beyond §O. |

---

## Default registry (minimal six)

`default_tools` is exactly these names; full parameter tables for them are in **[§A](#a-workspace-shell-git--files)**.

| Name | One-line |
|------|----------|
| `shell` | Run a shell command in the workspace. |
| `file_read` | Read files with line numbers, PDF text, or lossy binary. |
| `file_write` | Create or overwrite a file. |
| `file_edit` | Single exact-string replacement. |
| `glob_search` | Glob file paths under the workspace. |
| `content_search` | Regex search of file contents (ripgrep / grep). |

---

## Runtime & dynamic tools

| Mechanism | Notes |
|-----------|--------|
| **WASM plugins** | `plugins-wasm` + config: 0..N tools; **name** and **schema** from each plugin manifest (`src/plugins/wasm_tool.rs`). |
| **`McpToolWrapper`** | **Name** = `server_name__tool_name`; **schema** = MCP `inputSchema` from the server (`src/tools/mcp_tool.rs`). |
| **Skill `[[tools]]`** | `skillname.tool` names; schema from the skill (`src/tools/skill_tool.rs`, `src/tools/skill_http.rs`). |
| **`NodeTool`** | Prefixed names; schema from node config (`src/tools/node_tool.rs`). |

---

## Hardware & peripherals (extra)

Optional tools (GPIO, I2C, SPI, boards, `arduino_upload`, `pico_flash`, `device_*`, etc.) are described in the source under `src/hardware/` and `src/peripherals/`. Some **names** are dynamic (manifest-driven). The three `hardware_*` tools documented in **§O** are the Miroclaw-integrated memory/board helpers.

---

## Per-tool reference (generated)

## A. Workspace, shell, Git & files

### `content_search`

**Source:** `src/tools/content_search.rs`

**Description:** Search file contents by regex pattern within the workspace. Supports ripgrep (rg) with grep fallback. Output modes: 'content' (matching lines with context), 'files_with_matches' (file paths only), 'count' (match counts per file). Example: pattern='fn main', include='*.rs', output_mode='content'.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `case_sensitive` | no | `boolean` — Case-sensitive matching. Defaults to true — default: true |
| `context_after` | no | `integer` — Lines of context after each match (content mode only) — default: 0 |
| `context_before` | no | `integer` — Lines of context before each match (content mode only) — default: 0 |
| `include` | no | `string` — File glob filter, e.g. '*.rs', '*.{ts,tsx}' |
| `max_results` | no | `integer` — Maximum number of results to return. Defaults to 1000 — default: 1000 |
| `multiline` | no | `boolean` — Enable multiline matching (ripgrep only, errors on grep fallback) — default: false |
| `output_mode` | no | `string` — Output format: 'content' (matching lines), 'files_with_matches' (paths only), 'count' (match counts) — enum: content, files_with_matches, count; default: "content" |
| `path` | no | `string` — Directory to search in, relative to workspace root. Defaults to '.' — default: "." |
| `pattern` | yes | `string` — Regular expression pattern to search for |

### `file_edit`

**Source:** `src/tools/file_edit.rs`

**Description:** Edit a file by replacing an exact string match with new content

| Parameter | Required | Notes |
|-----------|----------|--------|
| `new_string` | yes | `string` — The replacement text (empty string to delete the matched text) |
| `old_string` | yes | `string` — The exact text to find and replace (must appear exactly once in the file) |
| `path` | yes | `string` — Path to the file. Relative paths resolve from workspace; outside paths require policy allowlist. |

### `file_read`

**Source:** `src/tools/file_read.rs`

**Description:** Read file contents with line numbers. Supports partial reading via offset and limit. Extracts text from PDF; other binary files are read with lossy UTF-8 conversion.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `limit` | no | `integer` — Maximum number of lines to return (default: all) |
| `offset` | no | `integer` — Starting line number (1-based, default: 1) |
| `path` | yes | `string` — Path to the file. Relative paths resolve from workspace; outside paths require policy allowlist. |

### `file_write`

**Source:** `src/tools/file_write.rs`

**Description:** Write contents to a file in the workspace

| Parameter | Required | Notes |
|-----------|----------|--------|
| `content` | yes | `string` — Content to write to the file |
| `path` | yes | `string` — Path to the file. Relative paths resolve from workspace; outside paths require policy allowlist. |

### `git_operations`

**Source:** `src/tools/git_operations.rs`

**Description:** Perform structured Git operations (status, diff, log, branch, commit, add, checkout, stash). Provides parsed JSON output and integrates with security policy for autonomy controls.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | no | `string` — Stash action (for 'stash' operation) — enum: push, pop, list, drop |
| `branch` | no | `string` — Branch name (for 'checkout' operation) |
| `cached` | no | `boolean` — Show staged changes (for 'diff' operation) |
| `files` | no | `string` — File or path to diff (for 'diff' operation, default: '.') |
| `index` | no | `integer` — Stash index (for 'stash' with 'drop' action) |
| `limit` | no | `integer` — Number of log entries (for 'log' operation, default: 10) |
| `message` | no | `string` — Commit message (for 'commit' operation) |
| `operation` | yes | `string` — Git operation to perform — enum: status, diff, log, branch, commit, add, checkout, stash |
| `path` | no | `string` — Optional subdirectory path within the workspace to run git operations in. Defaults to workspace root. |
| `paths` | no | `string` — File paths to stage (for 'add' operation) |

### `glob_search`

**Source:** `src/tools/glob_search.rs`

**Description:** Search for files matching a glob pattern within the workspace. Returns a sorted list of matching file paths relative to the workspace root. Examples: '**/*.rs' (all Rust files), 'src/**/mod.rs' (all mod.rs in src).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `pattern` | yes | `string` — Glob pattern to match files, e.g. '**/*.rs', 'src/**/mod.rs' |

### `pdf_read`

**Source:** `src/tools/pdf_read.rs`

**Description:** Extract plain text from a PDF file in the workspace. Returns all readable text. Image-only or encrypted PDFs return an empty result. Requires the 'rag-pdf' build feature.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `max_chars` | no | `integer` — Maximum characters to return (default: 50000, max: 200000) |
| `path` | yes | `string` — Path to the PDF file. Relative paths resolve from workspace; outside paths require policy allowlist. |

### `shell`

**Source:** `src/tools/shell.rs`

**Description:** Execute a shell command in the workspace directory

| Parameter | Required | Notes |
|-----------|----------|--------|
| `approved` | no | `boolean` — Set true to explicitly approve medium/high-risk commands in supervised mode — default: false |
| `command` | yes | `string` — The shell command to execute |

## B. Backups, retention & multi-workspace

### `backup`

**Source:** `src/tools/backup_tool.rs`

**Description:** Create, list, verify, and restore workspace backups

| Parameter | Required | Notes |
|-----------|----------|--------|
| `backup_name` | no | `string` — Name of backup (for verify/restore) |
| `command` | yes | `string` — Backup command to execute — enum: create, list, verify, restore |
| `confirm` | no | `boolean` — Confirm restore (required for actual restore, default false) |

### `data_management`

**Source:** `src/tools/data_management.rs`

**Description:** Workspace data retention, purge, and storage statistics

| Parameter | Required | Notes |
|-----------|----------|--------|
| `command` | yes | `string` — Data management command — enum: retention_status, purge, stats |
| `dry_run` | no | `boolean` — If true, purge only lists what would be deleted (default true) |

### `workspace`

**Source:** `src/tools/workspace_tool.rs`

**Description:** Manage multi-client workspaces. Subcommands: list, switch, create, info, export. Each workspace provides isolated memory, audit, secrets, and tool restrictions.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — Workspace action to perform — enum: list, switch, create, info, export |
| `name` | no | `string` — Workspace name (required for switch, create, export) |

## C. Long-term memory & knowledge

### `discord_search`

**Source:** `src/tools/discord_search.rs`

**Description:** Search Discord message history. Returns messages matching a keyword query, optionally filtered by channel_id, author_id, or time range.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `channel_id` | no | `string` — Filter results to a specific Discord channel ID |
| `limit` | no | `integer` — Max results to return (default: 10) |
| `query` | no | `string` — Keywords or phrase to search for in Discord messages (optional if since/until provided) |
| `since` | no | `string` — Filter messages at or after this time (RFC 3339, e.g. 2025-03-01T00:00:00Z) |
| `until` | no | `string` — Filter messages at or before this time (RFC 3339) |

### `knowledge`

**Source:** `src/tools/knowledge_tool.rs`

**Description:** Manage a knowledge graph of architecture decisions, solution patterns, lessons learned, and experts. Actions: capture, search, relate, suggest, expert_find, lessons_extract, graph_stats.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The action to perform — enum: capture, search, relate, suggest, expert_find, lessons_extract, graph_stats |
| `content` | no | `string` — Content body (for capture) or text to extract lessons from (for lessons_extract) |
| `filters` | no | `object` — Optional search filters |
| `from_id` | no | `string` — Source node ID (for relate) |
| `node_type` | no | `string` — Type of knowledge node (for capture) — enum: pattern, decision, lesson, expert, technology |
| `query` | no | `string` — Search query text (for search, suggest) |
| `relation` | no | `string` — Relationship type (for relate) — enum: uses, replaces, extends, authored_by, applies_to |
| `source_project` | no | `string` — Source project identifier (for capture) |
| `tags` | no | `array` — Tags for filtering and categorization |
| `title` | no | `string` — Title for the knowledge item (for capture) |
| `to_id` | no | `string` — Target node ID (for relate) |

### `memory_forget`

**Source:** `src/tools/memory_forget.rs`

**Description:** Remove a memory by key. Use to delete outdated facts or sensitive data. Returns whether the memory was found and removed.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `key` | yes | `string` — The key of the memory to forget |

### `memory_purge`

**Source:** `src/tools/memory_purge.rs`

**Description:** Remove all memories in a namespace (category) or session. Use to bulk-delete conversation context or category-scoped data. Returns the number of deleted entries. WARNING: This operation cannot be undone.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `namespace` | no | `string` — The namespace (category) to purge. Deletes all memories in this category. |
| `session_id` | no | `string` — The session ID to purge. Deletes all memories in this session. |

### `memory_recall`

**Source:** `src/tools/memory_recall.rs`

**Description:** Search long-term memory for relevant facts, preferences, or context. Returns scored results ranked by relevance. Supports keyword search, time-only query (since/until), or both.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `limit` | no | `integer` — Max results to return (default: 5) |
| `query` | no | `string` — Keywords or phrase to search for in memory (optional if since/until provided) |
| `since` | no | `string` — Filter memories created at or after this time (RFC 3339, e.g. 2025-03-01T00:00:00Z) |
| `until` | no | `string` — Filter memories created at or before this time (RFC 3339) |

### `memory_store`

**Source:** `src/tools/memory_store.rs`

**Description:** Store a fact, preference, or note in long-term memory. Use category 'core' for permanent facts, 'daily' for session notes, 'conversation' for chat context, or a custom category name.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `category` | no | `string` — Memory category: 'core' (permanent), 'daily' (session), 'conversation' (chat), or a custom category name. Defaults to 'core'. |
| `content` | yes | `string` — The information to remember |
| `key` | yes | `string` — Unique key for this memory (e.g. 'user_lang', 'project_stack') |

## D. Skills

### `read_skill`

**Source:** `src/tools/read_skill.rs`

**Description:** Read the full source file for an available skill by name. Use this in compact skills mode when you need the complete skill instructions without remembering file paths.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `name` | yes | `string` — The skill name exactly as listed in <available_skills>. |

## E. Model routing, LLM & HTTP stack

### `llm_task`

**Source:** `src/tools/llm_task.rs`

**Description:** Run a prompt through an LLM with no tool access and return the response. Optionally validates the output against a JSON Schema. Ideal for structured data extraction, classification, summarization, and transformation tasks.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `model` | no | `string` |
| `prompt` | yes | `string` — The prompt to send to the LLM. |
| `schema` | no | `object` |
| `temperature` | no | `number` |

### `model_routing_config`

**Source:** `src/tools/model_routing_config.rs`

**Description:** Manage default model settings, scenario-based provider/model routes, classification rules, and delegate sub-agent profiles

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | no | `string` — enum: get, list_hints, set_default, upsert_scenario, remove_scenario, upsert_agent, remove_agent; default: "get" |
| `agentic` | no | `boolean` — Enable tool-call loop mode for delegate agent |
| `allowed_tools` | no | `string` — Allowed tools for agentic delegate mode (string or string array) |
| `api_key` | no | `string \| null` — Optional API key override for scenario route or delegate agent |
| `classification_enabled` | no | `boolean` — When true, upsert classification rule for this hint; false removes it |
| `hint` | no | `string` — Scenario hint name (for example: conversation, coding, reasoning) |
| `keywords` | no | `string` — Classification keywords for upsert_scenario (string or string array) |
| `max_depth` | no | `integer \| null` — Delegate max recursion depth |
| `max_iterations` | no | `integer \| null` — Maximum tool-call iterations for agentic delegate mode |
| `max_length` | no | `integer \| null` — Optional maximum message length matcher |
| `min_length` | no | `integer \| null` — Optional minimum message length matcher |
| `model` | no | `string` — Model for set_default/upsert_scenario/upsert_agent |
| `name` | no | `string` — Delegate sub-agent name for upsert_agent/remove_agent |
| `patterns` | no | `string` — Classification literal patterns for upsert_scenario (string or string array) |
| `priority` | no | `integer \| null` — Classification priority (higher runs first) |
| `provider` | no | `string` — Provider for set_default/upsert_scenario/upsert_agent |
| `remove_classification` | no | `boolean` — When remove_scenario, whether to remove matching classification rule (default true) |
| `system_prompt` | no | `string \| null` — Optional system prompt override for delegate agent |
| `temperature` | no | `number \| null` — Optional temperature override (0.0-2.0) |

### `model_switch`

**Source:** `src/tools/model_switch.rs`

**Description:** Switch the AI model at runtime. Use 'get' to see current model, 'list_providers' to see available providers, 'list_models' to see models for a provider, or 'set' to switch to a different model. The switch takes effect immediately for the current conversation.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — Action to perform: get current model, set a new model, list available providers, or list models for a provider — enum: get, set, list_providers, list_models |
| `model` | no | `string` — Model ID (e.g., 'gpt-4o', 'claude-sonnet-4-6'). Required for 'set' action. |
| `provider` | no | `string` — Provider name (e.g., 'openai', 'anthropic', 'groq', 'ollama'). Required for 'set' and 'list_models' actions. |

### `proxy_config`

**Source:** `src/tools/proxy_config.rs`

**Description:** Manage Miroclaw proxy settings (scope: environment | miroclaw | services), including runtime and process env application

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | no | `string` — enum: get, set, disable, list_services, apply_env, clear_env; default: "get" |
| `all_proxy` | no | `string \| null` — Fallback proxy URL for all protocols |
| `clear_env` | no | `boolean` — When action=disable, clear process proxy environment variables |
| `enabled` | no | `boolean` — Enable or disable proxy |
| `http_proxy` | no | `string \| null` — HTTP proxy URL |
| `https_proxy` | no | `string \| null` — HTTPS proxy URL |
| `no_proxy` | no | `string` — Comma-separated string or array of NO_PROXY entries |
| `scope` | no | `string` — Proxy scope: environment \| miroclaw \| services |
| `services` | no | `string` — Comma-separated string or array of service selectors used when scope=services |

### `vi_verify`

**Source:** `src/tools/verifiable_intent.rs`

**Description:** Verify a Verifiable Intent credential chain. Supports two operations: 'verify_binding' checks sd_hash binding between credential layers; 'evaluate_constraints' validates constraints against fulfillment data.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `constraints` | no | `array` — Constraint array (for evaluate_constraints). |
| `exp` | no | `integer` — Expiration timestamp (for verify_timestamps). |
| `fulfillment` | no | `object` — Fulfillment data to evaluate against (for evaluate_constraints). |
| `iat` | no | `integer` — Issued-at timestamp (for verify_timestamps). |
| `operation` | yes | `string` — The VI operation to perform. — enum: verify_binding, evaluate_constraints, verify_timestamps |
| `sd_hash` | no | `string` — Expected sd_hash value (for verify_binding). |
| `serialized_parent` | no | `string` — Serialized parent SD-JWT (for verify_binding). |

## F. Scheduling & cron

### `cron_add`

**Source:** `src/tools/cron_add.rs`

**Description:** Create a scheduled cron job (shell or agent) with cron/at/every schedules. Use job_type='agent' with a prompt to run the AI agent on schedule. To deliver output to a channel (Discord, Telegram, Slack, Mattermost, Matrix, QQ), set delivery={ "mode ": "announce ", "channel ": "discord ", "to ": "<channel_id_or_chat_id> "}. This is the preferred tool for sending scheduled/delayed messages to users via channels.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `allowed_tools` | no | `array` — Optional allowlist of tool names for agent jobs. When omitted, all tools remain available. |
| `approved` | no | `boolean` — Set true to explicitly approve medium/high-risk shell commands in supervised mode — default: false |
| `command` | no | `string` — Shell command (job_type 'shell') or hand name without .toml (job_type 'hand') |
| `delete_after_run` | no | `boolean` — If true, the job is automatically deleted after its first successful run. Defaults to true for 'at' schedules. |
| `delivery` | no | `object` — Optional delivery config to send job output to a channel after each run. When provided, all three of mode, channel, and to are expected. — enum: none, announce |
| `job_type` | no | `string` — Type of job: 'shell' runs a command, 'agent' runs the AI agent with a prompt, 'hand' runs a hand from ~/.miroclaw/hands/{command}.toml — enum: shell, agent, hand |
| `model` | no | `string` — Optional model override for agent jobs, e.g. 'x-ai/grok-4-1-fast' |
| `name` | no | `string` — Optional human-readable name for the job |
| `prompt` | no | `string` — Agent prompt to run on schedule (required when job_type is 'agent') |
| `schedule` | no | `object` — When to run the job. Exactly one of three forms must be used. — enum: cron |
| `session_target` | no | `string` — Agent session context: 'isolated' starts a fresh session each run, 'main' reuses the primary session — enum: isolated, main |

### `cron_list`

**Source:** `src/tools/cron_list.rs`

**Description:** List all scheduled cron jobs

| Parameter | Required | Notes |
|-----------|----------|--------|
| — | — | No parameters; pass an empty object `{}`. |

### `cron_remove`

**Source:** `src/tools/cron_remove.rs`

**Description:** Remove a cron job by id

| Parameter | Required | Notes |
|-----------|----------|--------|
| `job_id` | yes | `string` |

### `cron_run`

**Source:** `src/tools/cron_run.rs`

**Description:** Force-run a cron job immediately and record run history

| Parameter | Required | Notes |
|-----------|----------|--------|
| `approved` | no | `boolean` — Set true to explicitly approve medium/high-risk shell commands in supervised mode — default: false |
| `job_id` | yes | `string` |

### `cron_runs`

**Source:** `src/tools/cron_runs.rs`

**Description:** List recent run history for a cron job

| Parameter | Required | Notes |
|-----------|----------|--------|
| `job_id` | yes | `string` |
| `limit` | no | `integer` |

### `cron_update`

**Source:** `src/tools/cron_update.rs`

**Description:** Patch an existing cron job (schedule, command, prompt, enabled, delivery, model, etc.)

| Parameter | Required | Notes |
|-----------|----------|--------|
| `approved` | no | `boolean` — Set true to explicitly approve medium/high-risk shell commands in supervised mode — default: false |
| `job_id` | no | `string` — ID of the cron job to update, as returned by cron_add or cron_list |
| `patch` | no | `object` — Fields to update. Only include fields you want to change; omitted fields are left as-is. — enum: isolated, main |

### `schedule`

**Source:** `src/tools/schedule.rs`

**Description:** Manage scheduled shell-only tasks. Actions: create/add/once/list/get/cancel/remove/pause/resume. WARNING: This tool creates shell jobs whose output is only logged, NOT delivered to any channel. To send a scheduled message to Discord/Telegram/Slack/Matrix, use the cron_add tool with job_type='agent' and a delivery config like { "mode ": "announce ", "channel ": "discord ", "to ": "<channel_id> "}.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — Action to perform — enum: create, add, once, list, get, cancel, remove, pause, resume |
| `approved` | no | `boolean` — Set true to explicitly approve medium/high-risk shell commands in supervised mode — default: false |
| `command` | no | `string` — Shell command to execute. Required for create/add/once. |
| `delay` | no | `string` — Delay for one-shot tasks (e.g. '30m', '2h', '1d'). |
| `expression` | no | `string` — Cron expression for recurring tasks (e.g. '*/5 * * * *'). |
| `id` | no | `string` — Task ID. Required for get/cancel/remove/pause/resume. |
| `run_at` | no | `string` — Absolute RFC3339 time for one-shot tasks (e.g. '2030-01-01T00:00:00Z'). |

## G. Web, HTTP, search & browsers

### `browser`

**Source:** `src/tools/browser.rs`

**Description:** Web/browser automation with pluggable backends (agent-browser, rust-native, computer_use).  Supports DOM actions plus optional OS-level actions (mouse_move, mouse_click, mouse_drag,  key_type, key_press, screen_capture) through a computer-use sidecar. Use 'snapshot' to map  interactive elements to refs (@e1, @e2). Enforces browser.allowed_domains for open actions.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — Browser action to perform (OS-level actions require backend=computer_use) — enum: 22 values (see source) |
| `button` | no | `string` — Mouse button for computer_use mouse_click — enum: left, right, middle |
| `by` | no | `string` — For find: semantic locator type — enum: role, text, label, placeholder, testid |
| `compact` | no | `boolean` — For snapshot: remove empty structural elements |
| `depth` | no | `integer` — For snapshot: limit tree depth |
| `direction` | no | `string` — Scroll direction — enum: up, down, left, right |
| `fill_value` | no | `string` — For find with fill action: value to fill |
| `find_action` | no | `string` — For find: action to perform on found element — enum: click, fill, text, hover, check |
| `from_x` | no | `integer` — Drag source X coordinate (computer_use: mouse_drag) |
| `from_y` | no | `integer` — Drag source Y coordinate (computer_use: mouse_drag) |
| `full_page` | no | `boolean` — For screenshot: capture full page |
| `interactive_only` | no | `boolean` — For snapshot: only show interactive elements |
| `key` | no | `string` — Key to press (Enter, Tab, Escape, etc.) |
| `ms` | no | `integer` — Milliseconds to wait |
| `path` | no | `string` — File path for screenshot |
| `pixels` | no | `integer` — Pixels to scroll |
| `selector` | no | `string` — Element selector: @ref (e.g. @e1), CSS (#id, .class), or text=... |
| `text` | no | `string` — Text to type or wait for |
| `to_x` | no | `integer` — Drag target X coordinate (computer_use: mouse_drag) |
| `to_y` | no | `integer` — Drag target Y coordinate (computer_use: mouse_drag) |
| `url` | no | `string` — URL to navigate to (for 'open' action) |
| `value` | no | `string` — Value to fill or type |
| `x` | no | `integer` — Screen X coordinate (computer_use: mouse_move/mouse_click) |
| `y` | no | `integer` — Screen Y coordinate (computer_use: mouse_move/mouse_click) |

### `browser_delegate`

**Source:** `src/tools/browser_delegate.rs`

**Description:** Delegate browser-based tasks to a browser-capable CLI for interacting with web applications like Teams, Outlook, Jira, Confluence

| Parameter | Required | Notes |
|-----------|----------|--------|
| `extract_format` | no | `string` — Desired output format (default: text) — enum: text, json, summary |
| `task` | yes | `string` — Description of the browser task to perform |
| `url` | no | `string` — Optional URL to navigate to before performing the task |

### `browser_open`

**Source:** `src/tools/browser_open.rs`

**Description:** Open an approved HTTPS URL in the system browser. Security constraints: allowlist-only domains, no local/private hosts, no scraping.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `url` | yes | `string` — HTTPS URL to open in the system browser |

### `http_request`

**Source:** `src/tools/http_request.rs`

**Description:** Make HTTP requests to external APIs. Supports GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS methods. Security constraints: allowlist-only domains, no local/private hosts, configurable timeout and response size limits.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `body` | no | `string` — Optional request body (for POST, PUT, PATCH requests) |
| `headers` | no | `object` — Optional HTTP headers as key-value pairs (e.g., {"Authorization": "Bearer token", "Content-Type": "application/json"}) — default: { |
| `method` | no | `string` — HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS) — default: "GET" |
| `url` | yes | `string` — HTTP or HTTPS URL to request |

### `text_browser`

**Source:** `src/tools/text_browser.rs`

**Description:** Render a web page as plain text using a text-based browser (lynx, links, or w3m). Ideal for headless/SSH environments without a graphical browser. Auto-detects available browser or uses a configured preference.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `browser` | no | `string` — Text browser to use: "lynx", "links", or "w3m". If omitted, auto-detects an available browser. — enum: lynx, links, w3m |
| `url` | yes | `string` — The HTTP or HTTPS URL to render as plain text |

### `web_fetch`

**Source:** `src/tools/web_fetch.rs`

**Description:** Fetch a web page and return its content as clean plain text. HTML pages are automatically converted to readable text. JSON and plain text responses are returned as-is. Only GET requests; follows redirects. Falls back to Firecrawl for JS-heavy/bot-blocked sites (if enabled). Security: allowlist-only domains, no local/private hosts.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `url` | yes | `string` — The HTTP or HTTPS URL to fetch |

### `web_search_tool`

**Source:** `src/tools/web_search_tool.rs`

**Description:** Search the web for information. Returns relevant search results with titles, URLs, and descriptions. Use this to find current information, news, or research topics.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `query` | yes | `string` — The search query. Be specific for better results. |

## H. Media, vision & live canvas

### `canvas`

**Source:** `src/tools/canvas.rs`

**Description:** Push rendered content (HTML, SVG, Markdown) to a live web canvas that users can see in real-time. Actions: render (push content), snapshot (get current content), clear (reset canvas), eval (evaluate JS expression in canvas context). Each canvas is identified by a canvas_id string.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — Action to perform on the canvas. — enum: render, snapshot, clear, eval |
| `canvas_id` | no | `string` — Unique identifier for the canvas. Defaults to 'default'. |
| `content` | no | `string` — Content to render (for render action). |
| `content_type` | no | `string` — Content type for render action: html, svg, markdown, or text. — enum: html, svg, markdown, text |
| `expression` | no | `string` |

### `image_gen`

**Source:** `src/tools/image_gen.rs`

**Description:** Generate an image from a text prompt using fal.ai (Flux models). Saves the result to the workspace images directory and returns the file path.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `filename` | no | `string` — Output filename without extension (default: 'generated_image'). Saved as PNG in workspace/images/. |
| `model` | no | `string` — fal.ai model identifier (default: 'fal-ai/flux/schnell'). |
| `prompt` | yes | `string` — Text prompt describing the image to generate. |
| `size` | no | `string` — Image aspect ratio / size preset (default: 'square_hd'). — enum: square_hd, landscape_4_3, portrait_4_3, landscape_16_9, portrait_16_9 |

### `image_info`

**Source:** `src/tools/image_info.rs`

**Description:** Read image file metadata (format, dimensions, size) and optionally return base64-encoded data.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `include_base64` | no | `boolean` — Include base64-encoded image data in output (default: false) |
| `path` | yes | `string` — Path to the image file (absolute or relative to workspace) |

### `screenshot`

**Source:** `src/tools/screenshot.rs`

**Description:** Capture a screenshot of the current screen. Returns the file path and base64-encoded PNG data.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `filename` | no | `string` — Optional filename (default: screenshot_<timestamp>.png). Saved in workspace. |
| `region` | no | `string` — Optional region for macOS: 'selection' for interactive crop, 'window' for front window. Ignored on Linux. |

## I. Channel UX & sessions

### `ask_user`

**Source:** `src/tools/ask_user.rs`

**Description:** Ask the user a question and wait for their response. Sends the question to a messaging channel and blocks until the user replies or the timeout expires. Optionally provide choices for structured responses.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `channel` | no | `string` — Target channel name. Defaults to the first available channel if omitted. |
| `choices` | no | `array` — Optional list of choices (renders as buttons on Telegram, numbered list on CLI) |
| `question` | yes | `string` — The question to ask the user |
| `timeout_secs` | no | `integer` — Seconds to wait for a response (default: 300) |

### `poll`

**Source:** `src/tools/poll.rs`

**Description:** Create a poll in a messaging channel. For Telegram/Discord uses native polls; for other channels formats as a numbered text message with emoji reactions for voting.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `channel` | no | `string` — Target channel name. Defaults to the first available channel if omitted. |
| `duration_minutes` | no | `integer` — Poll duration in minutes (default: 60) |
| `multi_select` | no | `boolean` — Allow multiple selections (default: false) |
| `options` | yes | `array` — Poll answer options (2-10 items) |
| `question` | yes | `string` — The poll question |
| `recipient` | no | `string` — Recipient/chat identifier within the channel (e.g., chat_id for Telegram, channel_id for Slack) |

### `pushover`

**Source:** `src/tools/pushover.rs`

**Description:** Send a Pushover notification to your device. Requires PUSHOVER_TOKEN and PUSHOVER_USER_KEY in .env file.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `message` | yes | `string` — The notification message to send |
| `priority` | no | `integer` — Message priority: -2 (lowest/silent), -1 (low/no sound), 0 (normal), 1 (high), 2 (emergency/repeating) |
| `sound` | no | `string` — Notification sound override (e.g., 'pushover', 'bike', 'bugle', 'cashregister', etc.) |
| `title` | no | `string` — Optional notification title |

### `reaction`

**Source:** `src/tools/reaction.rs`

**Description:** Add or remove an emoji reaction on a message in any active channel. Provide the channel name (e.g. 'discord', 'slack'), the platform channel ID, the platform message ID, and the emoji (Unicode character or platform shortcode).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | no | `string` — Whether to add or remove the reaction (default: 'add') — enum: add, remove |
| `channel` | yes | `string` — Name of the channel to react in (e.g. 'discord', 'slack', 'telegram') |
| `channel_id` | yes | `string` — Platform-specific channel/conversation identifier (e.g. Discord channel snowflake, Slack channel ID) |
| `emoji` | yes | `string` — Emoji to react with (Unicode character or platform shortcode) |
| `message_id` | yes | `string` — Platform-scoped message identifier to react to |

### `sessions_history`

**Source:** `src/tools/sessions.rs`

**Description:** Read the message history of a specific session by its session ID. Returns the last N messages.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `limit` | no | `integer` — Max messages to return, from most recent (default: 20) |
| `session_id` | yes | `string` — The session ID to read history from (e.g. telegram__user123) |

### `sessions_list`

**Source:** `src/tools/sessions.rs`

**Description:** List all active conversation sessions with their channel, last activity time, and message count.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `limit` | no | `integer` — Max sessions to return (default: 50) |

### `sessions_send`

**Source:** `src/tools/sessions.rs`

**Description:** Send a message to a specific session by its session ID. The message is appended to the session's conversation history as a 'user' message, enabling inter-agent communication.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `message` | yes | `string` — The message content to send |
| `session_id` | yes | `string` — The target session ID (e.g. telegram__user123) |

## J. SaaS & work apps

### `composio`

**Source:** `src/tools/composio.rs`

**Description:** Execute actions on 1000+ apps via Composio (Gmail, Notion, GitHub, Slack, etc.). Use action='list' to see available actions (includes parameter names). action='execute' with action_name/tool_slug and params to run an action. If you are unsure of the exact params, pass 'text' instead with a natural-language description of what you want (Composio will resolve the correct parameters via NLP). action='list_accounts' or action='connected_accounts' to list OAuth-connected accounts. action='connect' with app/auth_config_id to get OAuth URL. connected_account_id is auto-resolved when omitted.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The operation: 'list' (list available actions), 'list_accounts'/'connected_accounts' (list connected accounts), 'execute' (run an action), or 'connect' (get OAuth URL) — enum: list, list_accounts, connected_accounts, execute, connect |
| `action_name` | no | `string` — Action/tool identifier to execute (legacy aliases supported) |
| `app` | no | `string` — Toolkit slug filter for 'list' or 'list_accounts', optional app hint for 'execute', or toolkit/app for 'connect' (e.g. 'gmail', 'notion', 'github') |
| `auth_config_id` | no | `string` — Optional Composio v3 auth config id for connect flow |
| `connected_account_id` | no | `string` — Optional connected account ID for execute flow when a specific account is required |
| `entity_id` | no | `string` — Entity/user ID for multi-user setups (defaults to composio.entity_id from config) |
| `params` | no | `object` — Structured parameters to pass to the action (use the key names shown by action='list') |
| `text` | no | `string` — Natural-language description of what you want the action to do (alternative to 'params' when you are unsure of the exact parameter names). Composio will resolve the correct parameters via NLP. Mutually exclusive with 'params'. |
| `tool_slug` | no | `string` — Preferred v3 tool slug to execute (alias of action_name) |

### `google_workspace`

**Source:** `src/tools/google_workspace.rs`

**Description:** Interact with Google Workspace services (Drive, Gmail, Calendar, Sheets, Docs, etc.) via the gws CLI. Requires gws to be installed and authenticated.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `body` | no | `object` — Request body for POST/PATCH/PUT operations (passed as --json JSON) |
| `format` | no | `string` — Output format (default: json) — enum: json, table, yaml, csv |
| `method` | yes | `string` — Method to call on the resource (e.g. list, get, create, update, delete) |
| `page_all` | no | `boolean` — Auto-paginate through all results |
| `page_limit` | no | `integer` — Max pages to fetch when using page_all (default: 10) |
| `params` | no | `object` — URL/query parameters as key-value pairs (passed as --params JSON) |
| `resource` | yes | `string` — Service resource (e.g. files, messages, events, spreadsheets) |
| `service` | yes | `string` — Google Workspace service (e.g. drive, gmail, calendar, sheets, docs, slides, tasks, people, chat, classroom, forms, keep, meet, events) |
| `sub_resource` | no | `string` — Optional sub-resource for nested operations |

### `jira`

**Source:** `src/tools/jira_tool.rs`

**Description:** Interact with Jira: get tickets with configurable detail level, search issues with JQL, add comments with mention and formatting support.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The Jira action to perform. Enabled actions are configured in [jira].allowed_actions. Use 'myself' to verify that credentials are valid and the Jira connection is working. — enum: get_ticket, search_tickets, comment_ticket, list_projects, myself |
| `comment` | no | `string` — Comment body for comment_ticket. Supports a limited markdown-like syntax converted to Atlassian Document Format (ADF). Mention a user with @user@domain.com — the leading @ is required (a bare email without @ prefix is treated as plain text). Bold with **text**. Bullet list items with a leading '- '. Newlines become line breaks. Everything else is plain text. Example: 'Hi @john@company.com, this is |
| `issue_key` | no | `string` — Jira issue key, e.g. 'PROJ-123'. Required for get_ticket and comment_ticket. |
| `jql` | no | `string` — JQL query string for search_tickets. Example: 'project = PROJ AND status = "In Progress" ORDER BY updated DESC'. |
| `level_of_details` | no | `string` — How much data to return for get_ticket. Omit to use the default ('basic'). Options: 'basic' — summary, status, priority, assignee, rendered description, and rendered comments (best for reading a ticket in full); 'basic_search' — lightweight fields only, no description or comments (best when you only need to identify the ticket); 'full' — all Jira fields plus rendered HTML (verbose, use sparingly); — enum: basic, basic_search, full, changelog |
| `max_results` | no | `integer` — Maximum number of issues to return for search_tickets. Defaults to 25, capped at 999. — default: 25 |

### `linkedin`

**Source:** `src/tools/linkedin.rs`

**Description:** Manage LinkedIn: create posts, list your posts, comment, react, delete posts, view engagement, get profile info, and read the configured content strategy. Requires LINKEDIN_* credentials in .env file.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The LinkedIn action to perform — enum: create_post, list_posts, comment, react, delete_post, get_engagement, get_profile, get_content_strategy |
| `article_title` | no | `string` — Title for the article (requires article_url) |
| `article_url` | no | `string` — URL for link preview in a post |
| `count` | no | `integer` — Number of posts to retrieve (default 10, max 50) |
| `generate_image` | no | `boolean` — Generate an AI image for the post (requires [linkedin.image] config). Falls back to branded SVG card if all providers fail. |
| `image_prompt` | no | `string` — Custom prompt for image generation. If omitted, a prompt is derived from the post text. |
| `post_id` | no | `string` — LinkedIn post URN identifier |
| `reaction_type` | no | `string` — Type of reaction to add to a post — enum: LIKE, CELEBRATE, SUPPORT, LOVE, INSIGHTFUL, FUNNY |
| `scheduled_at` | no | `string` — Schedule the post for future publication. ISO 8601 / RFC 3339 timestamp, e.g. '2026-03-17T08:00:00Z'. The post is saved as a draft with scheduledPublishTime on LinkedIn. |
| `text` | no | `string` — Post or comment text content |
| `visibility` | no | `string` — Post visibility (default: PUBLIC) — enum: PUBLIC, CONNECTIONS |

### `microsoft365`

**Source:** `src/tools/microsoft365/mod.rs`

**Description:** Microsoft 365 integration: manage Outlook mail, Teams messages, Calendar events, OneDrive files, and SharePoint search via Microsoft Graph API

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The Microsoft 365 action to perform — enum: mail_list, mail_send, teams_message_list, teams_message_send, calendar_events_list, calendar_event_create, calendar_event_delete, onedrive_list, onedrive_download, sharepoint_search |
| `attendees` | no | `array` — Attendee email addresses (for calendar_event_create) |
| `body` | no | `string` — Message body text |
| `channel_id` | no | `string` — Teams channel ID (for teams_message_list/send) |
| `end` | no | `string` — End datetime in ISO 8601 format (for calendar actions) |
| `event_id` | no | `string` — Calendar event ID (for calendar_event_delete) |
| `folder` | no | `string` — Mail folder ID (for mail_list, e.g. 'inbox', 'sentitems') |
| `item_id` | no | `string` — OneDrive item ID (for onedrive_download) |
| `max_size` | no | `integer` — Maximum download size in bytes (for onedrive_download, default 10MB) |
| `path` | no | `string` — OneDrive folder path (for onedrive_list) |
| `query` | no | `string` — Search query (for sharepoint_search) |
| `start` | no | `string` — Start datetime in ISO 8601 format (for calendar actions) |
| `subject` | no | `string` — Email subject or calendar event subject |
| `team_id` | no | `string` — Teams team ID (for teams_message_list/send) |
| `to` | no | `array` — Recipient email addresses (for mail_send) |
| `top` | no | `integer` — Maximum number of items to return (default 25) |

### `notion`

**Source:** `src/tools/notion_tool.rs`

**Description:** Interact with Notion: query databases, read/create/update pages, and search the workspace.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The Notion API action to perform — enum: query_database, read_page, create_page, update_page, search |
| `database_id` | no | `string` — Database ID (required for query_database, optional for create_page) |
| `filter` | no | `object` — Notion filter object for query_database |
| `page_id` | no | `string` — Page ID (required for read_page and update_page) |
| `properties` | no | `object` — Properties object for create_page and update_page |
| `query` | no | `string` — Search query string for the search action |

## K. Project, cloud & security advisory

### `cloud_ops`

**Source:** `src/tools/cloud_ops.rs`

**Description:** Cloud transformation advisory tool. Analyzes IaC plans, assesses migration paths, reviews costs, and checks architecture against Well-Architected Framework pillars. Read-only: does not create or modify cloud resources.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The analysis action to perform. — enum: review_iac, assess_migration, cost_analysis, architecture_review |
| `cloud` | no | `string` — Target cloud provider (aws, azure, gcp). Uses configured default if omitted. |
| `input` | yes | `string` — For review_iac: IaC plan text or JSON content to analyze. For assess_migration: current architecture description text. For cost_analysis: billing data as CSV/JSON text. For architecture_review: architecture description text. Note: provide text content directly, not file paths. |

### `cloud_patterns`

**Source:** `src/tools/cloud_patterns.rs`

**Description:** Cloud pattern library. Given a workload description, suggests applicable cloud-native architectural patterns (containerization, serverless, database modernization, etc.).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — Action: 'match' to find patterns for a workload, 'list' to show all patterns. — enum: match, list |
| `cloud` | no | `string` — Filter patterns by cloud provider (aws, azure, gcp). Optional. |
| `workload` | no | `string` — Description of the workload to match patterns against (required for 'match'). |

### `project_intel`

**Source:** `src/tools/project_intel.rs`

**Description:** Project delivery intelligence: generate status reports, detect risks, draft client updates, summarize sprints, and estimate effort. Read-only analysis tool.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The analysis action to perform — enum: status_report, risk_scan, draft_update, sprint_summary, effort_estimate |
| `audience` | no | `string` — Target audience (for draft_update) — enum: client, internal |
| `blocked` | no | `string` — Blocked items (for sprint_summary) |
| `blockers` | no | `string` — Current blockers (for risk_scan) |
| `completed` | no | `string` — Completed items (for sprint_summary) |
| `concerns` | no | `string` — Items requiring attention (for draft_update) |
| `deadlines` | no | `string` — Deadline information (for risk_scan) |
| `git_log` | no | `string` — Git log summary text (for status_report) |
| `highlights` | no | `string` — Key highlights for the update (for draft_update) |
| `in_progress` | no | `string` — In-progress items (for sprint_summary) |
| `jira_summary` | no | `string` — Jira/issue tracker summary (for status_report) |
| `language` | no | `string` — Report language: en, de, fr, it (default from config) |
| `notes` | no | `string` — Additional notes or context |
| `period` | no | `string` — Reporting period: week, sprint, or month (for status_report) |
| `project_name` | no | `string` — Project name (for status_report, risk_scan, draft_update) |
| `sprint_dates` | no | `string` — Sprint date range (for sprint_summary) |
| `tasks` | no | `string` — Task descriptions, one per line (for effort_estimate) |
| `tone` | no | `string` — Communication tone (for draft_update) — enum: formal, casual |
| `velocity` | no | `string` — Team velocity data (for risk_scan, sprint_summary) |

### `security_ops`

**Source:** `src/tools/security_ops.rs`

**Description:** Security operations tool for managed cybersecurity services. Actions: triage_alert (classify/prioritize alerts), run_playbook (execute incident response steps), parse_vulnerability (parse scan results), generate_report (create security posture reports), list_playbooks (list available playbooks), alert_stats (summarize alert metrics).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | yes | `string` — The security operation to perform — enum: triage_alert, run_playbook, parse_vulnerability, generate_report, list_playbooks, alert_stats |
| `alert` | no | `object` — Alert JSON for triage_alert (requires: type, severity; optional: source, description) |
| `alert_severity` | no | `string` — Alert severity context for run_playbook |
| `alert_stats` | no | `object` — Alert statistics to include in generate_report |
| `alerts` | no | `array` — Array of alert objects for alert_stats |
| `client_name` | no | `string` — Client name for generate_report |
| `period` | no | `string` — Reporting period for generate_report |
| `playbook` | no | `string` — Playbook name for run_playbook |
| `scan_data` | no | `object` — Vulnerability scan data (JSON string or object) for parse_vulnerability |
| `step` | no | `integer` — 0-based step index for run_playbook |
| `vuln_summary` | no | `string` — Vulnerability summary to include in generate_report |

### `sop_advance`

**Source:** `src/tools/sop_advance.rs`

**Description:** Report the result of the current SOP step and advance to the next step. Provide the run_id, whether the step succeeded or failed, and a brief output summary.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `output` | yes | `string` — Brief summary of what happened in this step |
| `run_id` | yes | `string` — The run ID to advance |
| `status` | yes | `string` — Result status of the current step — enum: completed, failed, skipped |

### `sop_approve`

**Source:** `src/tools/sop_approve.rs`

**Description:** Approve a pending SOP step that is waiting for operator approval. Returns the step instruction to execute. Use sop_status to see which runs are waiting.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `run_id` | yes | `string` — The run ID to approve |

### `sop_execute`

**Source:** `src/tools/sop_execute.rs`

**Description:** Manually trigger a Standard Operating Procedure (SOP) by name. Returns the run ID and first step instruction. Use sop_list to see available SOPs.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `name` | yes | `string` — Name of the SOP to execute |
| `payload` | no | `string` — Optional trigger payload (JSON string) |

### `sop_list`

**Source:** `src/tools/sop_list.rs`

**Description:** List all loaded Standard Operating Procedures (SOPs) with their triggers, priority, step count, and active run count. Optionally filter by name or priority.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `filter` | no | `string` — Filter SOPs by name substring or priority (low/normal/high/critical) |

### `sop_status`

**Source:** `src/tools/sop_status.rs`

**Description:** Query SOP execution status. Provide run_id for a specific run, or sop_name to list runs for that SOP. With no arguments, shows all active runs.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `include_gate_status` | no | `boolean` — Include trust phase and gate evaluation status |
| `include_metrics` | no | `boolean` — Include aggregated SOP metrics (completion rate, deviation rate, intervention counts, windowed variants) |
| `run_id` | no | `string` — Specific run ID to query |
| `sop_name` | no | `string` — SOP name to list runs for |

## L. Orchestration & external CLIs

### `claude_code`

**Source:** `src/tools/claude_code.rs`

**Description:** Delegate a coding task to Claude Code (claude -p). Supports file editing, bash execution, structured output, and multi-turn sessions. Use for complex coding work that benefits from Claude Code's full agent loop.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `allowed_tools` | no | `array` — Override the default tool allowlist (e.g. ["Read", "Edit", "Bash", "Write"]) |
| `json_schema` | no | `object` — Request structured output conforming to this JSON Schema |
| `prompt` | yes | `string` — The coding task to delegate to Claude Code |
| `session_id` | no | `string` — Resume a previous Claude Code session by its ID |
| `system_prompt` | no | `string` — Override or append a system prompt for this invocation |
| `working_directory` | no | `string` — Working directory within the workspace (must be inside workspace_dir) |

### `claude_code_runner`

**Source:** `src/tools/claude_code_runner.rs`

**Description:** Spawn a Claude Code task in a tmux session with live Slack progress updates and SSH handoff. Returns immediately with session ID and attach command.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `prompt` | yes | `string` — The coding task to delegate to Claude Code |
| `slack_channel` | no | `string` — Slack channel ID to post progress updates to |
| `working_directory` | no | `string` — Working directory within the workspace (must be inside workspace_dir) |

### `codex_cli`

**Source:** `src/tools/codex_cli.rs`

**Description:** Delegate a coding task to Codex CLI (codex -q). Supports file editing and bash execution. Use for complex coding work that benefits from Codex's full agent loop.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `prompt` | yes | `string` — The coding task to delegate to Codex |
| `working_directory` | no | `string` — Working directory within the workspace (must be inside workspace_dir) |

### `delegate`

**Source:** `src/tools/delegate.rs`

**Description:** Delegate a subtask to a specialized agent. Use when: a task benefits from a different model (e.g. fast summarization, deep reasoning, code generation). The sub-agent runs a single prompt by default; with agentic=true it can iterate with a filtered tool-call loop. Supports background execution (returns a task_id immediately) and parallel execution (runs multiple agents concurrently). Use action='check_result' with a task_id to retrieve background results.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `action` | no | `string` — enum: delegate, check_result, list_results, cancel_task; default: "delegate" |
| `agent` | no | `string` |
| `background` | no | `boolean` — default: false |
| `context` | no | `string` — Optional context to prepend (e.g. relevant code, prior findings) |
| `parallel` | no | `array` |
| `prompt` | no | `string` — The task/prompt to send to the sub-agent |
| `task_id` | no | `string` |

### `gemini_cli`

**Source:** `src/tools/gemini_cli.rs`

**Description:** Delegate a coding task to Gemini CLI (gemini -p). Supports file editing and shell execution. Use for complex coding work that benefits from Gemini CLI's full agent loop.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `prompt` | yes | `string` — The coding task to delegate to Gemini CLI |
| `working_directory` | no | `string` — Working directory within the workspace (must be inside workspace_dir) |

### `opencode_cli`

**Source:** `src/tools/opencode_cli.rs`

**Description:** Delegate a coding task to OpenCode CLI (opencode run). Supports file editing and bash execution. Use for complex coding work that benefits from OpenCode's full agent loop.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `prompt` | yes | `string` — The coding task to delegate to OpenCode |
| `working_directory` | no | `string` — Working directory within the workspace (must be inside workspace_dir) |

### `swarm`

**Source:** `src/tools/swarm.rs`

**Description:** Orchestrate a swarm of agents to collaboratively handle a task. Supports sequential (pipeline), parallel (fan-out/fan-in), and router (LLM-selected) strategies.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `context` | no | `string` — Optional context to include (e.g. relevant code, prior findings) |
| `prompt` | yes | `string` — The task/prompt to send to the swarm |
| `swarm` | yes | `string` |

## M. Productivity & misc

### `calculator`

**Source:** `src/tools/calculator.rs`

**Description:** Perform arithmetic and statistical calculations. Supports 25 functions: add, subtract, divide, multiply, pow, sqrt, abs, modulo, round, log, ln, exp, factorial, sum, average, median, mode, min, max, range, variance, stdev, percentile, count, percentage_change, clamp. Use this tool whenever you need to compute a numeric result instead of guessing.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `a` | no | `number` — First operand. Required for: pow, modulo, percentage_change. |
| `b` | no | `number` — Second operand. Required for: pow, modulo, percentage_change. |
| `base` | no | `number` — Logarithm base (default: 10). Optional for: log. |
| `decimals` | no | `integer` — Number of decimal places for rounding. Required for: round. |
| `function` | yes | `string` — enum: 26 values (see source) |
| `max_val` | no | `number` — Maximum bound. Required for: clamp. |
| `min_val` | no | `number` — Minimum bound. Required for: clamp. |
| `p` | no | `integer` — Percentile rank (0-100). Required for: percentile. |
| `values` | no | `array` — Array of numeric values. Required for: add, subtract, divide, multiply, sum, average, median, mode, min, max, range, variance, stdev, percentile, count. |
| `x` | no | `number` — Input number. Required for: sqrt, abs, exp, ln, log, factorial. |

### `weather`

**Source:** `src/tools/weather_tool.rs`

**Description:** Get current weather conditions and up to 3-day forecast for any location worldwide. Supports city names (in any language or script), airport IATA codes (e.g. 'LAX'), GPS coordinates (e.g. '51.5,-0.1'), postal/zip codes, and domain-based geolocation. No API key required. Units default to metric (°C, km/h, mm) but can be switched to imperial (°F, mph, inches) per request.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `days` | no | `integer` |
| `location` | yes | `string` |
| `units` | no | `string` — enum: metric, imperial |

## N. MCP, search bridge & other runtime tools

### `tool_search`

**Source:** `src/tools/tool_search.rs`

**Description:** Fetch full schema definitions for deferred MCP tools so they can be called. Use "select:name1,name2 " for exact match or keywords to search.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `max_results` | no | `number` — Maximum number of results to return (default: 5) — default: DEFAULT_MAX_RESULTS |
| `query` | yes | `string` — Query to find deferred tools. Use "select:<tool_name>" for direct selection, or keywords to search. |

## O. Hardware & device helpers

### `hardware_board_info`

**Source:** `src/tools/hardware_board_info.rs`

**Description:** Return full board info (chip, architecture, memory map) for connected hardware. Use when: user asks for 'board info', 'what board do I have', 'connected hardware', 'chip info', 'what hardware', or 'memory map'.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `board` | no | `string` — Optional board name (e.g. nucleo-f401re). If omitted, returns info for first configured board. |

### `hardware_memory_map`

**Source:** `src/tools/hardware_memory_map.rs`

**Description:** Return the memory map (flash and RAM address ranges) for connected hardware. Use when: user asks for 'upper and lower memory addresses', 'memory map', 'address space', or 'readable addresses'. Returns flash/RAM ranges from datasheets.

| Parameter | Required | Notes |
|-----------|----------|--------|
| `board` | no | `string` — Optional board name (e.g. nucleo-f401re, arduino-uno). If omitted, returns map for first configured board. |

### `hardware_memory_read`

**Source:** `src/tools/hardware_memory_read.rs`

**Description:** Read actual memory/register values from Nucleo via USB. Use when: user asks to 'read register values', 'read memory at address', 'dump memory', 'lower memory 0-126', or 'give address and value'. Returns hex dump. Requires Nucleo connected via USB and probe feature. Params: address (hex, e.g. 0x20000000 for RAM start), length (bytes, default 128).

| Parameter | Required | Notes |
|-----------|----------|--------|
| `address` | no | `string` — Memory address in hex (e.g. 0x20000000 for RAM start). Default: 0x20000000 (RAM base). |
| `board` | no | `string` — Board name (nucleo-f401re). Optional if only one configured. |
| `length` | no | `integer` — Number of bytes to read (default 128, max 256). |


## See also

- Generator: `scripts/generate_tool_param_docs.py`
- Registry: `src/tools/mod.rs` (`all_tools`, `all_tools_with_runtime`)
