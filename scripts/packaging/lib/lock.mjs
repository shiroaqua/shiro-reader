import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { fail } from "./cli.mjs"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
export const PACKAGING_DIR = path.resolve(__dirname, "..")
export const REPO_ROOT = path.resolve(PACKAGING_DIR, "../..")
export const DESKTOP_DIR = path.join(REPO_ROOT, "apps/desktop")
export const UI_DIR = path.join(DESKTOP_DIR, "ui")
export const TAURI_DIR = path.join(DESKTOP_DIR, "src-tauri")
export const ASSETS_LOCK_PATH = path.join(PACKAGING_DIR, "assets.lock.json")
export const FIXTURES_DIR = path.join(PACKAGING_DIR, "fixtures")
export const REPORTS_DIR = path.join(REPO_ROOT, "target/packaging/reports")
export const TEMP_VERIFY_DIR = path.join(REPO_ROOT, "target/packaging/verify-tmp")

const SHA256_RE = /^[0-9a-f]{64}$/

export function validateLock(lock) {
  if (!lock || typeof lock !== "object" || lock.schemaVersion !== 2) {
    fail(`assets.lock.json must specify schemaVersion: 2`)
  }
  const pdfium = lock.pdfium
  if (!pdfium || typeof pdfium.upstreamReleaseTag !== "string" || !pdfium.targets) {
    fail(`assets.lock.json is missing pdfium metadata`)
  }
  for (const [targetName, spec] of Object.entries(pdfium.targets)) {
    for (const key of [
      "rustTarget",
      "abi",
      "assetName",
      "downloadUrl",
      "binaryMember",
      "licenseMember",
      "stagedBinaryRelativePath",
      "stagedLicenseRelativePath",
    ]) {
      if (typeof spec[key] !== "string" || spec[key].length === 0) {
        fail(`assets.lock.json target '${targetName}' missing string field '${key}'`)
      }
    }
    for (const key of ["archiveSizeBytes", "extractedBinarySizeBytes"]) {
      if (!Number.isInteger(spec[key]) || spec[key] <= 0) {
        fail(`assets.lock.json target '${targetName}' missing positive integer '${key}'`)
      }
    }
    for (const key of ["archiveSha256", "extractedBinarySha256", "licenseSha256"]) {
      if (typeof spec[key] !== "string" || !SHA256_RE.test(spec[key])) {
        fail(`assets.lock.json target '${targetName}' has invalid SHA-256 for '${key}'`)
      }
    }
    if (!Array.isArray(spec.requiredLicenseMembers) || spec.requiredLicenseMembers.length === 0) {
      fail(`assets.lock.json target '${targetName}' missing requiredLicenseMembers`)
    }
  }
  return lock
}

export function loadLock(lockPath = ASSETS_LOCK_PATH) {
  const raw = fs.readFileSync(lockPath, "utf8")
  return validateLock(JSON.parse(raw))
}

export function resolveTargetSpec(lock, target) {
  const spec = lock.pdfium.targets[target]
  if (!spec) {
    const valid = Object.keys(lock.pdfium.targets).join("|")
    fail(`--target must be one of ${valid}`)
  }
  return spec
}

export function resolvePdfiumCacheRoot(lock, env = process.env) {
  if (env.PDFIUM_CACHE_DIR) {
    return path.resolve(env.PDFIUM_CACHE_DIR)
  }
  const slug = lock.pdfium.upstreamReleaseTag.replace(/[^A-Za-z0-9._-]+/g, "-")
  return path.join(REPO_ROOT, ".cache/packaging/pdfium", slug)
}
