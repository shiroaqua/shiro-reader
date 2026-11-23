#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/build-common.sh
source "${SCRIPT_DIR}/lib/build-common.sh"

DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: scripts/clean.sh [OPTIONS]

Clean all build artifacts, packaging staging files, caches, and node_modules
using native toolchain commands (cargo, tsc, gradlew) where available.

Options:
  -n, --dry-run   Print commands and paths that would be executed/removed
  -h, --help      Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--dry-run)
      DRY_RUN=1
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

run_native_step() {
  local label="$1"
  local workdir="$2"
  shift 2

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] (${label}) (cd ${workdir#"$REPO_ROOT"/} && $*)"
    return 0
  fi

  log "${label}: $*"
  if (cd "$workdir" && "$@"); then
    return 0
  else
    log "Warning: native command failed (${label}: $*), falling back to directory cleanup."
    return 1
  fi
}

remove_rel_path() {
  local rel="$1"
  [[ -n "$rel" && "$rel" != "/" && "$rel" != "." && "$rel" != ".." ]] || die "Unsafe clean path: '$rel'"

  local abs="${REPO_ROOT}/${rel}"
  if [[ ! -e "$abs" && ! -L "$abs" ]]; then
    return 1
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] Would remove ${rel}"
  else
    log "Removing ${rel}..."
    rm -rf -- "$abs"
  fi
  return 0
}

# 1. Rust workspace (native: cargo clean)
if command -v cargo >/dev/null 2>&1; then
  if [[ "$DRY_RUN" -eq 1 ]]; then
    run_native_step "Cargo workspace clean" "$REPO_ROOT" cargo clean --dry-run
    [[ -f "${REPO_ROOT}/apps/desktop/src-tauri/Cargo.toml" && -d "${REPO_ROOT}/apps/desktop/src-tauri/target" ]] && \
      run_native_step "Tauri crate clean" "${REPO_ROOT}/apps/desktop/src-tauri" cargo clean --dry-run
  else
    run_native_step "Cargo workspace clean" "$REPO_ROOT" cargo clean || true
    [[ -f "${REPO_ROOT}/apps/desktop/src-tauri/Cargo.toml" && -d "${REPO_ROOT}/apps/desktop/src-tauri/target" ]] && \
      run_native_step "Tauri crate clean" "${REPO_ROOT}/apps/desktop/src-tauri" cargo clean || true
  fi
fi

# 2. Frontend TypeScript incremental build cache (native: tsc -b --clean)
UI_DIR="${REPO_ROOT}/apps/desktop/ui"
if [[ -x "${UI_DIR}/node_modules/.bin/tsc" ]]; then
  run_native_step "TypeScript build clean" "$UI_DIR" ./node_modules/.bin/tsc -b --clean || true
elif command -v pnpm >/dev/null 2>&1 && [[ -d "${UI_DIR}/node_modules/typescript" ]]; then
  run_native_step "TypeScript build clean" "$UI_DIR" pnpm exec tsc -b --clean || true
fi

# 3. Android Gradle project (native: ./gradlew clean --stop)
ANDROID_DIR="${REPO_ROOT}/apps/desktop/src-tauri/gen/android"
ANDROID_SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [[ -x "${ANDROID_DIR}/gradlew" ]] && command -v java >/dev/null 2>&1 && [[ -n "$ANDROID_SDK" && -d "$ANDROID_SDK" ]]; then
  export ANDROID_HOME="$ANDROID_SDK"
  export ANDROID_SDK_ROOT="$ANDROID_SDK"
  export GRADLE_USER_HOME="${GRADLE_USER_HOME:-${REPO_ROOT}/.cache/gradle}"
  run_native_step "Android Gradle clean" "$ANDROID_DIR" ./gradlew clean --quiet || true
  run_native_step "Stop Gradle daemon" "$ANDROID_DIR" ./gradlew --stop --quiet || true
fi

# 4. Clean remaining packaging reports, Tauri staging, frontend dist, caches, and node_modules
CLEAN_PATHS=(
  # Remaining target directories (e.g. target/packaging/ reports or if cargo was unavailable)
  "target"
  "apps/desktop/src-tauri/target"

  # Tauri staging & generated schemas
  "apps/desktop/src-tauri/staging"
  "apps/desktop/src-tauri/gen/schemas"

  # Android Gradle caches & native outputs not removed by `./gradlew clean`
  "apps/desktop/src-tauri/gen/android/.gradle"
  "apps/desktop/src-tauri/gen/android/.kotlin"
  "apps/desktop/src-tauri/gen/android/.tauri"
  "apps/desktop/src-tauri/gen/android/build"
  "apps/desktop/src-tauri/gen/android/app/build"
  "apps/desktop/src-tauri/gen/android/app/.cxx"
  "apps/desktop/src-tauri/gen/android/app/.externalNativeBuild"
  "apps/desktop/src-tauri/gen/android/app/src/main/jniLibs"
  "apps/desktop/src-tauri/gen/android/app/src/main/assets/native"

  # Frontend & packaging Node outputs / dependencies
  "apps/desktop/ui/dist"
  "apps/desktop/ui/dist-ssr"
  "apps/desktop/ui/.eslintcache"
  "apps/desktop/ui/tsconfig.tsbuildinfo"
  "apps/desktop/ui/tsconfig.app.tsbuildinfo"
  "apps/desktop/ui/tsconfig.node.tsbuildinfo"
  "apps/desktop/ui/tsconfig.test.tsbuildinfo"
  "apps/desktop/ui/node_modules"
  "scripts/packaging/node_modules"
  "node_modules"

  # Build & packaging caches (PDFium, Gradle, xwin, keystore)
  ".cache"

  # Coverage reports
  "coverage"
  "tarpaulin-report.html"
)

removed_count=0
for rel_path in "${CLEAN_PATHS[@]}"; do
  if remove_rel_path "$rel_path"; then
    removed_count=$((removed_count + 1))
  fi
done

if [[ "$DRY_RUN" -eq 1 ]]; then
  log "Dry run complete (${removed_count} additional path(s) matched)."
else
  log "Clean complete."
fi
