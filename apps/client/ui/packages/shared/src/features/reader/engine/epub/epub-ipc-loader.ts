import type { EpubLoader } from "foliate-js/epub.js"
import {
  READER_ERROR_CODES,
  readerBridge,
  type OpenEpubResponse,
  type ReaderBridge,
} from "@shiro/ui-shared/api/reader-bridge"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import {
  inferEpubEntryMime,
  lookupEntrySize,
  resolveArchiveEntryPath,
} from "./mime"

function rotl32(value: number, bits: number): number {
  return ((value << bits) | (value >>> (32 - bits))) >>> 0
}

export function sha1PureJs(data: Uint8Array): Uint8Array {
  const bitLen = data.byteLength * 8
  const paddedLen = Math.ceil((data.byteLength + 9) / 64) * 64
  const padded = new Uint8Array(paddedLen)
  padded.set(data, 0)
  padded[data.byteLength] = 0x80

  const view = new DataView(padded.buffer)
  const highBits = Math.floor(bitLen / 0x1_0000_0000)
  const lowBits = bitLen >>> 0
  view.setUint32(paddedLen - 8, highBits, false)
  view.setUint32(paddedLen - 4, lowBits, false)

  let h0 = 0x67452301
  let h1 = 0xefcdab89
  let h2 = 0x98badcfe
  let h3 = 0x10325476
  let h4 = 0xc3d2e1f0

  const words = new Uint32Array(80)

  for (let offset = 0; offset < paddedLen; offset += 64) {
    for (let i = 0; i < 16; i++) {
      words[i] = view.getUint32(offset + i * 4, false)
    }
    for (let i = 16; i < 80; i++) {
      const x =
        (words[i - 3] ?? 0) ^
        (words[i - 8] ?? 0) ^
        (words[i - 14] ?? 0) ^
        (words[i - 16] ?? 0)
      words[i] = rotl32(x, 1)
    }

    let a = h0
    let b = h1
    let c = h2
    let d = h3
    let e = h4

    for (let i = 0; i < 80; i++) {
      let f: number
      let k: number
      if (i < 20) {
        f = ((b & c) | (~b & d)) >>> 0
        k = 0x5a827999
      } else if (i < 40) {
        f = (b ^ c ^ d) >>> 0
        k = 0x6ed9eba1
      } else if (i < 60) {
        f = ((b & c) | (b & d) | (c & d)) >>> 0
        k = 0x8f1bbcdc
      } else {
        f = (b ^ c ^ d) >>> 0
        k = 0xca62c1d6
      }

      const temp = (rotl32(a, 5) + f + e + k + (words[i] ?? 0)) >>> 0
      e = d
      d = c
      c = rotl32(b, 30)
      b = a
      a = temp
    }

    h0 = (h0 + a) >>> 0
    h1 = (h1 + b) >>> 0
    h2 = (h2 + c) >>> 0
    h3 = (h3 + d) >>> 0
    h4 = (h4 + e) >>> 0
  }

  const digest = new Uint8Array(20)
  const outView = new DataView(digest.buffer)
  outView.setUint32(0, h0, false)
  outView.setUint32(4, h1, false)
  outView.setUint32(8, h2, false)
  outView.setUint32(12, h3, false)
  outView.setUint32(16, h4, false)
  return digest
}

export async function sha1Bytes(input: string): Promise<Uint8Array> {
  const bytes = new TextEncoder().encode(input)
  const subtle = globalThis.crypto?.subtle
  if (subtle && typeof subtle.digest === "function") {
    try {
      const digest = await subtle.digest("SHA-1", bytes)
      return new Uint8Array(digest)
    } catch {
      // Fall back to pure-JS SHA-1 in restricted/non-secure WebView contexts.
    }
  }
  return sha1PureJs(bytes)
}

function isCommandErrorCode(error: unknown, code: number): boolean {
  return error instanceof CommandError && error.payload.code === code
}

export interface EpubIpcLoaderOpenOptions {
  readonly bookId: string
  readonly reattachSessionId?: string | undefined
  readonly manageSessionLifecycleInFrontend?: boolean | undefined
  readonly bridge?: ReaderBridge | undefined
  readonly signal?: AbortSignal | undefined
  readonly onSessionChange?: ((sessionId: string) => void) | undefined
}

function extractDeclaredCharset(headText: string): string | null {
  const xmlMatch = headText.match(/<\?xml\b[^>]*\bencoding\s*=\s*["']([^"']+)["']/i)
  if (xmlMatch?.[1]) {
    return xmlMatch[1].trim()
  }
  const metaCharsetMatch = headText.match(/<meta\b[^>]*\bcharset\s*=\s*["']?\s*([a-zA-Z0-9._-]+)/i)
  if (metaCharsetMatch?.[1]) {
    return metaCharsetMatch[1].trim()
  }
  const httpEquivMatch = headText.match(
    /<meta\b[^>]*http-equiv\s*=\s*["']?content-type["']?[^>]*content\s*=\s*["'][^"']*charset\s*=\s*([a-zA-Z0-9._-]+)/i
  )
  if (httpEquivMatch?.[1]) {
    return httpEquivMatch[1].trim()
  }
  return null
}

function tryDecodeWithLabel(bytes: Uint8Array, label: string, fatal: boolean): string | null {
  try {
    return new TextDecoder(label, { fatal }).decode(bytes)
  } catch {
    return null
  }
}

export function decodeEpubText(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer)
  if (bytes.length === 0) return ""

  // 1. BOM checks (UTF-8, UTF-16LE, UTF-16BE)
  if (
    bytes.length >= 3 &&
    bytes[0] === 0xef &&
    bytes[1] === 0xbb &&
    bytes[2] === 0xbf
  ) {
    return new TextDecoder("utf-8").decode(bytes)
  }
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return tryDecodeWithLabel(bytes, "utf-16le", false) ?? new TextDecoder("utf-8").decode(bytes)
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return tryDecodeWithLabel(bytes, "utf-16be", false) ?? new TextDecoder("utf-8").decode(bytes)
  }

  // 2. Sniff declared XML/HTML encoding in first 512 bytes (ASCII-compatible prefix)
  const headLen = Math.min(bytes.length, 512)
  const asciiPrefix = new TextDecoder("latin1").decode(bytes.subarray(0, headLen))
  const declared = extractDeclaredCharset(asciiPrefix)
  if (declared) {
    const lower = declared.toLowerCase()
    if (lower !== "utf-8" && lower !== "utf8") {
      const decoded = tryDecodeWithLabel(bytes, declared, false)
      if (decoded !== null) {
        return decoded
      }
    }
  }

  // 3. Strict UTF-8 first; on invalid byte sequences fall back through legacy CJK/Western encodings
  const strictUtf8 = tryDecodeWithLabel(bytes, "utf-8", true)
  if (strictUtf8 !== null) {
    return strictUtf8
  }

  for (const fallbackEncoding of ["gb18030", "shift_jis", "big5", "windows-1252"]) {
    const decoded = tryDecodeWithLabel(bytes, fallbackEncoding, true)
    if (decoded !== null) {
      return decoded
    }
  }

  return new TextDecoder("utf-8").decode(bytes)
}

export class EpubIpcLoader implements EpubLoader {
  readonly bookId: string
  readonly fileSize: number
  private readonly bridge: ReaderBridge
  private readonly manageSessionLifecycleInFrontend: boolean
  private readonly signal: AbortSignal | undefined
  private readonly onSessionChange: ((sessionId: string) => void) | undefined
  private currentSessionId: string
  private currentEntries: Readonly<Record<string, number>>
  private reopenPromise: Promise<string> | null = null
  private disposed = false

  private constructor(
    initial: OpenEpubResponse,
    options: EpubIpcLoaderOpenOptions
  ) {
    this.bookId = initial.bookId
    this.fileSize = initial.fileSize
    this.currentSessionId = initial.sessionId
    this.currentEntries = initial.entries
    this.bridge = options.bridge ?? readerBridge
    this.manageSessionLifecycleInFrontend =
      options.manageSessionLifecycleInFrontend ?? true
    this.signal = options.signal
    this.onSessionChange = options.onSessionChange
  }

  static async open(
    options: EpubIpcLoaderOpenOptions
  ): Promise<EpubIpcLoader> {
    const bridge = options.bridge ?? readerBridge
    const hasReattach =
      typeof options.reattachSessionId === "string" &&
      options.reattachSessionId.length > 0
    const opened = hasReattach
      ? await bridge.epubAttachOrOpen(
          options.bookId,
          options.reattachSessionId ?? null
        )
      : await bridge.epubOpen(options.bookId)
    return new EpubIpcLoader(opened, options)
  }

  get sessionId(): string {
    return this.currentSessionId
  }

  get entries(): Readonly<Record<string, number>> {
    return this.currentEntries
  }

  get isDisposed(): boolean {
    return this.disposed
  }

  getSize = (name: string): number => {
    return lookupEntrySize(name, this.currentEntries) ?? 0
  }

  loadBlob = async (name: string): Promise<Blob | null> => {
    if (this.disposed) {
      throw new Error("EPUB loader has been disposed")
    }
    const resolved = resolveArchiveEntryPath(name, this.currentEntries)
    if (resolved === null) {
      return null
    }
    const buffer = await this.readEntryWithSelfHeal(resolved)
    if (buffer === null) {
      return null
    }
    return new Blob([buffer], { type: inferEpubEntryMime(resolved) })
  }

  loadText = async (name: string): Promise<string | null> => {
    if (this.disposed) {
      throw new Error("EPUB loader has been disposed")
    }
    const resolved = resolveArchiveEntryPath(name, this.currentEntries)
    if (resolved === null) {
      return null
    }
    const buffer = await this.readEntryWithSelfHeal(resolved)
    if (buffer === null) {
      return null
    }
    return decodeEpubText(buffer)
  }

  sha1 = (str: string): Promise<Uint8Array> => {
    return sha1Bytes(str)
  }

  async close(
    keepBackendSession = false,
    options?: { readonly forceCloseUntracked?: boolean | undefined }
  ): Promise<void> {
    if (this.disposed) return
    this.disposed = true
    const sessionToClose = this.currentSessionId
    if (
      !keepBackendSession &&
      (this.manageSessionLifecycleInFrontend ||
        options?.forceCloseUntracked === true)
    ) {
      await this.bridge.epubClose(sessionToClose)
    }
  }

  private async readEntryWithSelfHeal(
    resolvedPath: string
  ): Promise<ArrayBuffer | null> {
    if (this.disposed || this.signal?.aborted) {
      throw new Error("EPUB loader has been disposed")
    }

    const attemptSessionId = this.currentSessionId
    try {
      return await this.bridge.epubReadEntry(attemptSessionId, resolvedPath)
    } catch (error) {
      if (isCommandErrorCode(error, READER_ERROR_CODES.ENTRY_NOT_FOUND)) {
        return null
      }
      if (!isCommandErrorCode(error, READER_ERROR_CODES.SESSION_NOT_FOUND)) {
        throw error
      }
      if (this.disposed || this.signal?.aborted) {
        throw error
      }

      const nextSessionId = await this.ensureReopened(attemptSessionId)
      if (this.disposed || this.signal?.aborted) {
        throw error
      }

      try {
        return await this.bridge.epubReadEntry(nextSessionId, resolvedPath)
      } catch (retryError) {
        if (
          isCommandErrorCode(retryError, READER_ERROR_CODES.ENTRY_NOT_FOUND)
        ) {
          return null
        }
        throw retryError
      }
    }
  }

  private async ensureReopened(staleSessionId: string): Promise<string> {
    if (this.currentSessionId !== staleSessionId) {
      return this.currentSessionId
    }
    if (this.reopenPromise !== null) {
      return this.reopenPromise
    }

    const pending = (async (): Promise<string> => {
      const reopened = await this.bridge.epubOpen(this.bookId)
      if (this.disposed || this.signal?.aborted) {
        // Freshly reopened session was never handed off via onSessionChange;
        // always close it so backend slots do not leak on desktop either.
        void this.bridge.epubClose(reopened.sessionId).catch(() => {})
        throw new Error("EPUB loader disposed during session reopen")
      }
      this.currentSessionId = reopened.sessionId
      this.currentEntries = reopened.entries
      this.onSessionChange?.(reopened.sessionId)
      return reopened.sessionId
    })().finally(() => {
      if (this.reopenPromise === pending) {
        this.reopenPromise = null
      }
    })

    this.reopenPromise = pending
    return pending
  }
}
