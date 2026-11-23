#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/build-common.sh
source "${SCRIPT_DIR}/lib/build-common.sh"

FORMAT="apk"
DEBUG=0
OFFLINE=0
RUN_SMOKE=0
DEVICE_ID=""

usage() {
  cat <<'EOF'
Usage: scripts/build-android.sh [OPTIONS]

Build and verify the ShiroReader Android (arm64-v8a) client package.

Options:
  --format <apk|aab>   Output package format (default: apk)
  --debug              Build debug variant instead of release
  --offline            Use offline Cargo/Gradle/PDFium caches only
  --smoke              Run adb smoke install/launch check after building (apk only)
  --device <id>        Target adb device serial for --smoke
  -h, --help           Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --format)
      [[ $# -ge 2 ]] || die "--format requires a value (apk|aab)"
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
    --smoke)
      RUN_SMOKE=1
      shift
      ;;
    --device)
      [[ $# -ge 2 ]] || die "--device requires a value"
      DEVICE_ID="$2"
      shift 2
      ;;
    --device=*)
      DEVICE_ID="${1#*=}"
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

case "$FORMAT" in
  apk|aab) ;;
  *) die "--format must be 'apk' or 'aab', got '$FORMAT'" ;;
esac

if [[ "$RUN_SMOKE" -eq 1 && "$FORMAT" != "apk" ]]; then
  die "--smoke only supports --format apk"
fi

require_cmd rustup "Install Rust via rustup."
require_cmd cargo "Install Cargo."
require_cmd java "Install JDK 17+ and set JAVA_HOME."

if ! rustup target list --installed | grep -qx 'aarch64-linux-android'; then
  die "Rust target 'aarch64-linux-android' is not installed. Run: rustup target add aarch64-linux-android"
fi

ANDROID_SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
[[ -n "$ANDROID_SDK" && -d "$ANDROID_SDK" ]] || die "ANDROID_HOME or ANDROID_SDK_ROOT must point to an Android SDK directory."
export ANDROID_HOME="$ANDROID_SDK"
export ANDROID_SDK_ROOT="$ANDROID_SDK"

NDK_DIR="${ANDROID_NDK_HOME:-${NDK_HOME:-}}"
if [[ -z "$NDK_DIR" || ! -d "$NDK_DIR" ]]; then
  if [[ -d "${ANDROID_SDK}/ndk" ]]; then
    latest_ndk="$(find "${ANDROID_SDK}/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1 || true)"
    [[ -n "$latest_ndk" ]] && NDK_DIR="$latest_ndk"
  fi
fi
[[ -n "$NDK_DIR" && -d "$NDK_DIR" ]] || die "ANDROID_NDK_HOME / NDK_HOME must point to an installed Android NDK."
export ANDROID_NDK_HOME="$NDK_DIR"
export NDK_HOME="$NDK_DIR"

ensure_node_deps
configure_gradle_proxy

cmd=(node "$PACKAGING_SCRIPT" build --target android-arm64 --format "$FORMAT")
[[ "$DEBUG" -eq 1 ]] && cmd+=(--debug)
[[ "$OFFLINE" -eq 1 ]] && cmd+=(--offline)

log "Building Android (${FORMAT}, profile=$([[ "$DEBUG" -eq 1 ]] && echo debug || echo release))..."
(cd "$REPO_ROOT" && "${cmd[@]}")
print_report_summary "android-arm64"

if [[ "$RUN_SMOKE" -eq 1 ]]; then
  artifact_path="$(node -e '
    const fs = require("node:fs");
    const r = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    process.stdout.write(r.artifactPath);
  ' "${REPO_ROOT}/target/packaging/reports/android-arm64-build-report.json")"
  smoke_cmd=(node "$PACKAGING_SCRIPT" smoke --target android-arm64 --artifact "${REPO_ROOT}/${artifact_path}")
  [[ -n "$DEVICE_ID" ]] && smoke_cmd+=(--device "$DEVICE_ID")
  (cd "$REPO_ROOT" && "${smoke_cmd[@]}")
fi
