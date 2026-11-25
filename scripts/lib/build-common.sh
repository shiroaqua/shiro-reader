#!/usr/bin/env bash

# Shared helper functions for ShiroReader platform build scripts.

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  echo "This file is a helper library and should be sourced, not executed directly." >&2
  exit 1
fi

SCRIPT_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_LIB_DIR}/../.." && pwd)"
PACKAGING_SCRIPT="${REPO_ROOT}/scripts/packaging/package.mjs"

log() {
  printf '[build] %s\n' "$*"
}

die() {
  printf '[build] ERROR: %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  local cmd="$1"
  local hint="${2:-}"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    if [[ -n "$hint" ]]; then
      die "Missing required command '$cmd'. $hint"
    fi
    die "Missing required command '$cmd'."
  fi
}

ensure_node_deps() {
  require_cmd node "Install Node.js >= 22."
  require_cmd pnpm "Install pnpm."

  if [[ ! -d "${REPO_ROOT}/apps/client/ui/node_modules/@tauri-apps/cli" ]]; then
    log "Installing client UI dependencies..."
    pnpm --dir "${REPO_ROOT}/apps/client/ui" install --frozen-lockfile
  fi

  if [[ ! -d "${REPO_ROOT}/scripts/packaging/node_modules/tar" ]]; then
    log "Installing packaging script dependencies..."
    pnpm --dir "${REPO_ROOT}/scripts/packaging" install --frozen-lockfile
  fi
}

configure_gradle_proxy() {
  local derived
  derived="$(node -e '
    const existing = (process.env.GRADLE_OPTS || "").trim();
    const extras = [];
    const parseProxy = (raw) => {
      try {
        const u = new URL(raw.includes("://") ? raw : `http://${raw}`);
        if (!u.hostname) return null;
        return { host: u.hostname, port: u.port || (u.protocol === "https:" ? "443" : "80") };
      } catch {
        return null;
      }
    };
    const httpProxy = process.env.HTTP_PROXY || process.env.http_proxy || "";
    const httpsProxy = process.env.HTTPS_PROXY || process.env.https_proxy || httpProxy;
    const noProxy = process.env.NO_PROXY || process.env.no_proxy || "";

    if (httpProxy && !existing.includes("-Dhttp.proxyHost=")) {
      const p = parseProxy(httpProxy);
      if (p) extras.push(`-Dhttp.proxyHost=${p.host}`, `-Dhttp.proxyPort=${p.port}`);
    }
    if (httpsProxy && !existing.includes("-Dhttps.proxyHost=")) {
      const p = parseProxy(httpsProxy);
      if (p) extras.push(`-Dhttps.proxyHost=${p.host}`, `-Dhttps.proxyPort=${p.port}`);
    }
    if (noProxy && !existing.includes("-Dhttp.nonProxyHosts=")) {
      const hosts = noProxy
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s && !s.includes("/"))
        .map((s) => (s.startsWith(".") ? `*${s}` : s))
        .join("|");
      if (hosts) extras.push(`-Dhttp.nonProxyHosts=${hosts}`);
    }
    if ((httpProxy || httpsProxy || existing.includes("proxyHost")) && !existing.includes("-Djdk.tls.client.protocols=")) {
      extras.push("-Djdk.tls.client.protocols=TLSv1.2");
    }
    process.stdout.write([existing, ...extras].filter(Boolean).join(" "));
  ')"

  if [[ -n "$derived" ]]; then
    export GRADLE_OPTS="$derived"
  fi
}

resolve_host_os() {
  case "$(uname -s)" in
    Linux*) printf 'linux' ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT*) printf 'windows' ;;
    Darwin*) printf 'macos' ;;
    *) uname -s | tr '[:upper:]' '[:lower:]' ;;
  esac
}

print_report_summary() {
  local target="$1"
  local report_json="${REPO_ROOT}/target/packaging/reports/${target}-build-report.json"
  if [[ -f "$report_json" ]]; then
    node -e '
      const fs = require("node:fs");
      const r = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
      console.log(`[build] Artifact (${r.target}): ${r.artifactPath}`);
      console.log(`[build]   Size   : ${r.artifactSizeBytes} bytes`);
      console.log(`[build]   SHA256 : ${r.artifactSha256}`);
      console.log(`[build]   Signing: ${r.signing}`);
      console.log(`[build]   Verify : ${r.verification && r.verification.passed ? "PASSED" : "FAILED"}`);
      console.log(`[build]   Report : target/packaging/reports/${r.target}-build-report.md`);
    ' "$report_json"
  fi
}
