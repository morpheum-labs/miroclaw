#!/usr/bin/env bash
# Probe whether Miroclaw's gateway is reachable on localhost.
#
# Usage:
#   bash scripts/check-miroclaw-online.sh          # human output, exit 0/1
#   bash scripts/check-miroclaw-online.sh --quiet  # exit code only (URL still on stderr)
#   bash scripts/check-miroclaw-online.sh --json   # print /health JSON on success
#
# Environment (matches src/config/schema.rs):
#   MIROCLAW_CONFIG, MIROCLAW_CONFIG_DIR, MIROCLAW_WORKSPACE
#   MIROCLAW_GATEWAY_PORT / PORT, MIROCLAW_GATEWAY_HOST / HOST
#
# Exit codes:
#   0 — gateway /health returned 2xx
#   1 — offline or unhealthy
#   2 — usage / missing dependency

set -euo pipefail

TIMEOUT_SECS="${MIROCLAW_HEALTH_TIMEOUT_SECS:-5}"
DEFAULT_PORT=42617
DEFAULT_HOST="127.0.0.1"

QUIET=0
JSON=0

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \?//'
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quiet | -q)
      QUIET=1
      shift
      ;;
    --json | -j)
      JSON=1
      shift
      ;;
    -h | --help)
      usage
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      ;;
  esac
done

log() {
  [[ "$QUIET" -eq 1 ]] || echo "$@"
}

err() {
  [[ "$QUIET" -eq 1 ]] || echo "$@" >&2
}

_expand_tilde() {
  case "$1" in
    "~") printf '%s' "${HOME:-}" ;;
    "~/"*) printf '%s/%s' "${HOME:-}" "${1#~/}" ;;
    *) printf '%s' "$1" ;;
  esac
}

_resolve_config_file() {
  if [[ -n "${MIROCLAW_CONFIG:-}" ]]; then
    _expand_tilde "${MIROCLAW_CONFIG}"
    return 0
  fi

  local base=""
  if [[ -n "${MIROCLAW_CONFIG_DIR:-}" ]]; then
    base="$(_expand_tilde "${MIROCLAW_CONFIG_DIR}")"
  elif [[ -n "${MIROCLAW_WORKSPACE:-}" ]]; then
    local ws="$(_expand_tilde "${MIROCLAW_WORKSPACE}")"
    if [[ -f "${ws}/config.toml" ]]; then
      printf '%s' "${ws}/config.toml"
      return 0
    fi
    if [[ "${ws##*/}" == "workspace" && -f "${ws%/*}/config.toml" ]]; then
      printf '%s' "${ws%/*}/config.toml"
      return 0
    fi
    base="${ws}"
  elif [[ -n "${HOME:-}" ]]; then
    if [[ -f "${HOME}/.miroclaw/profiles/main/config.toml" ]]; then
      printf '%s' "${HOME}/.miroclaw/profiles/main/config.toml"
      return 0
    fi
    base="${HOME}/.miroclaw"
  fi

  for candidate in \
    "${base}/config.toml" \
    "${base}/configuration.yaml" \
    "${base}/config.yaml"; do
    if [[ -n "${candidate}" && -f "${candidate}" ]]; then
      printf '%s' "${candidate}"
      return 0
    fi
  done

  printf '%s' "${base}/config.toml"
}

_read_gateway_from_config() {
  local config_file="$1"
  local port="$DEFAULT_PORT"
  local host="$DEFAULT_HOST"

  if [[ -f "${config_file}" ]]; then
    local parsed
    parsed="$(
      awk -v default_port="$DEFAULT_PORT" -v default_host="$DEFAULT_HOST" '
        BEGIN { in_gw=0; port=default_port; host=default_host }
        /^\[gateway\]/ { in_gw=1; next }
        /^\[/ { in_gw=0 }
        in_gw && $1 ~ /^port/ {
          gsub(/[^0-9]/, "", $3); if ($3 != "") port = $3
        }
        in_gw && $1 ~ /^host/ {
          gsub(/"/, "", $3); if ($3 != "") host = $3
        }
        END { print port, host }
      ' "${config_file}"
    )"
    port="${parsed%% *}"
    host="${parsed#* }"
  fi

  if [[ -n "${MIROCLAW_GATEWAY_PORT:-${PORT:-}}" ]]; then
    port="${MIROCLAW_GATEWAY_PORT:-${PORT}}"
  fi
  if [[ -n "${MIROCLAW_GATEWAY_HOST:-${HOST:-}}" ]]; then
    host="${MIROCLAW_GATEWAY_HOST:-${HOST}}"
  fi

  case "${host}" in
    0.0.0.0 | "[::]" | ::) host="127.0.0.1" ;;
  esac

  printf '%s %s' "${port}" "${host}"
}

_probe_health() {
  local url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -fsS --max-time "${TIMEOUT_SECS}" "${url}"
    return $?
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$url" "$TIMEOUT_SECS" <<'PY'
import sys
import urllib.request

url, timeout = sys.argv[1], float(sys.argv[2])
with urllib.request.urlopen(url, timeout=timeout) as resp:
    sys.stdout.write(resp.read().decode())
PY
    return $?
  fi
  err "Need curl or python3 to probe ${url}"
  exit 2
}

config_file="$(_resolve_config_file)"
parsed="$(_read_gateway_from_config "${config_file}")"
port="${parsed%% *}"
host="${parsed#* }"
url="http://${host}:${port}/health"

echo "Probing: ${url}" >&2

if command -v miroclaw >/dev/null 2>&1 && miroclaw status --format=exit-code >/dev/null 2>&1; then
  if [[ "$JSON" -eq 1 ]]; then
    _probe_health "${url}"
  else
    log "Miroclaw is online at ${url}"
  fi
  exit 0
fi

if body="$(_probe_health "${url}")"; then
  if [[ "$JSON" -eq 1 ]]; then
    printf '%s\n' "${body}"
  else
    log "Miroclaw is online at ${url}"
    if command -v python3 >/dev/null 2>&1; then
      printf '%s\n' "${body}" | python3 -m json.tool 2>/dev/null || printf '%s\n' "${body}"
    else
      printf '%s\n' "${body}"
    fi
  fi
  exit 0
fi

err "Miroclaw is offline (no response from ${url} within ${TIMEOUT_SECS}s)"
exit 1
