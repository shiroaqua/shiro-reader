#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/build-common.sh
source "${SCRIPT_DIR}/lib/build-common.sh"

DEBUG=0
OFFLINE=0
WITH_SERVER=0

usage() {
  cat <<'EOF'
Usage: scripts/build-windows.sh [OPTIONS]

Build and verify the ShiroReader Windows x64 (x86_64-pc-windows-msvc) NSIS installer.
Cross-compiles via cargo-xwin + makensis when executed on Linux.

Options:
  --debug        Build debug variant instead of release
  --offline      Use offline Cargo/xwin/PDFium caches only
  --with-server  Also compile shiro-reader-server.exe for x86_64-pc-windows-msvc
  -h, --help     Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)
      DEBUG=1
      shift
      ;;
    --offline)
      OFFLINE=1
      shift
      ;;
    --with-server)
      WITH_SERVER=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "Unknown option: $1 (see --help)"
      ;;
  esac
done

require_cmd rustup "Install Rust via rustup."
require_cmd cargo "Install Cargo."

if ! rustup target list --installed | grep -qx 'x86_64-pc-windows-msvc'; then
  die "Rust target 'x86_64-pc-windows-msvc' is not installed. Run: rustup target add x86_64-pc-windows-msvc"
fi

HOST_OS="$(resolve_host_os)"
if [[ "$HOST_OS" == "linux" ]]; then
  require_cmd cargo-xwin "Install cargo-xwin for Windows cross-compilation on Linux."
  require_cmd makensis "Install NSIS (makensis) for generating Windows installers on Linux."
  if [[ -z "${XWIN_CACHE_DIR:-}" ]]; then
    if [[ -d "/opt/xwin-cache" && -w "/opt/xwin-cache" ]]; then
      export XWIN_CACHE_DIR="/opt/xwin-cache"
    else
      export XWIN_CACHE_DIR="${REPO_ROOT}/.cache/xwin"
    fi
  fi
fi

ensure_node_deps

cmd=(node "$PACKAGING_SCRIPT" build --target windows-x64)
[[ "$DEBUG" -eq 1 ]] && cmd+=(--debug)
[[ "$OFFLINE" -eq 1 ]] && cmd+=(--offline)

log "Building Windows x64 client (host=${HOST_OS}, profile=$([[ "$DEBUG" -eq 1 ]] && echo debug || echo release))..."
(cd "$REPO_ROOT" && "${cmd[@]}")
print_report_summary "windows-x64"

if [[ "$WITH_SERVER" -eq 1 ]]; then
  server_runner=(cargo)
  if [[ "$HOST_OS" == "linux" ]]; then
    server_runner=(cargo-xwin)
  fi
  server_args=(build -q --locked -p shiro-reader-server --target x86_64-pc-windows-msvc)
  [[ "$DEBUG" -eq 0 ]] && server_args+=(--release)
  [[ "$OFFLINE" -eq 1 ]] && server_args+=(--offline)
  log "Building Windows x64 server binary..."
  (cd "$REPO_ROOT" && "${server_runner[@]}" "${server_args[@]}")
  profile_dir="$([[ "$DEBUG" -eq 1 ]] && echo debug || echo release)"
  log "Server binary: target/x86_64-pc-windows-msvc/${profile_dir}/shiro-reader-server.exe"
fi
