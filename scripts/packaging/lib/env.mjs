import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import process from "node:process"
import { fail, log } from "./cli.mjs"
import { REPO_ROOT, UI_DIR } from "./lock.mjs"

export function commandExists(cmd) {
  const checker = process.platform === "win32" ? "where" : "which"
  const res = spawnSync(checker, [cmd], { stdio: "ignore" })
  return res.status === 0
}

export function runCmd(cmd, args, opts = {}) {
  log(`$ ${cmd} ${args.join(" ")}`)
  const res = spawnSync(cmd, args, {
    stdio: "inherit",
    ...opts,
  })
  if (res.error) {
    fail(`failed to spawn '${cmd}': ${res.error.message}`)
  }
  if (res.status !== 0) {
    fail(`command failed (${res.status}): ${cmd} ${args.join(" ")}`)
  }
}

export function captureCmd(cmd, args, opts = {}) {
  return spawnSync(cmd, args, {
    encoding: "utf8",
    ...opts,
  })
}

export function findAndroidReadelf(env = process.env) {
  const ndk = env.ANDROID_NDK_HOME || env.NDK_HOME
  if (ndk) {
    const hostTag = process.platform === "win32" ? "windows-x86_64" : "linux-x86_64"
    const exe = process.platform === "win32" ? "llvm-readelf.exe" : "llvm-readelf"
    const candidate = path.join(ndk, `toolchains/llvm/prebuilt/${hostTag}/bin`, exe)
    if (fs.existsSync(candidate)) return candidate
  }
  return commandExists("llvm-readelf") ? "llvm-readelf" : null
}

export function findLinuxReadelf(env = process.env) {
  for (const candidate of ["readelf", "llvm-readelf"]) {
    if (commandExists(candidate)) return candidate
  }
  return findAndroidReadelf(env)
}

export function findBundletool(env = process.env) {
  if (commandExists("bundletool")) {
    return { cmd: "bundletool", prefixArgs: [] }
  }
  if (env.BUNDLETOOL_JAR && fs.existsSync(env.BUNDLETOOL_JAR)) {
    return { cmd: "java", prefixArgs: ["-jar", env.BUNDLETOOL_JAR] }
  }
  const cachedToolsDir = path.join(REPO_ROOT, ".cache/packaging/tools")
  if (fs.existsSync(cachedToolsDir)) {
    const jar = fs
      .readdirSync(cachedToolsDir)
      .sort()
      .reverse()
      .find((name) => name.startsWith("bundletool-all-") && name.endsWith(".jar"))
    if (jar) {
      return { cmd: "java", prefixArgs: ["-jar", path.join(cachedToolsDir, jar)] }
    }
  }
  return null
}

export function findAndroidBuildTool(toolName, env = process.env) {
  const sdk = env.ANDROID_HOME || env.ANDROID_SDK_ROOT
  const exe = process.platform === "win32" ? `${toolName}.exe` : toolName
  if (sdk) {
    const btDir = path.join(sdk, "build-tools")
    if (fs.existsSync(btDir)) {
      const versions = fs.readdirSync(btDir).sort().reverse()
      for (const ver of versions) {
        const candidate = path.join(btDir, ver, exe)
        if (fs.existsSync(candidate)) return candidate
      }
    }
  }
  return commandExists(exe) ? exe : null
}

export function find7z() {
  for (const candidate of ["7z", "7za", "7zz"]) {
    if (commandExists(candidate)) return candidate
  }
  return null
}

export function tauriBinPath() {
  return path.join(
    UI_DIR,
    "node_modules/.bin",
    process.platform === "win32" ? "tauri.cmd" : "tauri"
  )
}

export function resolveWritableDir(configuredDir, fallbackPath) {
  if (configuredDir) {
    try {
      fs.accessSync(configuredDir, fs.constants.W_OK)
      return configuredDir
    } catch {
      // Fall back to repository cache directory when preconfigured cache is read-only.
    }
  }
  fs.mkdirSync(fallbackPath, { recursive: true })
  return fallbackPath
}

export function resolveXwinCacheDir(env = process.env) {
  return resolveWritableDir(env.XWIN_CACHE_DIR, path.join(REPO_ROOT, ".cache/xwin"))
}

export function resolveGradleUserHome(env = process.env) {
  return resolveWritableDir(env.GRADLE_USER_HOME, path.join(REPO_ROOT, ".cache/gradle"))
}

export function collectToolVersions() {
  const versionOf = (cmd, args) => {
    const res = captureCmd(cmd, args)
    return res.status === 0 ? res.stdout.trim().split(/\r?\n/)[0] : "unavailable"
  }
  const gitRev = versionOf("git", ["rev-parse", "HEAD"])
  const gitDirty = captureCmd("git", ["status", "--short"]).stdout.trim().length > 0
  return {
    gitRevision: gitRev,
    gitDirty,
    hostPlatform: `${process.platform}/${process.arch}`,
    node: process.version,
    pnpm: versionOf("pnpm", ["--version"]),
    rustc: versionOf("rustc", ["--version"]),
    cargo: versionOf("cargo", ["--version"]),
    java: captureCmd("java", ["-version"]).stderr.trim().split(/\r?\n/)[0] || "unavailable",
    cargoXwin: versionOf("cargo-xwin", ["--version"]),
    makensis: versionOf("makensis", ["-VERSION"]),
    dpkgDeb: versionOf("dpkg-deb", ["--version"]),
  }
}
