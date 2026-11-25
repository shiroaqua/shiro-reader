import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import process from "node:process"
import { fail, log } from "./cli.mjs"
import { assertExactFileDigest, sha256File } from "./crypto.mjs"
import {
  inspectElfAarch64,
  inspectElfX86_64,
  inspectPe64,
  inspectPeHeader,
} from "./binary-inspect.mjs"
import {
  captureCmd,
  commandExists,
  find7z,
  findAndroidBuildTool,
  findBundletool,
  runCmd,
} from "./env.mjs"
import { loadLock, REPO_ROOT, resolveTargetSpec, TAURI_DIR, TEMP_VERIFY_DIR } from "./lock.mjs"
import { openZip } from "./zip.mjs"

const RUST_ANDROID_ALLOWED_DT_NEEDED = [
  "libc.so",
  "libm.so",
  "libdl.so",
  "liblog.so",
  "libandroid.so",
  "libz.so",
]

export function resolveWindowsBuildProfileDir(artifactPath, { debug } = {}) {
  let profile = typeof debug === "boolean" ? (debug ? "debug" : "release") : null
  if (!profile && artifactPath) {
    const segments = path.resolve(artifactPath).split(path.sep)
    const targetIdx = segments.lastIndexOf("x86_64-pc-windows-msvc")
    if (targetIdx !== -1 && targetIdx + 1 < segments.length) {
      const candidate = segments[targetIdx + 1]
      if (candidate === "debug" || candidate === "release") {
        profile = candidate
      }
    }
  }
  return path.join(REPO_ROOT, "target/x86_64-pc-windows-msvc", profile ?? "release")
}

function verifyWindowsArtifact(artifactPath, spec, options = {}) {
  const artifactSha = sha256File(artifactPath)
  const artifactSize = fs.statSync(artifactPath).size
  const installerPe = inspectPeHeader(artifactPath, { require64Bit: false })

  const sevenZip = find7z()
  if (sevenZip) {
    const tmpDir = path.join(TEMP_VERIFY_DIR, `win-verify-${process.pid}`)
    fs.rmSync(tmpDir, { recursive: true, force: true })
    fs.mkdirSync(tmpDir, { recursive: true })
    try {
      runCmd(sevenZip, ["x", "-y", `-o${tmpDir}`, artifactPath])
      const pdfiumPath = path.join(tmpDir, "native/pdfium.dll")
      const licensePath = path.join(tmpDir, "native/PDFIUM-LICENSE.txt")
      const pdfiumLicPath = path.join(tmpDir, "native/licenses/pdfium.txt")
      const exePath = findSingleExe(tmpDir)

      assertExactFileDigest(
        pdfiumPath,
        spec.extractedBinarySizeBytes,
        spec.extractedBinarySha256,
        "installer-embedded native/pdfium.dll"
      )
      assertExactFileDigest(
        licensePath,
        undefined,
        spec.licenseSha256,
        "installer-embedded native/PDFIUM-LICENSE.txt"
      )
      if (!fs.existsSync(pdfiumLicPath)) {
        fail("installer missing native/licenses/pdfium.txt")
      }
      const pdfiumPe = inspectPe64(pdfiumPath)
      const appPe = inspectPe64(exePath)

      const result = {
        passed: true,
        target: "windows-x64",
        verificationMode: "nsis-extracted",
        artifactPath: path.relative(REPO_ROOT, artifactPath),
        artifactSizeBytes: artifactSize,
        artifactSha256: artifactSha,
        installerPe,
        appExecutablePe: appPe,
        bundledPdfiumPe: pdfiumPe,
        bundledPdfiumSha256: spec.extractedBinarySha256,
        bundledLicenseSha256: spec.licenseSha256,
      }
      log(`Verified Windows installer archive: ${result.artifactPath} (sha256=${artifactSha})`)
      return result
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true })
    }
  }

  // Fallback when 7z is unavailable: strictly require the matching build tree NSIS script and binaries.
  const buildProfileDir = resolveWindowsBuildProfileDir(artifactPath, options)
  const buildExe = path.join(buildProfileDir, "shiro-reader-desktop.exe")
  const nsisScript = path.join(buildProfileDir, "nsis/x64/installer.nsi")
  const stagedBin = path.join(TAURI_DIR, "staging/windows-x64", spec.stagedBinaryRelativePath)
  const stagedLic = path.join(TAURI_DIR, "staging/windows-x64", spec.stagedLicenseRelativePath)

  if (!fs.existsSync(nsisScript)) {
    fail(
      `7z is not installed and ${nsisScript} is missing; cannot verify Windows installer contents`
    )
  }
  const nsiContent = fs.readFileSync(nsisScript, "utf8")
  for (const needle of [
    "native\\pdfium.dll",
    "native\\PDFIUM-LICENSE.txt",
    "native\\licenses\\pdfium.txt",
  ]) {
    if (!nsiContent.includes(needle)) {
      fail(`installer.nsi missing required resource entry '${needle}'`)
    }
  }

  const appPe = inspectPe64(buildExe)
  assertExactFileDigest(
    stagedBin,
    spec.extractedBinarySizeBytes,
    spec.extractedBinarySha256,
    "staged Windows pdfium.dll"
  )
  assertExactFileDigest(
    stagedLic,
    undefined,
    spec.licenseSha256,
    "staged Windows PDFIUM-LICENSE.txt"
  )
  const pdfiumPe = inspectPe64(stagedBin)

  const result = {
    passed: true,
    target: "windows-x64",
    verificationMode: "nsis-script-and-staging",
    artifactPath: path.relative(REPO_ROOT, artifactPath),
    artifactSizeBytes: artifactSize,
    artifactSha256: artifactSha,
    installerPe,
    appExecutablePe: appPe,
    bundledPdfiumPe: pdfiumPe,
    bundledPdfiumSha256: spec.extractedBinarySha256,
    bundledLicenseSha256: spec.licenseSha256,
    nsisResourcesVerified: true,
  }
  log(`Verified Windows artifact: ${result.artifactPath} (sha256=${artifactSha})`)
  return result
}

function findSingleExe(rootDir) {
  const direct = path.join(rootDir, "shiro-reader-desktop.exe")
  if (fs.existsSync(direct)) return direct
  for (const name of fs.readdirSync(rootDir)) {
    if (name.endsWith(".exe") && !name.toLowerCase().includes("uninstall")) {
      return path.join(rootDir, name)
    }
  }
  fail(`could not locate extracted desktop .exe in ${rootDir}`)
}

function verifyApkContents(apkPath, spec, { requireSignature = true } = {}) {
  const zip = openZip(apkPath)
  const pdfiumMember = "lib/arm64-v8a/libpdfium.so"
  const rustMember = "lib/arm64-v8a/libshiro_reader_desktop_lib.so"
  const licenseMember = "assets/native/PDFIUM-LICENSE.txt"
  const pdfiumLicenseMember = "assets/native/licenses/pdfium.txt"

  for (const required of [pdfiumMember, rustMember, licenseMember, pdfiumLicenseMember]) {
    if (!zip.byName.has(required)) {
      fail(`${apkPath} missing required entry '${required}'`)
    }
  }

  for (const nativeLib of [pdfiumMember, rustMember]) {
    const entry = zip.byName.get(nativeLib)
    if (entry.compressMethod !== 0) {
      fail(
        `${apkPath} native library '${nativeLib}' must be uncompressed (STORED=0) for 16 KiB page alignment, got method=${entry.compressMethod}`
      )
    }
  }

  const tmpDir = path.join(TEMP_VERIFY_DIR, `apk-verify-${process.pid}-${Date.now()}`)
  fs.rmSync(tmpDir, { recursive: true, force: true })
  fs.mkdirSync(tmpDir, { recursive: true })
  try {
    const outPdfium = path.join(tmpDir, "libpdfium.so")
    const outRust = path.join(tmpDir, "libshiro_reader_desktop_lib.so")
    const outLicense = path.join(tmpDir, "PDFIUM-LICENSE.txt")
    zip.extractMember(pdfiumMember, outPdfium)
    zip.extractMember(rustMember, outRust)
    zip.extractMember(licenseMember, outLicense)

    assertExactFileDigest(
      outPdfium,
      spec.extractedBinarySizeBytes,
      spec.extractedBinarySha256,
      "packaged lib/arm64-v8a/libpdfium.so"
    )
    assertExactFileDigest(
      outLicense,
      undefined,
      spec.licenseSha256,
      "packaged assets/native/PDFIUM-LICENSE.txt"
    )

    const pdfiumElf = inspectElfAarch64(outPdfium, {
      allowedDtNeeded: spec.allowedDtNeeded,
      requireReadelf: true,
    })
    const rustElf = inspectElfAarch64(outRust, {
      allowedDtNeeded: RUST_ANDROID_ALLOWED_DT_NEEDED,
      requireReadelf: true,
    })

    const zipalign = findAndroidBuildTool("zipalign")
    if (!zipalign) fail("zipalign not found in ANDROID_HOME/build-tools or PATH")
    runCmd(zipalign, ["-c", "-P", "16", "-v", "4", apkPath])

    let signatureInfo = null
    if (requireSignature) {
      const apksigner = findAndroidBuildTool("apksigner")
      if (!apksigner) fail("apksigner not found in ANDROID_HOME/build-tools or PATH")
      const sigRes = captureCmd(apksigner, ["verify", "--print-certs", apkPath])
      if (sigRes.status !== 0) {
        fail(`apksigner verification failed: ${sigRes.stderr || sigRes.stdout}`)
      }
      signatureInfo = sigRes.stdout.trim()
    }

    return {
      bundledPdfiumSha256: spec.extractedBinarySha256,
      bundledLicenseSha256: spec.licenseSha256,
      pdfiumElf,
      rustElf,
      nativeLibsStored: true,
      zipalign16KiB: true,
      apksignerVerified: Boolean(signatureInfo),
      signatureSummary: signatureInfo,
      licenseEntriesPresent: [licenseMember, pdfiumLicenseMember],
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }
}

function verifyAabArtifact(aabPath, spec) {
  const zip = openZip(aabPath)
  for (const required of [
    "base/lib/arm64-v8a/libpdfium.so",
    "base/lib/arm64-v8a/libshiro_reader_desktop_lib.so",
    "base/assets/native/PDFIUM-LICENSE.txt",
    "base/assets/native/licenses/pdfium.txt",
  ]) {
    if (!zip.byName.has(required)) {
      fail(`${aabPath} missing required AAB entry '${required}'`)
    }
  }

  const bundletool = findBundletool()
  if (!bundletool) {
    fail("bundletool is required to build and verify an arm64 APK set from .aab")
  }

  const tmpDir = path.join(TEMP_VERIFY_DIR, `aab-verify-${process.pid}-${Date.now()}`)
  fs.rmSync(tmpDir, { recursive: true, force: true })
  fs.mkdirSync(tmpDir, { recursive: true })
  try {
    const apksPath = path.join(tmpDir, "bundle.apks")
    const env = { ...process.env }
    runCmd(
      bundletool.cmd,
      [...bundletool.prefixArgs, "build-apks", `--bundle=${aabPath}`, `--output=${apksPath}`, "--mode=universal"],
      { env }
    )
    const apksZip = openZip(apksPath)
    const universalApk = path.join(tmpDir, "universal.apk")
    apksZip.extractMember("universal.apk", universalApk)
    const apkVerification = verifyApkContents(universalApk, spec, { requireSignature: false })
    return {
      format: "aab",
      bundletoolVerified: true,
      ...apkVerification,
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }
}

function findLinuxAppExecutable(binDir) {
  const preferred = path.join(binDir, "shiro-reader-desktop")
  if (fs.existsSync(preferred)) return preferred
  if (fs.existsSync(binDir)) {
    const first = fs.readdirSync(binDir).find((name) => !name.startsWith("."))
    if (first) return path.join(binDir, first)
  }
  fail(`could not locate packaged Linux executable inside ${binDir}`)
}

function extractLinuxBundle(artifactPath, tmpDir) {
  const lower = artifactPath.toLowerCase()
  if (lower.endsWith(".deb")) {
    if (!commandExists("dpkg-deb")) {
      fail("dpkg-deb is required to verify .deb packages")
    }
    runCmd("dpkg-deb", ["-x", artifactPath, tmpDir])
    const meta = captureCmd("dpkg-deb", [
      "-f",
      artifactPath,
      "Package",
      "Version",
      "Architecture",
      "Depends",
    ])
    return {
      format: "deb",
      extractedRoot: tmpDir,
      packageMetadata: meta.status === 0 ? meta.stdout.trim() : null,
    }
  }

  if (lower.endsWith(".rpm")) {
    if (!commandExists("rpm2cpio") || !commandExists("cpio")) {
      fail("rpm2cpio and cpio are required to verify .rpm packages")
    }
    const rpmStream = spawnSync("rpm2cpio", [artifactPath], { maxBuffer: 512 * 1024 * 1024 })
    if (rpmStream.status !== 0) {
      fail(`rpm2cpio failed for ${artifactPath}: ${rpmStream.stderr?.toString() || ""}`)
    }
    const cpioRes = spawnSync("cpio", ["-idmu", "--quiet"], {
      cwd: tmpDir,
      input: rpmStream.stdout,
      encoding: "utf8",
    })
    if (cpioRes.status !== 0) {
      fail(`cpio extraction failed for ${artifactPath}: ${cpioRes.stderr || ""}`)
    }
    return { format: "rpm", extractedRoot: tmpDir, packageMetadata: null }
  }

  if (lower.endsWith(".appimage")) {
    const runnablePath = path.join(tmpDir, "bundle.AppImage")
    fs.copyFileSync(artifactPath, runnablePath)
    fs.chmodSync(runnablePath, 0o755)
    runCmd(runnablePath, ["--appimage-extract"], {
      cwd: tmpDir,
      env: { ...process.env, APPIMAGE_EXTRACT_AND_RUN: "1" },
    })
    const squashfsRoot = path.join(tmpDir, "squashfs-root")
    if (!fs.existsSync(squashfsRoot)) {
      fail(`AppImage extraction did not produce ${squashfsRoot}`)
    }
    return { format: "appimage", extractedRoot: squashfsRoot, packageMetadata: null }
  }

  fail(`unsupported Linux artifact extension for '${artifactPath}' (expected .deb, .rpm, or .AppImage)`)
}

function verifyLinuxArtifact(artifactPath, spec) {
  const artifactSha = sha256File(artifactPath)
  const artifactSize = fs.statSync(artifactPath).size
  const tmpDir = path.join(TEMP_VERIFY_DIR, `linux-verify-${process.pid}-${Date.now()}`)
  fs.rmSync(tmpDir, { recursive: true, force: true })
  fs.mkdirSync(tmpDir, { recursive: true })

  try {
    const { format, extractedRoot, packageMetadata } = extractLinuxBundle(artifactPath, tmpDir)
    const appExecutable = findLinuxAppExecutable(path.join(extractedRoot, "usr/bin"))
    const pdfiumPath = path.join(extractedRoot, "usr/lib/ShiroReader/native/libpdfium.so")
    const licensePath = path.join(extractedRoot, "usr/lib/ShiroReader/native/PDFIUM-LICENSE.txt")
    const pdfiumLicPath = path.join(extractedRoot, "usr/lib/ShiroReader/native/licenses/pdfium.txt")

    assertExactFileDigest(
      pdfiumPath,
      spec.extractedBinarySizeBytes,
      spec.extractedBinarySha256,
      "packaged usr/lib/ShiroReader/native/libpdfium.so"
    )
    assertExactFileDigest(
      licensePath,
      undefined,
      spec.licenseSha256,
      "packaged usr/lib/ShiroReader/native/PDFIUM-LICENSE.txt"
    )
    if (!fs.existsSync(pdfiumLicPath)) {
      fail("packaged Linux bundle missing usr/lib/ShiroReader/native/licenses/pdfium.txt")
    }

    const pdfiumElf = inspectElfX86_64(pdfiumPath, {
      allowedDtNeeded: spec.allowedDtNeeded,
      allowedTypes: [3],
      requireReadelf: true,
    })
    const appExecutableElf = inspectElfX86_64(appExecutable, {
      allowedDtNeeded: null,
      allowedTypes: [2, 3],
      requireReadelf: false,
    })

    const desktopDir = path.join(extractedRoot, "usr/share/applications")
    const desktopEntries = fs.existsSync(desktopDir)
      ? fs.readdirSync(desktopDir).filter((name) => name.endsWith(".desktop"))
      : []

    const result = {
      passed: true,
      target: "linux-x64",
      format,
      artifactPath: path.relative(REPO_ROOT, artifactPath),
      artifactSizeBytes: artifactSize,
      artifactSha256: artifactSha,
      appExecutable: path.relative(extractedRoot, appExecutable),
      appExecutableElf,
      bundledPdfiumElf: pdfiumElf,
      bundledPdfiumSha256: spec.extractedBinarySha256,
      bundledLicenseSha256: spec.licenseSha256,
      desktopEntries,
      packageMetadata,
    }
    log(`Verified Linux ${format} package: ${result.artifactPath} (sha256=${artifactSha})`)
    return result
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }
}

export function verifyArtifact(target, artifactPath, options = {}) {
  if (!fs.existsSync(artifactPath)) {
    fail(`artifact not found: ${artifactPath}`)
  }
  const lock = loadLock()
  const spec = resolveTargetSpec(lock, target)

  if (target === "windows-x64") {
    return verifyWindowsArtifact(artifactPath, spec, options)
  }

  if (target === "linux-x64") {
    return verifyLinuxArtifact(artifactPath, spec)
  }

  const ext = path.extname(artifactPath).toLowerCase()
  const artifactSha = sha256File(artifactPath)
  const artifactSize = fs.statSync(artifactPath).size

  if (ext === ".apk") {
    const details = verifyApkContents(artifactPath, spec, { requireSignature: true })
    const result = {
      passed: true,
      target,
      format: "apk",
      artifactPath: path.relative(REPO_ROOT, artifactPath),
      artifactSizeBytes: artifactSize,
      artifactSha256: artifactSha,
      ...details,
    }
    log(`Verified Android APK: ${result.artifactPath} (sha256=${artifactSha})`)
    return result
  }

  if (ext === ".aab") {
    const details = verifyAabArtifact(artifactPath, spec)
    const result = {
      passed: true,
      target,
      artifactPath: path.relative(REPO_ROOT, artifactPath),
      artifactSizeBytes: artifactSize,
      artifactSha256: artifactSha,
      ...details,
    }
    log(`Verified Android AAB: ${result.artifactPath} (sha256=${artifactSha})`)
    return result
  }

  fail(`unsupported Android artifact extension '${ext}' (expected .apk or .aab)`)
}
