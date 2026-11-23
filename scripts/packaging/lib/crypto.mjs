import crypto from "node:crypto"
import fs from "node:fs"
import { fail } from "./cli.mjs"

export function sha256File(filePath) {
  const hash = crypto.createHash("sha256")
  const fd = fs.openSync(filePath, "r")
  try {
    const buf = Buffer.allocUnsafe(64 * 1024)
    for (;;) {
      const n = fs.readSync(fd, buf, 0, buf.length, null)
      if (n === 0) break
      hash.update(buf.subarray(0, n))
    }
  } finally {
    fs.closeSync(fd)
  }
  return hash.digest("hex")
}

export function assertExactFileDigest(filePath, expectedSize, expectedSha256, label) {
  if (!fs.existsSync(filePath)) {
    fail(`${label} missing: ${filePath}`)
  }
  const actualSize = fs.statSync(filePath).size
  if (typeof expectedSize === "number" && actualSize !== expectedSize) {
    fail(`${label} size mismatch (${filePath}): expected ${expectedSize} bytes, got ${actualSize} bytes`)
  }
  const actualSha = sha256File(filePath)
  if (actualSha !== expectedSha256) {
    fail(`${label} SHA-256 mismatch (${filePath}): expected ${expectedSha256}, got ${actualSha}`)
  }
  return { sizeBytes: actualSize, sha256: actualSha }
}
