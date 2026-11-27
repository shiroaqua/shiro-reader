import { describe, expect, it, vi } from "vitest"
import {
  READER_ERROR_CODES,
  type ReaderBridge,
} from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import type { OpenDocumentParams } from "@shiro/ui-shared/features/reader/contracts/engine"
import {
  ReaderEngineRegistry,
  readerEngineRegistry,
} from "@shiro/ui-shared/features/reader/engine/engine-registry"
import {
  EpubIpcLoader,
  sha1Bytes,
  sha1PureJs,
} from "@shiro/ui-shared/features/reader/engine/epub/epub-ipc-loader"
import { UnsupportedFormatError } from "@shiro/ui-shared/features/reader/engine/unsupported-pdf-engine"

function makeCommandError(code: number, key: string): CommandError {
  return new CommandError({
    operationId: "op-test",
    errorId: "err-test",
    code,
    key,
    itemId: null,
  })
}

function bytesToHex(bytes: Uint8Array): string {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("")
}

describe("EpubIpcLoader", () => {
  it("loads text and blobs with exact MIME, returns null on missing optional XML without IPC, and computes pure-JS SHA-1", async () => {
    const mockBridge = createMockReaderBridge()
    const readSpy = vi.spyOn(mockBridge, "epubReadEntry")

    const loader = await EpubIpcLoader.open({
      bookId: "book-1",
      bridge: mockBridge,
    })

    expect(loader.getSize("./META-INF/container.xml")).toBeGreaterThan(0)
    expect(loader.getSize("META-INF/encryption.xml")).toBe(0)

    // Missing optional XML files must return null with ZERO IPC roundtrips.
    await expect(loader.loadText("META-INF/encryption.xml")).resolves.toBeNull()
    await expect(
      loader.loadBlob("META-INF/com.apple.ibooks.display-options.xml")
    ).resolves.toBeNull()
    expect(readSpy).not.toHaveBeenCalled()

    const opfText = await loader.loadText("OEBPS/content.opf")
    expect(opfText).toContain("<package")
    const ch1Blob = await loader.loadBlob("OEBPS/ch1.xhtml#ch1-title")
    expect(ch1Blob).not.toBeNull()
    expect(ch1Blob?.type).toBe("application/xhtml+xml")

    // Verify FIPS 180-4 SHA-1 fallback works without crypto.subtle.
    expect(
      bytesToHex(sha1PureJs(new TextEncoder().encode("abc")))
    ).toBe("a9993e364706816aba3e25717850c26c9cd0d89d")
    expect(bytesToHex(sha1PureJs(new Uint8Array(0)))).toBe(
      "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    )
    const subtleSpy = vi
      .spyOn(globalThis.crypto, "subtle", "get")
      .mockReturnValue(undefined as unknown as SubtleCrypto)
    const fallbackDigest = await sha1Bytes("abc")
    subtleSpy.mockRestore()
    expect(bytesToHex(fallbackDigest)).toBe(
      "a9993e364706816aba3e25717850c26c9cd0d89d"
    )
  })

  it("converges IPC 3102 missing entry to null", async () => {
    const baseBridge = createMockReaderBridge()
    const bridge: ReaderBridge = {
      ...baseBridge,
      async epubReadEntry() {
        throw makeCommandError(
          READER_ERROR_CODES.ENTRY_NOT_FOUND,
          "client.reader.entry_not_found"
        )
      },
    }

    const loader = await EpubIpcLoader.open({ bookId: "book-1", bridge })
    await expect(loader.loadBlob("OEBPS/ch1.xhtml")).resolves.toBeNull()
  })

  it("performs single-flight reopen on concurrent 3101 session eviction and updates sessionId once", async () => {
    const baseBridge = createMockReaderBridge()
    let openCalls = 0
    let activeSession = "sess-initial"
    const bridge: ReaderBridge = {
      ...baseBridge,
      async epubOpen(bookId) {
        openCalls += 1
        const res = await baseBridge.epubOpen(bookId)
        activeSession = `sess-${openCalls}`
        return {
          ...res,
          sessionId: activeSession,
        }
      },
      async epubReadEntry(sessionId, path) {
        if (sessionId === "sess-1") {
          throw makeCommandError(
            READER_ERROR_CODES.SESSION_NOT_FOUND,
            "client.reader.session_not_found"
          )
        }
        return new TextEncoder().encode(`content:${sessionId}:${path}`).buffer
      },
    }

    const onSessionChange = vi.fn()
    const loader = await EpubIpcLoader.open({
      bookId: "book-1",
      bridge,
      onSessionChange,
    })
    expect(openCalls).toBe(1)
    expect(loader.sessionId).toBe("sess-1")

    const entriesToRead = Array.from({ length: 8 }, () => "OEBPS/ch1.xhtml")
    const results = await Promise.all(
      entriesToRead.map((entry) => loader.loadText(entry))
    )

    expect(openCalls).toBe(2)
    expect(onSessionChange).toHaveBeenCalledTimes(1)
    expect(onSessionChange).toHaveBeenCalledWith("sess-2")
    expect(loader.sessionId).toBe("sess-2")
    for (const res of results) {
      expect(res).toBe("content:sess-2:OEBPS/ch1.xhtml")
    }
  })

  it("does not loop when 3101 persists after reopen and never reopens after dispose", async () => {
    const baseBridge = createMockReaderBridge()
    let openCalls = 0
    let rejectWith3101 = false
    const gate = { resolve: undefined as (() => void) | undefined }

    const bridge: ReaderBridge = {
      ...baseBridge,
      async epubOpen(bookId) {
        openCalls += 1
        return baseBridge.epubOpen(bookId)
      },
      async epubReadEntry() {
        if (rejectWith3101) {
          throw makeCommandError(
            READER_ERROR_CODES.SESSION_NOT_FOUND,
            "client.reader.session_not_found"
          )
        }
        await new Promise<void>((resolve) => {
          gate.resolve = resolve
        })
        throw makeCommandError(
          READER_ERROR_CODES.SESSION_NOT_FOUND,
          "client.reader.session_not_found"
        )
      },
    }

    // 1. Persistent 3101 after single reopen throws without looping.
    rejectWith3101 = true
    const loader1 = await EpubIpcLoader.open({ bookId: "book-1", bridge })
    await expect(loader1.loadText("OEBPS/ch1.xhtml")).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof CommandError &&
        err.payload.code === READER_ERROR_CODES.SESSION_NOT_FOUND
    )
    expect(openCalls).toBe(2)

    // 2. Disposed mid-flight prevents zombie reopen.
    rejectWith3101 = false
    const loader2 = await EpubIpcLoader.open({ bookId: "book-2", bridge })
    const opensBeforeFlight = openCalls
    const inFlight = loader2.loadText("OEBPS/ch1.xhtml")
    await Promise.resolve()
    await loader2.close()
    gate.resolve?.()
    await expect(inFlight).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof CommandError &&
        err.payload.code === READER_ERROR_CODES.SESSION_NOT_FOUND
    )
    expect(openCalls).toBe(opensBeforeFlight)
  })

  it("supports reattachSessionId and single-owner close lifecycle", async () => {
    const baseBridge = createMockReaderBridge()
    const attachSpy = vi.spyOn(baseBridge, "epubAttachOrOpen")
    const closeSpy = vi.spyOn(baseBridge, "epubClose")

    const loader = await EpubIpcLoader.open({
      bookId: "book-1",
      reattachSessionId: "existing-sess",
      manageSessionLifecycleInFrontend: false,
      bridge: baseBridge,
    })
    expect(attachSpy).toHaveBeenCalledWith("book-1", "existing-sess")
    await loader.close(false)
    expect(closeSpy).not.toHaveBeenCalled()

    const localLoader = await EpubIpcLoader.open({
      bookId: "book-1",
      bridge: baseBridge,
    })
    await localLoader.close(true)
    expect(closeSpy).not.toHaveBeenCalled()

    const closingLoader = await EpubIpcLoader.open({
      bookId: "book-1",
      bridge: baseBridge,
    })
    await closingLoader.close(false)
    await closingLoader.close(false)
    expect(closeSpy).toHaveBeenCalledTimes(1)
  })

  it("closes freshly reopened session when disposed mid-reopen even with desktop coordinator ownership", async () => {
    const baseBridge = createMockReaderBridge()
    let resolveReopen!: (res: Awaited<ReturnType<typeof baseBridge.epubOpen>>) => void
    const closeSpy = vi.fn(async () => {})
    let openCount = 0
    const bridge = {
      ...baseBridge,
      epubOpen: async (bookId: string) => {
        openCount += 1
        if (openCount === 1) {
          return {
            sessionId: "sess-initial",
            bookId,
            fileSize: 512,
            entries: { "OEBPS/ch1.xhtml": 10 },
          }
        }
        return await new Promise<Awaited<ReturnType<typeof baseBridge.epubOpen>>>(
          (resolve) => {
            resolveReopen = resolve
          }
        )
      },
      epubReadEntry: async () => {
        throw makeCommandError(
          READER_ERROR_CODES.SESSION_NOT_FOUND,
          "client.reader.session_not_found"
        )
      },
      epubClose: closeSpy,
    }

    const loader = await EpubIpcLoader.open({
      bookId: "book-1",
      manageSessionLifecycleInFrontend: false,
      bridge,
    })
    const pendingRead = loader.loadText("OEBPS/ch1.xhtml")
    await Promise.resolve()
    await Promise.resolve()

    await loader.close(true)
    resolveReopen({
      sessionId: "sess-untracked-reopen",
      bookId: "book-1",
      fileSize: 512,
      entries: { "OEBPS/ch1.xhtml": 10 },
    })

    await expect(pendingRead).rejects.toThrow("disposed")
    expect(closeSpy).toHaveBeenCalledWith("sess-untracked-reopen")
  })
})

describe("ReaderEngineRegistry & UnsupportedPdfEngine", () => {
  it("blocks PDF and unknown book formats with UnsupportedFormatError (3103)", async () => {
    const dummyParams = {} as OpenDocumentParams
    expect(readerEngineRegistry.has("PDF")).toBe(true)

    await expect(
      readerEngineRegistry.open("pdf", dummyParams)
    ).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof UnsupportedFormatError &&
        err.code === READER_ERROR_CODES.UNSUPPORTED_FORMAT &&
        err.format === "PDF"
    )

    const emptyRegistry = new ReaderEngineRegistry()
    await expect(emptyRegistry.open("epub", dummyParams)).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof UnsupportedFormatError &&
        err.code === READER_ERROR_CODES.UNSUPPORTED_FORMAT &&
        err.format === "EPUB"
    )
    await expect(emptyRegistry.open("mobi", dummyParams)).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof UnsupportedFormatError &&
        err.code === READER_ERROR_CODES.UNSUPPORTED_FORMAT &&
        err.format === "UNKNOWN"
    )
  })
})
