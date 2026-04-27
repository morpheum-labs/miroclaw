#!/usr/bin/env python3
"""
Extract tool name, description, and parameters_schema json!() blocks from Rust tool sources
and print Markdown tool sections. Run from repo root: python3 scripts/generate_tool_param_docs.py
"""
from __future__ import annotations

import json
import re
from pathlib import Path

TOOLS_DIR = Path("src/tools")
EXTRA = [Path("src/tools/microsoft365/mod.rs")]

# Primary doc category per tool (see docs/tools.md category map). Order of sections below.
_CATEGORIES: list[tuple[str, str, frozenset[str]]] = [
    (
        "A",
        "Workspace, shell, Git & files",
        frozenset(
            {
                "shell",
                "file_read",
                "file_write",
                "file_edit",
                "glob_search",
                "content_search",
                "git_operations",
                "pdf_read",
            }
        ),
    ),
    (
        "B",
        "Backups, retention & multi-workspace",
        frozenset({"backup", "data_management", "workspace"}),
    ),
    (
        "C",
        "Long-term memory & knowledge",
        frozenset(
            {
                "memory_store",
                "memory_recall",
                "memory_forget",
                "memory_purge",
                "knowledge",
                "discord_search",
            }
        ),
    ),
    ("D", "Skills", frozenset({"read_skill"})),
    (
        "E",
        "Model routing, LLM & HTTP stack",
        frozenset(
            {
                "model_switch",
                "model_routing_config",
                "proxy_config",
                "llm_task",
                "vi_verify",
            }
        ),
    ),
    (
        "F",
        "Scheduling & cron",
        frozenset(
            {
                "schedule",
                "cron_add",
                "cron_list",
                "cron_remove",
                "cron_update",
                "cron_run",
                "cron_runs",
            }
        ),
    ),
    (
        "G",
        "Web, HTTP, search & browsers",
        frozenset(
            {
                "http_request",
                "web_fetch",
                "web_search_tool",
                "text_browser",
                "browser",
                "browser_open",
                "browser_delegate",
            }
        ),
    ),
    (
        "H",
        "Media, vision & live canvas",
        frozenset({"screenshot", "image_info", "image_gen", "canvas"}),
    ),
    (
        "I",
        "Channel UX & sessions",
        frozenset(
            {
                "ask_user",
                "reaction",
                "poll",
                "sessions_list",
                "sessions_history",
                "sessions_send",
                "pushover",
            }
        ),
    ),
    (
        "J",
        "SaaS & work apps",
        frozenset(
            {
                "composio",
                "notion",
                "jira",
                "microsoft365",
                "google_workspace",
                "linkedin",
            }
        ),
    ),
    (
        "K",
        "Project, cloud & security advisory",
        frozenset(
            {
                "project_intel",
                "cloud_ops",
                "cloud_patterns",
                "security_ops",
                "sop_list",
                "sop_execute",
                "sop_advance",
                "sop_approve",
                "sop_status",
            }
        ),
    ),
    (
        "L",
        "Orchestration & external CLIs",
        frozenset(
            {
                "delegate",
                "swarm",
                "claude_code",
                "claude_code_runner",
                "codex_cli",
                "gemini_cli",
                "opencode_cli",
            }
        ),
    ),
    ("M", "Productivity & misc", frozenset({"calculator", "weather"})),
    ("N", "MCP, search bridge & other runtime tools", frozenset({"tool_search"})),
    (
        "O",
        "Hardware & device helpers",
        frozenset(
            {
                "hardware_board_info",
                "hardware_memory_map",
                "hardware_memory_read",
            }
        ),
    ),
]


def category_key_for_tool(name: str) -> str:
    for key, _title, members in _CATEGORIES:
        if name in members:
            return key
    return "Z"


def category_title(letter: str) -> str:
    for key, title, _ in _CATEGORIES:
        if key == letter:
            return title
    return "Other tools"


def find_parameters_schema_region(text: str) -> str | None:
    m = re.search(r"fn parameters_schema\(&self\)\s*->[^{]*\{", text)
    if not m:
        return None
    return text[m.start() :]


def extract_json_block_after(region: str) -> str | None:
    """First json!({ ... }) in region, brace-balanced; respects Rust string quotes."""
    pos = region.find("json!({")
    if pos == -1:
        pos = region.find("json! ({")
    if pos == -1:
        return None
    j = pos + region[pos:].find("{")
    depth = 0
    k = j
    while k < len(region):
        c = region[k]
        if c == '"':
            k += 1
            while k < len(region):
                if region[k] == "\\":
                    k += 2
                    continue
                if region[k] == '"':
                    k += 1
                    break
                k += 1
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return region[j + 1 : k]
        k += 1
    return None


def tool_name_from_source(text: str) -> str | None:
    m = re.search(r"fn name\(&self\)\s*->\s*&str\s*\{\s*\"([^\"]+)\"\s*\}", text)
    return m.group(1) if m else None


def tool_description_from_source(text: str) -> str:
    m = re.search(
        r"fn description\(&self\)\s*->\s*&str\s*\{([\s\S]*?)\n\s*\}",
        text,
    )
    if not m:
        return ""
    body = m.group(1)
    c = re.search(r"concat!\s*\(([\s\S]*?)\)\s*$", body.strip(), re.S)
    if c:
        parts = re.findall(r"\"((?:\\.|[^\"\\])*)\"", c.group(1), re.S)
        out: list[str] = []
        for p in parts:
            out.append(json.loads('"' + p + '"'))
        return " ".join(out).strip()
    # Collect all "..." string literals in the body and join (handles \ line continuation)
    parts = re.findall(r"\"((?:\\.|[^\"\\])*)\"", body, re.S)
    if not parts:
        return body.strip()[:500] if body else ""
    out_parts: list[str] = []
    for p in parts:
        try:
            out_parts.append(json.loads('"' + p + '"'))
        except json.JSONDecodeError:
            out_parts.append(p)
    raw = " ".join(out_parts)
    # Drop Rust line-continuation backslashes that ended up in the string
    raw = re.sub(r"\s*\\\s*", " ", raw)
    raw = re.sub(r"\s+", " ", raw)
    return raw.strip()


def parse_properties_block(block: str) -> tuple[list[str], dict[str, str]]:
    """Return required list and prop_name -> short notes string from raw inner JSON-ish block."""
    req: list[str] = []
    rm = re.search(r"\"required\"\s*:\s*\[([^\]]*)\]", block, re.S)
    if rm:
        req = re.findall(r'"([^"]+)"', rm.group(1))

    pm = re.search(r"\"properties\"\s*:\s*\{", block)
    if not pm:
        return req, {}

    i = pm.end() - 1
    assert block[i] == "{"
    depth = 0
    k = i
    while k < len(block):
        c = block[k]
        if c == '"':
            k += 1
            while k < len(block):
                if block[k] == "\\":
                    k += 2
                    continue
                if block[k] == '"':
                    k += 1
                    break
                k += 1
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                inner = block[i + 1 : k]
                break
        k += 1
    else:
        return req, {}

    props: dict[str, str] = {}
    p = 0
    while p < len(inner):
        m = re.match(r'\s*"([a-zA-Z0-9_]+)"\s*:\s*\{', inner[p:])
        if not m:
            p += 1
            continue
        name = m.group(1)
        start = p + m.end() - 1
        depth = 0
        q = start
        while q < len(inner):
            c = inner[q]
            if c == '"':
                q += 1
                while q < len(inner):
                    if inner[q] == "\\":
                        q += 2
                        continue
                    if inner[q] == '"':
                        q += 1
                        break
                    q += 1
                continue
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    q += 1
                    break
            q += 1
        chunk = inner[start + 1 : q - 1]
        tym = re.search(
            r'"type"\s*:\s*(\[([^\]]+)\]|"(?:string|number|boolean|integer|object|array)")',
            chunk,
        )
        if tym and tym.group(1).startswith("["):
            typ = " | ".join(re.findall(r'"([^"]+)"', tym.group(1)))
        elif tym:
            typ = tym.group(1).strip('"')
        else:
            if "oneOf" in chunk or "anyOf" in chunk:
                typ = "oneOf/anyOf"
            else:
                typ = "object"
        ddesc = re.search(
            r'"description"\s*:\s*("(?:\\.|[^"\\])*"(?:\s*\\\s*\n\s*"(?:\\.|[^"\\])*")*)',
            chunk,
            re.S,
        )
        desc = ""
        if ddesc:
            raw = ddesc.group(1)
            # join rust multiline string
            parts = re.findall(r'"((?:\\.|[^"\\])*)"', raw)
            if parts:
                try:
                    desc = " ".join(json.loads('"' + p + '"') for p in parts)
                except Exception:
                    desc = parts[0]
        en = re.findall(r'"enum"\s*:\s*\[([^\]]+)\]', chunk, re.S)
        extra = []
        if en:
            vals = re.findall(r'"([^"]+)"', en[0])
            if len(vals) <= 12:
                extra.append("enum: " + ", ".join(vals))
            else:
                extra.append(f"enum: {len(vals)} values (see source)")
        df = re.search(r'"default"\s*:\s*([^,}\n]+)', chunk)
        if df and "description" not in df.group(1):
            extra.append("default: " + df.group(1).strip())
        note = f"`{typ}`"
        if desc:
            note += " — " + desc.replace("\n", " ")[:400]
        if extra:
            note += " — " + "; ".join(extra)
        props[name] = note
        p = q
    return req, props


def block_for_file(path: Path) -> str | None:
    text = path.read_text()
    region = find_parameters_schema_region(text)
    if not region:
        return None
    return extract_json_block_after(region)


def emit_tool_md(name: str, description: str, path: str, block: str | None) -> str:
    lines: list[str] = []
    lines.append(f"### `{name}`")
    lines.append("")
    lines.append(f"**Source:** `{path}`")
    lines.append("")
    lines.append(f"**Description:** {description or '—'}")
    lines.append("")
    if not block or '"properties"' not in block:
        lines.append("*Parameters: see `parameters_schema` in source (non-`json!` or generated schema).*")
        lines.append("")
        return "\n".join(lines)
    required, props = parse_properties_block(block)
    if not props:
        empty_obj = re.search(
            r'"properties"\s*:\s*\{\s*\}\s*,\s*"additionalProperties"\s*:\s*false',
            block,
        )
        if empty_obj:
            note = "No parameters; pass an empty object `{}`."
        else:
            note = "No `properties` in schema (empty object, nested schema, or dynamic). See source."
        lines.append("| Parameter | Required | Notes |")
        lines.append("|-----------|----------|--------|")
        lines.append(f"| — | — | {note} |")
    else:
        lines.append("| Parameter | Required | Notes |")
        lines.append("|-----------|----------|--------|")
        for pname, note in sorted(props.items()):
            req = "yes" if pname in required else "no"
            safe = note.replace("|", "\\|")
            lines.append(f"| `{pname}` | {req} | {safe} |")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    files: list[Path] = []
    for p in sorted(TOOLS_DIR.glob("*.rs")):
        if p.name in {
            "mod.rs",
            "traits.rs",
            "schema.rs",
            "mcp_serve.rs",
            "mcp_deferred.rs",
            "mcp_protocol.rs",
            "mcp_client.rs",
            "mcp_transport.rs",
            "report_templates.rs",
            "cli_discovery.rs",
            "web_search_provider_routing.rs",
        }:
            continue
        files.append(p)
    for p in EXTRA:
        if p.exists():
            files.append(p)

    chunks: list[tuple[str, str]] = []
    for p in files:
        text = p.read_text()
        if "impl Tool for" not in text or "fn parameters_schema" not in text:
            continue
        # Multiple impls (sessions.rs): split by struct name blocks
        parts = re.split(r"(?m)^#\[async_trait\]", text)
        # simpler: one file may have multiple "fn name" + parameters_schema pairs
        impl_blocks = list(re.finditer(r"(?m)^impl Tool for [^{]+\{", text))
        for i, im in enumerate(impl_blocks):
            start = im.start()
            end = impl_blocks[i + 1].start() if i + 1 < len(impl_blocks) else len(text)
            segment = text[start:end]
            if "fn parameters_schema" not in segment:
                continue
            nm = tool_name_from_source(segment)
            if not nm:
                continue
            desc = tool_description_from_source(segment)
            reg = find_parameters_schema_region(segment)
            blk = extract_json_block_after(reg) if reg else None
            chunks.append(
                (nm, emit_tool_md(nm, desc, str(p), blk)),
            )
    skip_names = frozenset(
        {
            "noop",
            "echo_tool",
            "mcp_fake",
            "contract_ping",
            "mock",
        }
    )
    # Dedupe by name (first wins), preserve first impl per name
    seen: set[str] = set()
    by_name: dict[str, str] = {}
    for name, md in sorted(chunks, key=lambda x: x[0].lower()):
        if name in seen or name in skip_names:
            continue
        seen.add(name)
        by_name[name] = md

    out: list[str] = []
    # Category A–O, then Z (uncategorized)
    used_letters: list[str] = [c[0] for c in _CATEGORIES] + ["Z"]
    for letter in used_letters:
        names = [n for n in by_name if category_key_for_tool(n) == letter]
        if not names:
            continue
        title = category_title(letter)
        out.append(f"## {letter}. {title}")
        out.append("")
        for name in sorted(names, key=str.lower):
            out.append(by_name[name])
    print("\n".join(out))


if __name__ == "__main__":
    main()
