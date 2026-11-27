import fs from "node:fs"
import path from "node:path"
import process from "node:process"
import { fail, log } from "./cli.mjs"
import { captureCmd, commandExists, runCmd } from "./env.mjs"
import { FIXTURES_DIR, TEMP_VERIFY_DIR } from "./lock.mjs"
import { verifyArtifact } from "./verify.mjs"

const ANDROID_PACKAGE = "com.shiroaqua.shiroreader"
const ANDROID_ACTIVITY = `${ANDROID_PACKAGE}/.MainActivity`

export async function cmdSmoke(flags) {
  const target = flags.target
  const artifact = flags.artifact
  const device = flags.device
  if (!target || !artifact) {
    fail("smoke requires --target <windows-x64|android-arm64|linux-x64> --artifact <path>")
  }

  const resolvedArtifact = path.resolve(artifact)
  const verification = verifyArtifact(target, resolvedArtifact)
  log(`Artifact verification passed (${verification.artifactSha256}). Proceeding to smoke test...`)

  if (target === "android-arm64") {
    if (!commandExists("adb")) fail("adb not found in PATH")
    const devs = captureCmd("adb", ["devices"])
    const online = devs.stdout
      .split(/\r?\n/)
      .slice(1)
      .filter((line) => /\s+device$/.test(line.trim()))
    if (online.length === 0) {
      fail(
        "no online Android arm64 device attached via adb (connect a physical arm64 device and re-run)"
      )
    }

    const targetDevice = device || online[0].split(/\s+/)[0]
    const abiRes = captureCmd("adb", ["-s", targetDevice, "shell", "getprop", "ro.product.cpu.abi"])
    const deviceAbi = abiRes.stdout.trim()
    if (deviceAbi !== "arm64-v8a") {
      fail(`Android device ${targetDevice} ABI is '${deviceAbi}', expected 'arm64-v8a'`)
    }

    for (const fixture of ["smoke_pattern.pdf", "smoke_cover.epub"]) {
      const fixturePath = path.join(FIXTURES_DIR, fixture)
      if (fs.existsSync(fixturePath)) {
        runCmd("adb", ["-s", targetDevice, "push", fixturePath, "/sdcard/Download/"])
      }
    }

    runCmd("adb", ["-s", targetDevice, "install", "-r", resolvedArtifact])
    runCmd("adb", ["-s", targetDevice, "shell", "am", "start", "-W", "-n", ANDROID_ACTIVITY])

    const pidRes = captureCmd("adb", ["-s", targetDevice, "shell", "pidof", ANDROID_PACKAGE])
    if (pidRes.status !== 0 || pidRes.stdout.trim().length === 0) {
      fail(`app process '${ANDROID_PACKAGE}' is not running after launch on ${targetDevice}`)
    }
    log(
      `Launched ${ANDROID_PACKAGE} (pid=${pidRes.stdout.trim()}) on arm64-v8a device ${targetDevice}. Complete offline PDF/EPUB import check on device.`
    )
    return
  }

  if (target === "linux-x64") {
    if (process.platform !== "linux") {
      fail(`Linux smoke test requires a Linux host; current host is ${process.platform}`)
    }
    if (resolvedArtifact.toLowerCase().endsWith(".deb") && commandExists("dpkg-deb") && commandExists("ldd")) {
      const tmpDir = path.join(TEMP_VERIFY_DIR, `linux-smoke-${process.pid}`)
      fs.rmSync(tmpDir, { recursive: true, force: true })
      fs.mkdirSync(tmpDir, { recursive: true })
      try {
        runCmd("dpkg-deb", ["-x", resolvedArtifact, tmpDir])
        const exePath = path.join(tmpDir, "usr/bin/shiro-reader-client")
        const pdfiumPath = path.join(tmpDir, "usr/lib/ShiroReader/native/libpdfium.so")
        for (const bin of [exePath, pdfiumPath]) {
          if (!fs.existsSync(bin)) {
            fail(`expected binary missing from Linux package: ${path.relative(tmpDir, bin)}`)
          }
          const ldd = captureCmd("ldd", [bin])
          if (ldd.status !== 0 || ldd.stdout.includes("not found")) {
            fail(`unresolved dynamic libraries in ${bin}:\n${ldd.stdout}\n${ldd.stderr}`)
          }
        }
        log(`Verified Linux dynamic library resolution via ldd for binary and bundled libpdfium.so.`)
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true })
      }
    }
    if (!process.env.DISPLAY && !process.env.WAYLAND_DISPLAY) {
      log(`No DISPLAY/WAYLAND_DISPLAY detected; install ${resolvedArtifact} in a desktop session to run GUI smoke verification.`)
    } else {
      log(`Linux artifact verified; install ${resolvedArtifact} and launch ShiroReader for offline PDF/EPUB verification.`)
    }
    return
  }

  if (process.platform !== "win32") {
    fail(
      `Windows runtime smoke test requires a Windows x64 host/VM; current host is ${process.platform}`
    )
  }
  log(`Windows artifact verified; run installer ${resolvedArtifact} in an offline Windows environment.`)
}
