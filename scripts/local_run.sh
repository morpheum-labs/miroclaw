#!/usr/bin/env bash
# Run Miroclaw from this repository without `cargo install --path .`.
#
# Usage (from repo root):
#   bash scripts/local_run.sh [arguments passed to miroclaw]
#
# If `miroclaw` already exists under the Cargo target directory (release or debug),
# it is executed directly; otherwise `cargo run` builds and runs.
#
# The workspace root comes from Cargo (`cargo locate-project --workspace`); the
# binary is searched there and under the resolved target-dir (custom or default).
#
# With no arguments, runs `miroclaw gateway` (webhooks + HTTP API). Web dashboard static files are
# off by default here (`MIROCLAW_WEBUI_DISABLED=1`); set MIROCLAW_WEBUI_DISABLED=0 and supply a dist/
# path via config or MIROCLAW_WEBUI_EXTERNAL_PATH to enable the UI.
#
# Examples:
#   bash scripts/local_run.sh status
#   bash scripts/local_run.sh gateway
#   bash scripts/local_run.sh daemon
#   bash scripts/local_run.sh agent -m "Hello"
#   bash scripts/local_run.sh onboard
#
# Environment:
#   MIROCLAW_LOCAL_DEBUG=1       Use debug profile (target/debug and `cargo run` without --release).
#   MIROCLAW_WEBUI_DISABLED      Defaults to 1 for this script (API-only gateway). Set to 0 to serve
#                                a dashboard from [webui].external_path / MIROCLAW_WEBUI_EXTERNAL_PATH.
#
# Agent workspace (data/config) defaults to ~/miroclaw_space when neither MIROCLAW_WORKSPACE nor
# MIROCLAW_CONFIG_DIR is set. See src/config/schema.rs (resolve_runtime_config_dirs).
#
# Web dashboard is skipped unless you explicitly enable it (see MIROCLAW_WEBUI_DISABLED below).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

_workspace_root() {
  local manifest
  manifest="$(cargo locate-project --workspace --message-format plain 2>/dev/null || true)"
  if [[ -n "${manifest}" ]]; then
    cd "$(dirname "${manifest}")" && pwd
  else
    printf '%s' "${repo_root}"
  fi
}

workspace_root="$(_workspace_root)"
cd "${workspace_root}"

# Rust/Cargo workspace root is `workspace_root`; Miroclaw agent files default under ~/miroclaw_space.
if [[ -n "${HOME:-}" && -z "${MIROCLAW_WORKSPACE:-}" && -z "${MIROCLAW_CONFIG_DIR:-}" ]]; then
  export MIROCLAW_WORKSPACE="${HOME}/miroclaw_space"
fi

if [[ "$#" -eq 0 ]]; then
  set -- gateway
fi

if [[ -z "${MIROCLAW_WEBUI_DISABLED+x}" ]]; then
  export MIROCLAW_WEBUI_DISABLED=1
fi

# Prefer $CARGO_TARGET_DIR, then `cargo metadata` (respects .cargo/config.toml target-dir),
# else ./target under the workspace root.
_resolve_target_dir() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    printf '%s' "${CARGO_TARGET_DIR}"
    return 0
  fi
  if command -v python3 >/dev/null 2>&1; then
    local td
    td="$(
      cargo metadata --format-version 1 --no-deps --quiet 2>/dev/null \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null
    )" || true
    if [[ -n "${td}" ]]; then
      printf '%s' "${td}"
      return 0
    fi
  fi
  printf '%s/target' "${workspace_root}"
}

# Ordered candidates: primary target-dir, then workspace ./target if it differs (e.g. custom [build].target-dir).
_miroclaw_bin_candidates() {
  local td td_ws profile
  td="$(_resolve_target_dir)"
  td_ws="${workspace_root}/target"
  if [[ -n "${MIROCLAW_LOCAL_DEBUG:-}" ]]; then
    profile=debug
  else
    profile=release
  fi
  printf '%s/%s/miroclaw\n' "${td}" "${profile}"
  if [[ "${td}" != "${td_ws}" ]]; then
    printf '%s/%s/miroclaw\n' "${td_ws}" "${profile}"
  fi
}

_bin=""
while IFS= read -r candidate; do
  [[ -z "${candidate}" ]] && continue
  if [[ -x "${candidate}" ]]; then
    _bin="${candidate}"
    break
  fi
done < <(_miroclaw_bin_candidates)

if [[ -n "${_bin}" ]]; then
  exec "${_bin}" "$@"
fi

if [[ -n "${MIROCLAW_LOCAL_DEBUG:-}" ]]; then
  exec cargo run --locked -- "$@"
else
  exec cargo run --locked --release -- "$@"
fi
