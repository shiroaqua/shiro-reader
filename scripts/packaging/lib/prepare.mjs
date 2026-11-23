import fs from "node:fs"
import path from "node:path"
import process from "node:process"
import * as tar from "tar"
import { fail, log } from "./cli.mjs"
import { assertExactFileDigest } from "./crypto.mjs"
import { commandExists, runCmd } from "./env.mjs"
import { inspectElfAarch64, inspectElfX86_64, inspectPe64 } from "./binary-inspect.mjs"
import { loadLock, REPO_ROOT, resolvePdfiumCacheRoot, resolveTargetSpec, TAURI_DIR } from "./lock.mjs"

export function assertSafeEntryPath(entryPath) {
  if (!entryPath || entryPath.includes("\0")) {
    fail(`unsafe archive entry name: ${JSON.stringify(entryPath)}`)
  }
  if (entryPath.startsWith("/") || entryPath.startsWith("\\") || /^[A-Za-z]:/.test(entryPath)) {
    fail(`archive entry uses absolute path: ${entryPath}`)
  }
  const parts = entryPath.split(/[\\/]+/)
  if (parts.some((p) => p === "..")) {
    fail(`archive entry contains path traversal: ${entryPath}`)
  }
}

async function downloadArchive(url, destPath) {
  log(`Downloading ${url}`)
  fs.mkdirSync(path.dirname(destPath), { recursive: true })
  const tmpPath = `${destPath}.tmp-${process.pid}`
  const hasProxyEnv = Boolean(
    process.env.HTTPS_PROXY ||
      process.env.https_proxy ||
      process.env.HTTP_PROXY ||
      process.env.http_proxy ||
      process.env.ALL_PROXY ||
      process.env.all_proxy
  )
  try {
    if (hasProxyEnv && commandExists("curl")) {
      runCmd("curl", ["-fsSL", "-o", tmpPath, url])
    } else {
      const response = await fetch(url)
      if (!response.ok) {
        fail(`failed to download ${url}: HTTP ${response.status} ${response.statusText}`)
      }
      const data = Buffer.from(await response.arrayBuffer())
      fs.writeFileSync(tmpPath, data)
    }
    fs.renameSync(tmpPath, destPath)
  } catch (err) {
    fs.rmSync(tmpPath, { force: true })
    throw err
  }
}

function copyDirectoryTreeSafe(srcDir, destDir) {
  fs.mkdirSync(destDir, { recursive: true })
  for (const dirent of fs.readdirSync(srcDir, { withFileTypes: true })) {
    const srcPath = path.join(srcDir, dirent.name)
    const destPath = path.join(destDir, dirent.name)
    if (dirent.isSymbolicLink()) {
      fail(`refusing to copy symbolic link in licenses tree: ${srcPath}`)
    } else if (dirent.isDirectory()) {
      copyDirectoryTreeSafe(srcPath, destPath)
    } else if (dirent.isFile()) {
      fs.copyFileSync(srcPath, destPath)
    }
  }
}

function swapDirectoryAtomic(nextDir, targetDir) {
  const backupDir = `${targetDir}.old-${process.pid}`
  fs.rmSync(backupDir, { recursive: true, force: true })
  const hadOld = fs.existsSync(targetDir)
  if (hadOld) {
    fs.renameSync(targetDir, backupDir)
  }
  try {
    fs.renameSync(nextDir, targetDir)
  } catch (err) {
    if (hadOld && fs.existsSync(backupDir) && !fs.existsSync(targetDir)) {
      fs.renameSync(backupDir, targetDir)
    }
    throw err
  } finally {
    fs.rmSync(backupDir, { recursive: true, force: true })
  }
}

export async function inspectArchiveSafety(archivePath) {
  const entries = []
  await tar.t({
    file: archivePath,
    onReadEntry(entry) {
      assertSafeEntryPath(entry.path)
      if (entry.type !== "File" && entry.type !== "Directory") {
        fail(`disallowed tar entry type '${entry.type}' for ${entry.path}`)
      }
      entries.push(entry.path)
    },
  })
  return entries
}

export async function cmdPrepare(flags) {
  const target = flags.target
  const offline = Boolean(flags.offline)
  const lock = loadLock()
  const spec = resolveTargetSpec(lock, target)
  const cacheRoot = resolvePdfiumCacheRoot(lock)
  const archivePath = path.join(cacheRoot, spec.assetName)

  if (!fs.existsSync(archivePath)) {
    if (offline) {
      fail(`offline mode enabled and cached archive is missing: ${archivePath}`)
    }
    await downloadArchive(spec.downloadUrl, archivePath)
  }

  const archiveDigest = assertExactFileDigest(
    archivePath,
    spec.archiveSizeBytes,
    spec.archiveSha256,
    `PDFium archive ${spec.assetName}`
  )
  log(`Verified archive ${spec.assetName} (${archiveDigest.sizeBytes} bytes, sha256=${archiveDigest.sha256})`)

  const entries = await inspectArchiveSafety(archivePath)
  const entrySet = new Set(entries)

  if (!entrySet.has(spec.binaryMember)) {
    fail(`expected binary member '${spec.binaryMember}' missing from ${spec.assetName}`)
  }
  for (const requiredLic of spec.requiredLicenseMembers) {
    if (!entrySet.has(requiredLic)) {
      fail(`expected license member '${requiredLic}' missing from ${spec.assetName}`)
    }
  }

  const stagingRoot = path.join(TAURI_DIR, "staging")
  const extractTmp = path.join(stagingRoot, `.extract-${target}-${process.pid}`)
  const nextStagingDir = path.join(stagingRoot, `.next-${target}-${process.pid}`)
  fs.rmSync(extractTmp, { recursive: true, force: true })
  fs.rmSync(nextStagingDir, { recursive: true, force: true })
  fs.mkdirSync(extractTmp, { recursive: true })
  fs.mkdirSync(nextStagingDir, { recursive: true })

  try {
    const wantedLicenses = new Set(
      entries.filter((e) => e === spec.licenseMember || (e.startsWith("licenses/") && !e.endsWith("/")))
    )
    await tar.x({
      file: archivePath,
      cwd: extractTmp,
      preservePaths: false,
      strict: true,
      filter(entryPath) {
        return entryPath === spec.binaryMember || wantedLicenses.has(entryPath)
      },
    })

    const extractedBin = path.join(extractTmp, spec.binaryMember)
    const extractedLicense = path.join(extractTmp, spec.licenseMember)

    const binDigest = assertExactFileDigest(
      extractedBin,
      spec.extractedBinarySizeBytes,
      spec.extractedBinarySha256,
      `extracted PDFium binary (${spec.binaryMember})`
    )
    const licDigest = assertExactFileDigest(
      extractedLicense,
      undefined,
      spec.licenseSha256,
      `extracted PDFium license (${spec.licenseMember})`
    )

    let binaryInfo
    if (target === "windows-x64") {
      binaryInfo = inspectPe64(extractedBin)
    } else if (target === "linux-x64") {
      binaryInfo = inspectElfX86_64(extractedBin, {
        allowedDtNeeded: spec.allowedDtNeeded,
        allowedTypes: [3],
        requireReadelf: true,
      })
    } else {
      binaryInfo = inspectElfAarch64(extractedBin, {
        allowedDtNeeded: spec.allowedDtNeeded,
        requireReadelf: true,
      })
    }

    const destBin = path.join(nextStagingDir, spec.stagedBinaryRelativePath)
    const destLic = path.join(nextStagingDir, spec.stagedLicenseRelativePath)
    fs.mkdirSync(path.dirname(destBin), { recursive: true })
    fs.mkdirSync(path.dirname(destLic), { recursive: true })
    fs.copyFileSync(extractedBin, destBin)
    fs.copyFileSync(extractedLicense, destLic)

    const licensesSrcDir = path.join(extractTmp, "licenses")
    if (fs.existsSync(licensesSrcDir)) {
      copyDirectoryTreeSafe(licensesSrcDir, path.join(path.dirname(destLic), "licenses"))
    }

    const finalStagingDir = path.join(stagingRoot, target)
    const manifest = {
      schemaVersion: lock.schemaVersion,
      target,
      abi: spec.abi,
      rustTarget: spec.rustTarget,
      pdfiumTag: lock.pdfium.upstreamReleaseTag,
      wrapperCrate: `${lock.pdfium.wrapperCrate}@${lock.pdfium.wrapperVersion}`,
      wrapperFeature: lock.pdfium.wrapperFeature,
      archiveName: spec.assetName,
      archiveSizeBytes: archiveDigest.sizeBytes,
      archiveSha256: archiveDigest.sha256,
      stagedBinaryPath: path.relative(REPO_ROOT, path.join(finalStagingDir, spec.stagedBinaryRelativePath)),
      extractedBinarySizeBytes: binDigest.sizeBytes,
      extractedBinarySha256: binDigest.sha256,
      binaryInspection: binaryInfo,
      stagedLicensePath: path.relative(REPO_ROOT, path.join(finalStagingDir, spec.stagedLicenseRelativePath)),
      licenseSha256: licDigest.sha256,
      preparedAt: new Date().toISOString(),
    }

    fs.writeFileSync(
      path.join(nextStagingDir, "manifest.json"),
      JSON.stringify(manifest, null, 2) + "\n"
    )

    swapDirectoryAtomic(nextStagingDir, finalStagingDir)
    log(`Prepared ${target} staging -> ${path.relative(REPO_ROOT, path.join(finalStagingDir, "manifest.json"))}`)
    return manifest
  } finally {
    fs.rmSync(extractTmp, { recursive: true, force: true })
    fs.rmSync(nextStagingDir, { recursive: true, force: true })
  }
}
