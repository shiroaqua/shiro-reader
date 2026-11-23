import fs from "node:fs"
import path from "node:path"
import zlib from "node:zlib"
import { fail } from "./cli.mjs"

const SIG_EOCD = 0x06054b50
const SIG_ZIP64_EOCD_LOCATOR = 0x07064b50
const SIG_ZIP64_EOCD = 0x06064b50
const SIG_CENTRAL_DIR = 0x02014b50
const SIG_LOCAL_FILE = 0x04034b50

const MAX_EOCD_SEARCH = 65557

function readU64AsNumber(buf, offset) {
  const val = buf.readBigUInt64LE(offset)
  if (val > BigInt(Number.MAX_SAFE_INTEGER)) {
    fail(`ZIP64 value exceeds safe integer range: ${val}`)
  }
  return Number(val)
}

function locateCentralDirectory(buf, zipPath) {
  const minPos = Math.max(0, buf.length - MAX_EOCD_SEARCH)
  let eocdPos = -1
  for (let i = buf.length - 22; i >= minPos; i -= 1) {
    if (buf.readUInt32LE(i) === SIG_EOCD) {
      const commentLen = buf.readUInt16LE(i + 20)
      if (i + 22 + commentLen === buf.length) {
        eocdPos = i
        break
      }
    }
  }
  if (eocdPos === -1) {
    fail(`${zipPath} is not a valid ZIP archive (EOCD not found)`)
  }

  let entryCount = buf.readUInt16LE(eocdPos + 10)
  let cdSize = buf.readUInt32LE(eocdPos + 12)
  let cdOffset = buf.readUInt32LE(eocdPos + 16)

  if (entryCount === 0xffff || cdSize === 0xffffffff || cdOffset === 0xffffffff) {
    const locatorPos = eocdPos - 20
    if (locatorPos < 0 || buf.readUInt32LE(locatorPos) !== SIG_ZIP64_EOCD_LOCATOR) {
      fail(`${zipPath} uses ZIP64 sentinels without a ZIP64 EOCD locator`)
    }
    const zip64EocdOffset = readU64AsNumber(buf, locatorPos + 8)
    if (zip64EocdOffset + 56 > buf.length || buf.readUInt32LE(zip64EocdOffset) !== SIG_ZIP64_EOCD) {
      fail(`${zipPath} has invalid ZIP64 EOCD record`)
    }
    entryCount = readU64AsNumber(buf, zip64EocdOffset + 32)
    cdSize = readU64AsNumber(buf, zip64EocdOffset + 40)
    cdOffset = readU64AsNumber(buf, zip64EocdOffset + 48)
  }

  if (cdOffset + cdSize > buf.length) {
    fail(`${zipPath} central directory extends past end of file`)
  }

  return { entryCount, cdOffset, cdSize }
}

function parseZip64Extra(extraBuf, compressedSize, uncompressedSize, localHeaderOffset) {
  let cursor = 0
  let outCompressed = compressedSize
  let outUncompressed = uncompressedSize
  let outLocalOffset = localHeaderOffset

  while (cursor + 4 <= extraBuf.length) {
    const headerId = extraBuf.readUInt16LE(cursor)
    const dataSize = extraBuf.readUInt16LE(cursor + 2)
    cursor += 4
    if (cursor + dataSize > extraBuf.length) break
    if (headerId === 0x0001) {
      let inner = cursor
      if (outUncompressed === 0xffffffff && inner + 8 <= cursor + dataSize) {
        outUncompressed = readU64AsNumber(extraBuf, inner)
        inner += 8
      }
      if (outCompressed === 0xffffffff && inner + 8 <= cursor + dataSize) {
        outCompressed = readU64AsNumber(extraBuf, inner)
        inner += 8
      }
      if (outLocalOffset === 0xffffffff && inner + 8 <= cursor + dataSize) {
        outLocalOffset = readU64AsNumber(extraBuf, inner)
      }
      break
    }
    cursor += dataSize
  }

  return {
    compressedSize: outCompressed,
    uncompressedSize: outUncompressed,
    localHeaderOffset: outLocalOffset,
  }
}

export function openZip(zipPath) {
  const buf = fs.readFileSync(zipPath)
  const { entryCount, cdOffset, cdSize } = locateCentralDirectory(buf, zipPath)
  const entries = []
  const byName = new Map()

  let pos = cdOffset
  const cdEnd = cdOffset + cdSize
  for (let i = 0; i < entryCount; i += 1) {
    if (pos + 46 > cdEnd || buf.readUInt32LE(pos) !== SIG_CENTRAL_DIR) {
      fail(`${zipPath} has corrupt central directory entry #${i}`)
    }
    const compressMethod = buf.readUInt16LE(pos + 10)
    const crc32 = buf.readUInt32LE(pos + 16)
    let compressedSize = buf.readUInt32LE(pos + 20)
    let uncompressedSize = buf.readUInt32LE(pos + 24)
    const nameLen = buf.readUInt16LE(pos + 28)
    const extraLen = buf.readUInt16LE(pos + 30)
    const commentLen = buf.readUInt16LE(pos + 32)
    let localHeaderOffset = buf.readUInt32LE(pos + 42)

    const nameStart = pos + 46
    const extraStart = nameStart + nameLen
    const nextPos = extraStart + extraLen + commentLen
    if (nextPos > cdEnd) {
      fail(`${zipPath} central directory entry #${i} overflows central directory`)
    }

    const name = buf.subarray(nameStart, extraStart).toString("utf8")
    if (
      compressedSize === 0xffffffff ||
      uncompressedSize === 0xffffffff ||
      localHeaderOffset === 0xffffffff
    ) {
      const z64 = parseZip64Extra(
        buf.subarray(extraStart, extraStart + extraLen),
        compressedSize,
        uncompressedSize,
        localHeaderOffset
      )
      compressedSize = z64.compressedSize
      uncompressedSize = z64.uncompressedSize
      localHeaderOffset = z64.localHeaderOffset
    }

    const entry = {
      name,
      compressMethod,
      crc32,
      compressedSize,
      uncompressedSize,
      localHeaderOffset,
    }
    entries.push(entry)
    byName.set(name, entry)
    pos = nextPos
  }

  function readMember(memberName) {
    const entry = byName.get(memberName)
    if (!entry) {
      fail(`${zipPath} missing ZIP member '${memberName}'`)
    }
    const off = entry.localHeaderOffset
    if (off + 30 > buf.length || buf.readUInt32LE(off) !== SIG_LOCAL_FILE) {
      fail(`${zipPath} has invalid local file header for '${memberName}'`)
    }
    const localNameLen = buf.readUInt16LE(off + 26)
    const localExtraLen = buf.readUInt16LE(off + 28)
    const dataStart = off + 30 + localNameLen + localExtraLen
    const dataEnd = dataStart + entry.compressedSize
    if (dataEnd > buf.length) {
      fail(`${zipPath} compressed data truncated for '${memberName}'`)
    }
    const raw = buf.subarray(dataStart, dataEnd)
    let out
    if (entry.compressMethod === 0) {
      out = Buffer.from(raw)
    } else if (entry.compressMethod === 8) {
      out = zlib.inflateRawSync(raw)
    } else {
      fail(`${zipPath} member '${memberName}' uses unsupported compression method ${entry.compressMethod}`)
    }
    if (out.length !== entry.uncompressedSize) {
      fail(
        `${zipPath} member '${memberName}' size mismatch: expected ${entry.uncompressedSize}, got ${out.length}`
      )
    }
    return out
  }

  function extractMember(memberName, destPath) {
    const data = readMember(memberName)
    fs.mkdirSync(path.dirname(destPath), { recursive: true })
    fs.writeFileSync(destPath, data)
    return data
  }

  return { entries, byName, readMember, extractMember }
}
