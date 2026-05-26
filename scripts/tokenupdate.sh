#!/usr/bin/env bash
# Sync the latest bun-browser token into agentbook settings and miroclaw config.
#
#   1. bash scripts/buntoken.sh  → latest token on stdout
#   2. agentbook taxonomy settings.toml  → [bun_browser] token
#   3. miroclaw config.toml              → [grok_browser] api
#
# Usage:
#   bash scripts/tokenupdate.sh
#
# Overrides:
#   AGENTBOOK_SETTINGS_TOML   default: /home/eflash31/agentbook/taxonomy/config/settings.toml
#   MIROCLAW_CONFIG_TOML      default: /home/eflash31/clawlaundry/miroclaw/config.toml
#   BUN_BROWSER_TOKEN, BUN_BROWSER_DAEMON_JSON  (passed through to buntoken.sh)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

AGENTBOOK_SETTINGS_TOML="${AGENTBOOK_SETTINGS_TOML:-/home/eflash31/agentbook/taxonomy/config/settings.toml}"
MIROCLAW_CONFIG_TOML="${MIROCLAW_CONFIG_TOML:-/home/eflash31/clawlaundry/miroclaw/config.toml}"

die() {
  echo "tokenupdate: $*" >&2
  exit 1
}

escape_toml_string() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '%s' "$s"
}

# Update `key = "..."` inside [section]; append key if section exists but key does not.
update_toml_string_key() {
  local file="$1" section="$2" key="$3" value="$4"
  local escaped tmp
  escaped="$(escape_toml_string "$value")"
  tmp="$(mktemp "${file}.XXXXXX")"

  awk -v section="$section" -v key="$key" -v val="$escaped" '
    function trim(s, t) {
      t = s
      gsub(/^[ \t\r\n]+/, "", t)
      gsub(/[ \t\r\n]+$/, "", t)
      return t
    }
    BEGIN {
      in_target = 0
      updated = 0
      saw_section = 0
    }
    /^\[/ {
      if (in_target && !updated) {
        print key " = \"" val "\""
        updated = 1
      }
      in_target = (trim($0) == "[" section "]")
      if (in_target) {
        saw_section = 1
      }
    }
    in_target && $0 ~ "^[ \t]*" key "[ \t]*=" {
      print key " = \"" val "\""
      updated = 1
      next
    }
    { print }
    END {
      if (!saw_section) {
        print ""
        print "[" section "]"
        print key " = \"" val "\""
      } else if (!updated) {
        print key " = \"" val "\""
      }
    }
  ' "$file" >"$tmp"

  mv "$tmp" "$file"
}

token="$(bash "${SCRIPT_DIR}/buntoken.sh")"
[[ -n "$token" ]] || die "empty token from buntoken.sh"

echo "bun-browser token: ${token}"

[[ -f "$AGENTBOOK_SETTINGS_TOML" ]] || die "missing ${AGENTBOOK_SETTINGS_TOML}"
[[ -f "$MIROCLAW_CONFIG_TOML" ]] || die "missing ${MIROCLAW_CONFIG_TOML}"

update_toml_string_key "$AGENTBOOK_SETTINGS_TOML" "bun_browser" "token" "$token"
update_toml_string_key "$MIROCLAW_CONFIG_TOML" "grok_browser" "api" "$token"

echo "updated [bun_browser].token in ${AGENTBOOK_SETTINGS_TOML}"
echo "updated [grok_browser].api in ${MIROCLAW_CONFIG_TOML}"
