#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/build-common.sh
source "${SCRIPT_DIR}/lib/build-common.sh"

TARGETS="android,windows,linux"
ANDROID_FORMAT="apk"
LINUX_FORMAT="deb"
DEBUG=0
OFFLINE=0
WITH_SERVER=0

usage() {
  cat <<'EOF'
Usage: scripts/build-all.sh [OPTIONS]

Build ShiroReader packages for Android, Windows, and Linux.

Options:
  --targets <list>             Comma-separated subset of: android,windows,linux (default: android,windows,linux)
  --android-format <apk|aab>   Android package format (default: apk)
  --linux-format <deb|rpm|appimage>
                               Linux bundle format (default: deb)
  --debug                      Build debug variants instead of release
  --offline                    Use offline caches only
  --with-server                Also compile shiro-reader-server for Windows and Linux
  -h, --help                   Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --targets)
      [[ $# -ge 2 ]] || die "--targets requires a comma-separated list"
      TARGETS="$2"
      shift 2
      ;;
    --targets=*)
      TARGETS="${1#*=}"
      shift
      ;;
    --android-format)
      [[ $# -ge 2 ]] || die "--android-format requires a value"
      ANDROID_FORMAT="$2"
      shift 2
      ;;
    --android-format=*)
      ANDROID_FORMAT="${1#*=}"
      shift
      ;;
    --linux-format)
      [[ $# -ge 2 ]] || die "--linux-format requires a value"
      LINUX_FORMAT="$2"
      shift 2
      ;;
    --linux-format=*)
      LINUX_FORMAT="${1#*=}"
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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "Unknown option: $1 (see --help)"
      ;;
  esac
done

common_flags=()
[[ "$DEBUG" -eq 1 ]] && common_flags+=(--debug)
[[ "$OFFLINE" -eq 1 ]] && common_flags+=(--offline)

IFS=',' read -r -a requested <<< "$TARGETS"
built_targets=()

for raw_target in "${requested[@]}"; do
  target="$(printf '%s' "$raw_target" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  [[ -n "$target" ]] || continue
  case "$target" in
    android)
      "${SCRIPT_DIR}/build-android.sh" --format "$ANDROID_FORMAT" "${common_flags[@]}"
      built_targets+=("android-arm64")
      ;;
    windows)
      win_flags=("${common_flags[@]}")
      [[ "$WITH_SERVER" -eq 1 ]] && win_flags+=(--with-server)
      "${SCRIPT_DIR}/build-windows.sh" "${win_flags[@]}"
      built_targets+=("windows-x64")
      ;;
    linux)
      linux_flags=(--format "$LINUX_FORMAT" "${common_flags[@]}")
      [[ "$WITH_SERVER" -eq 1 ]] && linux_flags+=(--with-server)
      "${SCRIPT_DIR}/build-linux.sh" "${linux_flags[@]}"
      built_targets+=("linux-x64")
      ;;
    *)
      die "Unknown target '$target' (expected android, windows, or linux)"
      ;;
  esac
done

[[ "${#built_targets[@]}" -gt 0 ]] || die "No targets selected."

log "=========================================="
log "All requested builds completed:"
for report_target in "${built_targets[@]}"; do
  print_report_summary "$report_target"
done
