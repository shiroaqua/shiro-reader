import assert from "node:assert/strict"
import fs from "node:fs"
import os from "node:os"
import path from "node:path"
import test from "node:test"
import { PackagingError, parseArgs } from "../lib/cli.mjs"
import {
  inspectElfAarch64Buffer,
  inspectElfX86_64Buffer,
  inspectPeBuffer,
} from "../lib/binary-inspect.mjs"
import { FIXTURES_DIR, loadLock, REPO_ROOT } from "../lib/lock.mjs"
import { findNewestArtifact } from "../lib/build.mjs"
import { assertSafeEntryPath } from "../lib/prepare.mjs"
import { resolveWindowsBuildProfileDir } from "../lib/verify.mjs"
import { openZip } from "../lib/zip.mjs"

test("parseArgs parses valid flags and rejects unknown flags", () => {
  assert.deepEqual(parseArgs(["prepare", "--target", "windows-x64", "--offline"]), {
    command: "prepare",
    flags: { target: "windows-x64", offline: true },
  })
  assert.deepEqual(parseArgs(["build", "--target=android-arm64", "--format=aab", "--debug"]), {
    command: "build",
    flags: { target: "android-arm64", format: "aab", debug: true },
  })
  assert.deepEqual(parseArgs(["build", "--target", "linux-x64", "--format", "deb"]), {
    command: "build",
    flags: { target: "linux-x64", format: "deb" },
  })
  assert.throws(
    () => parseArgs(["prepare", "--target", "windows-x64", "--typo"]),
    PackagingError
  )
  assert.throws(() => parseArgs(["verify", "--target"]), PackagingError)
})

test("assets.lock.json passes strict schema validation with locked extracted digests", () => {
  const lock = loadLock()
  assert.equal(lock.schemaVersion, 2)
  assert.equal(lock.pdfium.targets["windows-x64"].extractedBinarySha256.length, 64)
  assert.equal(lock.pdfium.targets["android-arm64"].extractedBinarySha256.length, 64)
  assert.equal(lock.pdfium.targets["linux-x64"].extractedBinarySha256.length, 64)
  assert.equal(lock.pdfium.targets["windows-x64"].licenseSha256.length, 64)
  assert.equal(lock.pdfium.targets["android-arm64"].licenseSha256.length, 64)
  assert.equal(lock.pdfium.targets["linux-x64"].licenseSha256.length, 64)
})

test("assertSafeEntryPath rejects traversal, absolute paths, drive letters, and NUL bytes", () => {
  assert.doesNotThrow(() => assertSafeEntryPath("lib/libpdfium.so"))
  assert.doesNotThrow(() => assertSafeEntryPath("licenses/pdfium.txt"))
  for (const bad of ["../evil", "a/../../b", "/etc/passwd", "\\windows\\sys", "C:\\foo", "a\0b"]) {
    assert.throws(() => assertSafeEntryPath(bad), PackagingError)
  }
})

test("inspectPeBuffer validates PE32+ x86_64 and rejects wrong architecture", () => {
  const pe = Buffer.alloc(128, 0)
  pe[0] = 0x4d
  pe[1] = 0x5a
  pe.writeUInt32LE(64, 0x3c)
  pe.write("PE\0\0", 64, "ascii")
  pe.writeUInt16LE(0x8664, 68)
  pe.writeUInt16LE(0x020b, 88)

  assert.deepEqual(inspectPeBuffer(pe, "test.dll", { require64Bit: true }), {
    arch: "x86_64",
    format: "PE32+",
  })

  pe.writeUInt16LE(0x014c, 68)
  assert.throws(() => inspectPeBuffer(pe, "x86.dll", { require64Bit: true }), PackagingError)
})

test("inspectElfAarch64Buffer enforces ELF64 AArch64 ET_DYN and 16 KiB PT_LOAD alignment", () => {
  const buildElf = (pAlign, eMachine = 183) => {
    const buf = Buffer.alloc(128, 0)
    buf[0] = 0x7f
    buf[1] = 0x45
    buf[2] = 0x4c
    buf[3] = 0x46
    buf[4] = 2 // ELF64
    buf[5] = 1 // little-endian
    buf.writeUInt16LE(3, 16) // ET_DYN
    buf.writeUInt16LE(eMachine, 18)
    buf.writeBigUInt64LE(64n, 32) // e_phoff
    buf.writeUInt16LE(56, 54) // e_phentsize
    buf.writeUInt16LE(1, 56) // e_phnum
    buf.writeUInt32LE(1, 64) // PT_LOAD
    buf.writeBigUInt64LE(BigInt(pAlign), 64 + 48)
    return buf
  }

  assert.deepEqual(inspectElfAarch64Buffer(buildElf(16384), "good.so"), {
    arch: "aarch64",
    format: "ELF64",
    ptLoadAlignments: [16384],
  })
  assert.throws(() => inspectElfAarch64Buffer(buildElf(4096), "bad4k.so"), PackagingError)
  assert.throws(() => inspectElfAarch64Buffer(buildElf(16384, 62), "x86_64.so"), PackagingError)
})

test("inspectElfX86_64Buffer validates ELF64 x86_64 ET_DYN/ET_EXEC and rejects wrong arch", () => {
  const buildElf = (eType, eMachine = 62) => {
    const buf = Buffer.alloc(128, 0)
    buf[0] = 0x7f
    buf[1] = 0x45
    buf[2] = 0x4c
    buf[3] = 0x46
    buf[4] = 2 // ELF64
    buf[5] = 1 // little-endian
    buf.writeUInt16LE(eType, 16)
    buf.writeUInt16LE(eMachine, 18)
    buf.writeBigUInt64LE(64n, 32) // e_phoff
    buf.writeUInt16LE(56, 54) // e_phentsize
    buf.writeUInt16LE(1, 56) // e_phnum
    buf.writeUInt32LE(1, 64) // PT_LOAD
    buf.writeBigUInt64LE(4096n, 64 + 48)
    return buf
  }

  assert.deepEqual(inspectElfX86_64Buffer(buildElf(3), "libpdfium.so"), {
    arch: "x86_64",
    format: "ELF64",
    elfType: "ET_DYN",
    ptLoadAlignments: [4096],
  })
  assert.deepEqual(inspectElfX86_64Buffer(buildElf(2), "shiro-reader-desktop"), {
    arch: "x86_64",
    format: "ELF64",
    elfType: "ET_EXEC",
    ptLoadAlignments: [4096],
  })
  assert.throws(() => inspectElfX86_64Buffer(buildElf(3, 183), "aarch64.so"), PackagingError)
})

test("smoke_cover.epub is a valid EPUB 3 ZIP with real PNG cover entry (pure JS ZIP reader)", () => {
  const epubPath = path.join(FIXTURES_DIR, "smoke_cover.epub")
  const zip = openZip(epubPath)
  assert.equal(zip.entries[0].name, "mimetype")
  assert.equal(zip.entries[0].compressMethod, 0)
  assert.equal(zip.readMember("mimetype").toString("ascii"), "application/epub+zip")

  const opf = zip.readMember("EPUB/content.opf").toString("utf8")
  assert.match(opf, /href="cover\.png"/)
  assert.match(opf, /media-type="image\/png"/)

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "shiro-zip-test-"))
  try {
    const outCover = path.join(tmpDir, "cover.png")
    const pngBytes = zip.extractMember("EPUB/cover.png", outCover)
    assert.deepEqual(pngBytes.subarray(0, 8), Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))
    assert.equal(fs.statSync(outCover).size, pngBytes.length)
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }
})

test("findNewestArtifact selects the most recently modified matching file", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "shiro-newest-artifact-"))
  try {
    const older = path.join(tmpDir, "ShiroReader_0.1.0_x64-setup.exe")
    const newer = path.join(tmpDir, "ShiroReader_0.2.0_x64-setup.exe")
    const ignoredDir = path.join(tmpDir, "nested.exe")
    fs.writeFileSync(older, "old")
    fs.writeFileSync(newer, "new")
    fs.mkdirSync(ignoredDir)

    const oldTime = new Date("2025-01-01T00:00:00Z")
    const newTime = new Date("2026-01-01T00:00:00Z")
    fs.utimesSync(older, oldTime, oldTime)
    fs.utimesSync(newer, newTime, newTime)
    assert.equal(
      findNewestArtifact(tmpDir, (name) => name.endsWith(".exe")),
      "ShiroReader_0.2.0_x64-setup.exe"
    )

    // Even if a lexicographically earlier file is newer, mtime wins.
    const newestTime = new Date("2027-01-01T00:00:00Z")
    fs.utimesSync(older, newestTime, newestTime)
    assert.equal(
      findNewestArtifact(tmpDir, (name) => name.endsWith(".exe")),
      "ShiroReader_0.1.0_x64-setup.exe"
    )
    assert.equal(
      findNewestArtifact(tmpDir, (name) => name.endsWith(".deb")),
      null
    )
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }
})

test("resolveWindowsBuildProfileDir respects debug builds and artifact path", () => {
  const debugArtifact = path.join(
    REPO_ROOT,
    "target/x86_64-pc-windows-msvc/debug/bundle/nsis/ShiroReader_0.1.0_x64-setup.exe"
  )
  const releaseArtifact = path.join(
    REPO_ROOT,
    "target/x86_64-pc-windows-msvc/release/bundle/nsis/ShiroReader_0.1.0_x64-setup.exe"
  )
  const expectedDebug = path.join(REPO_ROOT, "target/x86_64-pc-windows-msvc/debug")
  const expectedRelease = path.join(REPO_ROOT, "target/x86_64-pc-windows-msvc/release")

  assert.equal(resolveWindowsBuildProfileDir(debugArtifact), expectedDebug)
  assert.equal(resolveWindowsBuildProfileDir(releaseArtifact), expectedRelease)
  assert.equal(resolveWindowsBuildProfileDir("/tmp/copied-setup.exe", { debug: true }), expectedDebug)
  assert.equal(resolveWindowsBuildProfileDir("/tmp/copied-setup.exe", { debug: false }), expectedRelease)
  assert.equal(resolveWindowsBuildProfileDir("/tmp/copied-setup.exe"), expectedRelease)
})
