import fs from "node:fs"
import path from "node:path"
import process from "node:process"
import { fail, log } from "./cli.mjs"
import { sha256File } from "./crypto.mjs"
import {
  captureCmd,
  collectToolVersions,
  commandExists,
  findAndroidBuildTool,
  resolveGradleUserHome,
  resolveXwinCacheDir,
  runCmd,
  tauriBinPath,
} from "./env.mjs"
import { DESKTOP_DIR, loadLock, REPO_ROOT, REPORTS_DIR, resolveTargetSpec, TAURI_DIR, UI_DIR } from "./lock.mjs"
import { cmdPrepare } from "./prepare.mjs"
import { verifyArtifact } from "./verify.mjs"

export function checkOfflineReadiness(target) {
  const missing = []
  if (!fs.existsSync(path.join(UI_DIR, "node_modules/@tauri-apps/cli"))) {
    missing.push("apps/desktop/ui/node_modules (@tauri-apps/cli)")
  }
  if (!fs.existsSync(path.join(UI_DIR, "node_modules/vite"))) {
    missing.push("apps/desktop/ui/node_modules (vite)")
  }
  if (!fs.existsSync(path.join(REPO_ROOT, "Cargo.lock"))) {
    missing.push("Cargo.lock")
  } else {
    const offlineCargo = captureCmd("cargo", ["metadata", "--offline", "--no-deps"], {
      cwd: REPO_ROOT,
    })
    if (offlineCargo.status !== 0) {
      missing.push(`Cargo offline cache incomplete: ${offlineCargo.stderr.trim()}`)
    }
  }
  const stagingManifest = path.join(TAURI_DIR, "staging", target, "manifest.json")
  if (!fs.existsSync(stagingManifest)) {
    missing.push(`staging manifest (${path.relative(REPO_ROOT, stagingManifest)})`)
  }
  if (target === "android-arm64") {
    const gradlew = path.join(TAURI_DIR, "gen/android/gradlew")
    if (!fs.existsSync(gradlew)) {
      missing.push("apps/desktop/src-tauri/gen/android/gradlew")
    }
    const gradleHome = resolveGradleUserHome()
    const gradleCaches = path.join(gradleHome, "caches")
    if (!fs.existsSync(gradleCaches) || fs.readdirSync(gradleCaches).length === 0) {
      missing.push(`Gradle offline cache empty at ${gradleCaches}`)
    }
  } else if (target === "windows-x64" && process.platform === "linux") {
    const xwinDir = resolveXwinCacheDir()
    if (fs.readdirSync(xwinDir).length === 0) {
      missing.push(`xwin CRT/SDK cache empty at ${xwinDir}`)
    }
  }

  if (missing.length > 0) {
    fail(`offline build check failed; missing caches:\n - ${missing.join("\n - ")}`)
  }
}

function ensureDebugKeystore() {
  const keystoreDir = path.join(REPO_ROOT, ".cache/packaging/keystore")
  const keystorePath = path.join(keystoreDir, "debug.keystore")
  if (!fs.existsSync(keystorePath)) {
    fs.mkdirSync(keystoreDir, { recursive: true })
    runCmd("keytool", [
      "-genkeypair",
      "-v",
      "-keystore",
      keystorePath,
      "-storepass",
      "android",
      "-alias",
      "androiddebugkey",
      "-keypass",
      "android",
      "-keyalg",
      "RSA",
      "-keysize",
      "2048",
      "-validity",
      "10000",
      "-dname",
      "CN=Android Debug,O=Android,C=US",
    ])
  }
  return keystorePath
}

function signWithDebugKeystore(unsignedApkPath) {
  const apksigner = findAndroidBuildTool("apksigner")
  const zipalign = findAndroidBuildTool("zipalign")
  if (!apksigner || !zipalign) {
    fail("apksigner or zipalign not found in ANDROID_HOME/build-tools")
  }
  const keystorePath = ensureDebugKeystore()
  const alignedPath = unsignedApkPath.replace(/-unsigned\.apk$/, "-aligned.apk")
  const signedPath = unsignedApkPath.replace(/-unsigned\.apk$/, "-debugsigned.apk")
  runCmd(zipalign, ["-f", "-P", "16", "4", unsignedApkPath, alignedPath])
  try {
    runCmd(apksigner, [
      "sign",
      "--ks",
      keystorePath,
      "--ks-key-alias",
      "androiddebugkey",
      "--ks-pass",
      "pass:android",
      "--key-pass",
      "pass:android",
      "--out",
      signedPath,
      alignedPath,
    ])
  } finally {
    fs.rmSync(alignedPath, { force: true })
  }
  return signedPath
}

function writeBuildReport(target, report) {
  fs.mkdirSync(REPORTS_DIR, { recursive: true })
  const jsonPath = path.join(REPORTS_DIR, `${target}-build-report.json`)
  const mdPath = path.join(REPORTS_DIR, `${target}-build-report.md`)
  fs.writeFileSync(jsonPath, JSON.stringify(report, null, 2) + "\n")

  const md = `# Build Report — ${target}

- **Generated At**: ${report.generatedAt}
- **Git Revision**: \`${report.tools.gitRevision}\` (dirty=${report.tools.gitDirty})
- **Target / ABI**: \`${report.rustTarget}\` / \`${report.abi}\`
- **Artifact**: \`${report.artifactPath}\`
- **Artifact Size**: ${report.artifactSizeBytes} bytes
- **Artifact SHA-256**: \`${report.artifactSha256}\`
- **Signing**: \`${report.signing}\`
- **Bundled PDFium**: \`${report.pdfium.pdfiumTag}\` (SHA-256 \`${report.pdfium.extractedBinarySha256}\`)
- **Verification**: ${report.verification.passed ? "PASSED" : "FAILED"}

## Toolchain Versions

- Host: \`${report.tools.hostPlatform}\`
- Rust: \`${report.tools.rustc}\` / \`${report.tools.cargo}\`
- Node / pnpm: \`${report.tools.node}\` / \`${report.tools.pnpm}\`
- Java: \`${report.tools.java}\`
- cargo-xwin / makensis: \`${report.tools.cargoXwin}\` / \`${report.tools.makensis}\`
- dpkg-deb: \`${report.tools.dpkgDeb}\`

## Verification Details

\`\`\`json
${JSON.stringify(report.verification, null, 2)}
\`\`\`
`
  fs.writeFileSync(mdPath, md)
  log(`Wrote build report -> ${path.relative(REPO_ROOT, mdPath)}`)
}

export function findNewestArtifact(dir, predicate) {
  const matches = []
  for (const name of fs.readdirSync(dir)) {
    if (!predicate(name)) continue
    const fullPath = path.join(dir, name)
    const stat = fs.statSync(fullPath)
    if (!stat.isFile()) continue
    matches.push({ name, mtimeMs: stat.mtimeMs })
  }
  matches.sort((a, b) => b.mtimeMs - a.mtimeMs || b.name.localeCompare(a.name))
  return matches[0]?.name ?? null
}

export async function cmdBuild(flags) {
  const target = flags.target
  const offline = Boolean(flags.offline)
  const debug = Boolean(flags.debug)
  const lock = loadLock()
  resolveTargetSpec(lock, target)

  const manifest = await cmdPrepare({ target, offline })
  if (offline) {
    checkOfflineReadiness(target)
  }

  runCmd("pnpm", ["--dir", UI_DIR, "run", "build"], { cwd: REPO_ROOT })

  let artifactPath
  let signing = "unsigned"
  const profile = debug ? "debug" : "release"

  if (target === "windows-x64") {
    const tauri = tauriBinPath()
    const args = ["build", "--ci", "--target", "x86_64-pc-windows-msvc", "--bundles", "nsis"]
    if (debug) args.push("--debug")
    const env = { ...process.env }
    if (process.platform === "linux") {
      env.XWIN_CACHE_DIR = resolveXwinCacheDir()
      args.push("--runner", "cargo-xwin", "--no-sign")
    }
    if (offline) {
      args.push("--", "--offline")
    }
    runCmd(tauri, args, { cwd: DESKTOP_DIR, env })

    const nsisDir = path.join(REPO_ROOT, "target/x86_64-pc-windows-msvc", profile, "bundle/nsis")
    if (!fs.existsSync(nsisDir)) {
      fail(`expected NSIS bundle directory missing: ${nsisDir}`)
    }
    const exe = findNewestArtifact(nsisDir, (f) => f.endsWith(".exe"))
    if (!exe) {
      fail(`no .exe installer generated in ${nsisDir}`)
    }
    artifactPath = path.join(nsisDir, exe)
    signing = process.env.TAURI_SIGNING_PRIVATE_KEY ? "release" : "unsigned"
  } else if (target === "linux-x64") {
    if (process.platform !== "linux") {
      fail(`linux-x64 build must run on a Linux host (current host: ${process.platform})`)
    }
    const format = flags.format || "deb"
    const formatMeta = {
      deb: { bundle: "deb", ext: ".deb", requiredTool: "dpkg-deb" },
      rpm: { bundle: "rpm", ext: ".rpm", requiredTool: "rpmbuild" },
      appimage: { bundle: "appimage", ext: ".AppImage", requiredTool: null },
    }[format]
    if (!formatMeta) {
      fail("--format must be deb, rpm, or appimage for linux-x64")
    }
    if (formatMeta.requiredTool && !commandExists(formatMeta.requiredTool)) {
      fail(`required Linux bundler tool '${formatMeta.requiredTool}' not found in PATH`)
    }

    const tauri = tauriBinPath()
    const args = [
      "build",
      "--ci",
      "--target",
      "x86_64-unknown-linux-gnu",
      "--bundles",
      formatMeta.bundle,
    ]
    if (debug) args.push("--debug")
    if (offline) args.push("--", "--offline")

    const env = { ...process.env }
    if (format === "appimage" && !env.APPIMAGE_EXTRACT_AND_RUN) {
      env.APPIMAGE_EXTRACT_AND_RUN = "1"
    }
    runCmd(tauri, args, { cwd: DESKTOP_DIR, env })

    const bundleDir = path.join(
      REPO_ROOT,
      "target/x86_64-unknown-linux-gnu",
      profile,
      "bundle",
      formatMeta.bundle
    )
    if (!fs.existsSync(bundleDir)) {
      fail(`expected Linux bundle directory missing: ${bundleDir}`)
    }
    const bundleFile = findNewestArtifact(bundleDir, (f) =>
      f.toLowerCase().endsWith(formatMeta.ext.toLowerCase())
    )
    if (!bundleFile) {
      fail(`no ${formatMeta.ext} artifact generated in ${bundleDir}`)
    }
    artifactPath = path.join(bundleDir, bundleFile)
    signing = "unsigned"
  } else {
    const format = flags.format || "apk"
    if (format !== "apk" && format !== "aab") {
      fail("--format must be apk or aab for android-arm64")
    }
    const androidDir = path.join(TAURI_DIR, "gen/android")
    if (!fs.existsSync(androidDir)) {
      fail(`Android Gradle project not initialized at ${androidDir}`)
    }
    const tauri = tauriBinPath()
    const args = ["android", "build", "--ci", "--target", "aarch64"]
    args.push(format === "aab" ? "--aab" : "--apk")
    if (debug) args.push("--debug")
    if (offline) args.push("--", "--offline")

    const env = {
      ...process.env,
      GRADLE_USER_HOME: resolveGradleUserHome(),
    }
    if (offline) {
      env.CARGO_NET_OFFLINE = "true"
      env.GRADLE_OPTS = [env.GRADLE_OPTS, "-Dorg.gradle.offline=true"].filter(Boolean).join(" ")
    }
    runCmd(tauri, args, { cwd: DESKTOP_DIR, env })

    if (format === "apk") {
      const apkRoot = path.join(
        androidDir,
        debug ? "app/build/outputs/apk/universal/debug" : "app/build/outputs/apk/universal/release"
      )
      const fallbackApkRoot = path.join(
        androidDir,
        debug ? "app/build/outputs/apk/arm64/debug" : "app/build/outputs/apk/arm64/release"
      )
      const searchDir = fs.existsSync(apkRoot) ? apkRoot : fallbackApkRoot
      if (!fs.existsSync(searchDir)) {
        fail(`APK output directory not found: ${apkRoot} or ${fallbackApkRoot}`)
      }
      const apkFile =
        findNewestArtifact(searchDir, (f) => f.endsWith("-unsigned.apk")) ||
        findNewestArtifact(
          searchDir,
          (f) => f.endsWith(".apk") && !f.includes("-debugsigned.apk")
        ) ||
        findNewestArtifact(searchDir, (f) => f.endsWith(".apk"))
      if (!apkFile) {
        fail(`no .apk found in ${searchDir}`)
      }
      artifactPath = path.join(searchDir, apkFile)
      if (!debug && apkFile.includes("unsigned")) {
        artifactPath = signWithDebugKeystore(artifactPath)
        signing = "debug-keystore (apksigner)"
      } else {
        signing = debug ? "debug-keystore (gradle)" : "release"
      }
    } else {
      const aabRoot = path.join(
        androidDir,
        debug ? "app/build/outputs/bundle/universalDebug" : "app/build/outputs/bundle/universalRelease"
      )
      if (!fs.existsSync(aabRoot)) {
        fail(`AAB output directory not found: ${aabRoot}`)
      }
      const aabFile = findNewestArtifact(aabRoot, (f) => f.endsWith(".aab"))
      if (!aabFile) {
        fail(`no .aab found in ${aabRoot}`)
      }
      artifactPath = path.join(aabRoot, aabFile)
      signing = debug ? "debug" : "unsigned"
    }
  }

  const verification = verifyArtifact(target, artifactPath, { debug })
  const report = {
    generatedAt: new Date().toISOString(),
    target,
    abi: manifest.abi,
    rustTarget: manifest.rustTarget,
    artifactPath: path.relative(REPO_ROOT, artifactPath),
    artifactSizeBytes: fs.statSync(artifactPath).size,
    artifactSha256: sha256File(artifactPath),
    signing,
    pdfium: manifest,
    tools: collectToolVersions(),
    verification,
  }
  writeBuildReport(target, report)
}
