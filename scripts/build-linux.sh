#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/build-common.sh
source "${SCRIPT_DIR}/lib/build-common.sh"

FORMAT="deb"
DEBUG=0
OFFLINE=0
WITH_SERVER=0
RUN_SMOKE=0

usage() {
  cat <<'EOF'
Usage: scripts/build-linux.sh [OPTIONS]

Build and verify the ShiroReader Linux x64 (x86_64-unknown-linux-gnu) package.

Options:
  --format <deb|rpm|appimage>  Bundle format (default: deb)
  --debug                      Build debug variant instead of release
  --offline                    Use offline Cargo/PDFium caches only
  --with-server                Also compile shiro-reader-server binary
  --smoke                      Run Linux dynamic-library smoke verification after building
  -h, --help                   Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --format)
      [[ $# -ge 2 ]] || die "--format requires a value (deb|rpm|appimage)"
      FORMAT="$2"
      shift 2
      ;;
    --format=*)
      FORMAT="${1#*=}"
      shift
      ;;
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
    --smoke)
      RUN_SMOKE=1
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

[[ "$(resolve_host_os)" == "linux" ]] || die "scripts/build-linux.sh must run on a Linux x86_64 host."

case "$FORMAT" in
  deb) require_cmd dpkg-deb "Install dpkg (dpkg-deb) to build .deb packages." ;;
  rpm) require_cmd rpmbuild "Install rpm (rpmbuild) to build .rpm packages." ;;
  appimage) ;;
  *) die "--format must be 'deb', 'rpm', or 'appimage', got '$FORMAT'" ;;
esac

require_cmd rustup "Install Rust via rustup."
require_cmd cargo "Install Cargo."
require_cmd pkg-config "Install pkg-config and GTK/WebKit2GTK development libraries."

if ! rustup target list --installed | grep -qx 'x86_64-unknown-linux-gnu'; then
  die "Rust target 'x86_64-unknown-linux-gnu' is not installed. Run: rustup target add x86_64-unknown-linux-gnu"
fi

for pc in gtk+-3.0 webkit2gtk-4.1; do
  if ! pkg-config --exists "$pc"; then
    die "Missing Linux system development package '$pc' (checked via pkg-config)."
  fi
done

ensure_node_deps

cmd=(node "$PACKAGING_SCRIPT" build --target linux-x64 --format "$FORMAT")
[[ "$DEBUG" -eq 1 ]] && cmd+=(--debug)
[[ "$OFFLINE" -eq 1 ]] && cmd+=(--offline)

log "Building Linux x64 client (${FORMAT}, profile=$([[ "$DEBUG" -eq 1 ]] && echo debug || echo release))..."
(cd "$REPO_ROOT" && "${cmd[@]}")
print_report_summary "linux-x64"

if [[ "$RUN_SMOKE" -eq 1 ]]; then
  artifact_path="$(node -e '
    const fs = require("node:fs");
    const r = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    process.stdout.write(r.artifactPath);
  ' "${REPO_ROOT}/target/packaging/reports/linux-x64-build-report.json")"
  (cd "$REPO_ROOT" && node "$PACKAGING_SCRIPT" smoke --target linux-x64 --artifact "${REPO_ROOT}/${artifact_path}")
fi

if [[ "$WITH_SERVER" -eq 1 ]]; then
  server_args=(build -q --locked -p shiro-reader-server --target x86_64-unknown-linux-gnu)
  [[ "$DEBUG" -eq 0 ]] && server_args+=(--release)
  [[ "$OFFLINE" -eq 1 ]] && server_args+=(--offline)
  log "Building Linux x64 server binary..."
  (cd "$REPO_ROOT" && cargo "${server_args[@]}")
  profile_dir="$([[ "$DEBUG" -eq 1 ]] && echo debug || echo release)"
  log "Server binary: target/x86_64-unknown-linux-gnu/${profile_dir}/shiro-reader-server"
fi
