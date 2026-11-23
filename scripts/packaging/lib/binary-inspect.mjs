import { execFileSync } from "node:child_process"
import fs from "node:fs"
import { fail } from "./cli.mjs"
import { findAndroidReadelf, findLinuxReadelf } from "./env.mjs"

export const DEFAULT_ANDROID_ALLOWED_DT_NEEDED = [
  "libc.so",
  "libm.so",
  "libdl.so",
  "liblog.so",
  "libandroid.so",
]

export const DEFAULT_LINUX_X64_ALLOWED_DT_NEEDED = [
  "libpthread.so.0",
  "libm.so.6",
  "libgcc_s.so.1",
  "libc.so.6",
  "ld-linux-x86-64.so.2",
]

export function inspectPeBuffer(buf, label, { require64Bit = true } = {}) {
  if (buf.length < 64 || buf[0] !== 0x4d || buf[1] !== 0x5a) {
    fail(`${label} is not a valid DOS/PE executable (missing MZ header)`)
  }
  const peOffset = buf.readUInt32LE(0x3c)
  if (
    peOffset + 26 > buf.length ||
    buf.subarray(peOffset, peOffset + 4).toString("ascii") !== "PE\0\0"
  ) {
    fail(`${label} is missing PE signature`)
  }
  const machine = buf.readUInt16LE(peOffset + 4)
  const optMagic = buf.readUInt16LE(peOffset + 24)
  if (require64Bit) {
    if (machine !== 0x8664) {
      fail(`${label} COFF machine is 0x${machine.toString(16)}, expected 0x8664 (x86_64)`)
    }
    if (optMagic !== 0x020b) {
      fail(`${label} optional header magic is 0x${optMagic.toString(16)}, expected 0x020b (PE32+)`)
    }
    return { arch: "x86_64", format: "PE32+" }
  }
  if (machine !== 0x14c && machine !== 0x8664) {
    fail(`${label} COFF machine is 0x${machine.toString(16)}, expected x86/x86_64 PE installer`)
  }
  return {
    arch: machine === 0x8664 ? "x86_64" : "x86 (NSIS installer stub)",
    format: optMagic === 0x020b ? "PE32+" : "PE32",
  }
}

export function inspectPeHeader(filePath, opts = {}) {
  return inspectPeBuffer(fs.readFileSync(filePath), filePath, opts)
}

export function inspectPe64(filePath) {
  return inspectPeHeader(filePath, { require64Bit: true })
}

function inspectElf64Buffer(
  buf,
  label,
  { expectedMachine, archName, allowedTypes = [3], minPtLoadAlign = 0 }
) {
  if (buf.length < 64 || buf[0] !== 0x7f || buf[1] !== 0x45 || buf[2] !== 0x4c || buf[3] !== 0x46) {
    fail(`${label} is not a valid ELF binary`)
  }
  const elfClass = buf[4]
  const dataEncoding = buf[5]
  if (elfClass !== 2 || dataEncoding !== 1) {
    fail(`${label} must be 64-bit little-endian ELF (got class=${elfClass}, data=${dataEncoding})`)
  }
  const eType = buf.readUInt16LE(16)
  const eMachine = buf.readUInt16LE(18)
  if (!allowedTypes.includes(eType)) {
    fail(`${label} ELF e_type=${eType}, expected one of [${allowedTypes.join(", ")}]`)
  }
  if (eMachine !== expectedMachine) {
    fail(`${label} ELF e_machine=${eMachine}, expected ${expectedMachine} (${archName})`)
  }
  const ePhoff = Number(buf.readBigUInt64LE(32))
  const ePhentsize = buf.readUInt16LE(54)
  const ePhnum = buf.readUInt16LE(56)
  if (ePhoff + ePhentsize * ePhnum > buf.length) {
    fail(`${label} program header table extends past EOF`)
  }

  const loadAlignments = []
  for (let i = 0; i < ePhnum; i += 1) {
    const off = ePhoff + i * ePhentsize
    const pType = buf.readUInt32LE(off)
    if (pType === 1) {
      const pAlign = Number(buf.readBigUInt64LE(off + 48))
      loadAlignments.push(pAlign)
      if (minPtLoadAlign > 0 && (pAlign < minPtLoadAlign || pAlign % minPtLoadAlign !== 0)) {
        fail(
          `${label} PT_LOAD segment #${i} has p_align=${pAlign}, requires >= ${minPtLoadAlign} (16 KiB page alignment)`
        )
      }
    }
  }
  if (loadAlignments.length === 0) {
    fail(`${label} has no PT_LOAD segments`)
  }

  return {
    arch: archName,
    format: "ELF64",
    elfType: eType === 2 ? "ET_EXEC" : "ET_DYN",
    ptLoadAlignments: loadAlignments,
  }
}

export function inspectElfAarch64Buffer(buf, label) {
  const { elfType: _ignored, ...info } = inspectElf64Buffer(buf, label, {
    expectedMachine: 183,
    archName: "aarch64",
    allowedTypes: [3],
    minPtLoadAlign: 16384,
  })
  return info
}

export function inspectElfX86_64Buffer(buf, label, { allowedTypes = [2, 3] } = {}) {
  return inspectElf64Buffer(buf, label, {
    expectedMachine: 62,
    archName: "x86_64",
    allowedTypes,
    minPtLoadAlign: 0,
  })
}

function inspectElfDtNeeded(filePath, headerInfo, { readelfPath, allowedDtNeeded, requireReadelf }) {
  if (!readelfPath) {
    if (requireReadelf) {
      fail(`readelf/llvm-readelf not found to verify DT_NEEDED for ${filePath}`)
    }
    return { ...headerInfo, dtNeeded: null }
  }

  const out = execFileSync(readelfPath, ["-d", filePath], { encoding: "utf8" })
  const needed = []
  for (const line of out.split(/\r?\n/)) {
    const match = /\(NEEDED\)\s+Shared library:\s+\[([^\]]+)\]/.exec(line)
    if (match) needed.push(match[1])
  }

  if (Array.isArray(allowedDtNeeded)) {
    const allowSet = new Set(allowedDtNeeded)
    const unexpected = needed.filter((lib) => !allowSet.has(lib))
    if (unexpected.length > 0) {
      fail(`${filePath} has unexpected DT_NEEDED libraries: ${unexpected.join(", ")}`)
    }
  }

  return {
    ...headerInfo,
    dtNeeded: needed,
  }
}

export function inspectElfAarch64(
  filePath,
  { allowedDtNeeded = DEFAULT_ANDROID_ALLOWED_DT_NEEDED, requireReadelf = true } = {}
) {
  const headerInfo = inspectElfAarch64Buffer(fs.readFileSync(filePath), filePath)
  const readelfPath = findAndroidReadelf()
  if (!readelfPath && requireReadelf) {
    fail(
      `llvm-readelf not found (set ANDROID_NDK_HOME or install llvm-readelf) to verify DT_NEEDED for ${filePath}`
    )
  }
  return inspectElfDtNeeded(filePath, headerInfo, {
    readelfPath,
    allowedDtNeeded,
    requireReadelf,
  })
}

export function inspectElfX86_64(
  filePath,
  {
    allowedDtNeeded = DEFAULT_LINUX_X64_ALLOWED_DT_NEEDED,
    allowedTypes = [2, 3],
    requireReadelf = true,
  } = {}
) {
  const headerInfo = inspectElfX86_64Buffer(fs.readFileSync(filePath), filePath, { allowedTypes })
  const readelfPath = findLinuxReadelf()
  return inspectElfDtNeeded(filePath, headerInfo, {
    readelfPath,
    allowedDtNeeded,
    requireReadelf,
  })
}
