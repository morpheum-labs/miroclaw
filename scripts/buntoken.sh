#!/usr/bin/env bash
# Print the current bun-browser daemon Bearer token (stdout only).
#
# Resolution order:
#   1. BUN_BROWSER_TOKEN (non-empty)
#   2. ~/.bun-browser/daemon.json "token" field
#
# Usage:
#   bash scripts/buntoken.sh

set -euo pipefail

DAEMON_JSON="${BUN_BROWSER_DAEMON_JSON:-${HOME}/.bun-browser/daemon.json}"

if [[ -n "${BUN_BROWSER_TOKEN:-}" ]]; then
  printf '%s\n' "$(printf '%s' "$BUN_BROWSER_TOKEN" | tr -d '[:space:]')"
  exit 0
fi

if [[ ! -f "$DAEMON_JSON" ]]; then
  echo "buntoken: missing ${DAEMON_JSON} (set BUN_BROWSER_TOKEN or start bun-browser)" >&2
  exit 1
fi

read_token_from_json() {
  local path="$1"
  if command -v jq >/dev/null 2>&1; then
    jq -r '.token // empty' "$path"
    return
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$path" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as f:
    data = json.load(f)
print((data.get("token") or "").strip())
PY
    return
  fi
  # Minimal fallback when jq/python3 are unavailable.
  sed -n 's/.*"token"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$path" | head -n1
}

token="$(read_token_from_json "$DAEMON_JSON" | tr -d '\r\n')"
if [[ -z "$token" ]]; then
  echo "buntoken: no token in ${DAEMON_JSON}" >&2
  exit 1
fi

printf '%s\n' "$token"
