import { describe, expect, it, vi } from "vitest"
import {
  READER_ERROR_CODES,
  getCustomFontProtocolUrl,
  toArrayBuffer,
} from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import { normalizeBookFormat } from "@shiro/ui-shared/features/reader/contracts/engine"

describe("reader-bridge helpers & mock bridge", () => {
  it("normalizes ArrayBuffer and Uint8Array slices into standalone ArrayBuffers", () => {
    const raw = new Uint8Array([10, 20, 30, 40, 50])
    const view = raw.subarray(1, 4)
    const buf = toArrayBuffer(view)
    expect(buf).toBeInstanceOf(ArrayBuffer)
    expect([...new Uint8Array(buf)]).toEqual([20, 30, 40])
    expect(getCustomFontProtocolUrl("shiro-font://localhost/font", "abc123")).toBe(
      "shiro-font://localhost/font/abc123"
    )
    expect(normalizeBookFormat("epub")).toBe("EPUB")
    expect(normalizeBookFormat(" PDF ")).toBe("PDF")
    expect(normalizeBookFormat("txt")).toBe("UNKNOWN")
  })

  it("opens mock EPUB, decodes container.xml, reattaches live session, and maps 3101/3102 errors", async () => {
    const bridge = createMockReaderBridge()
    const opened = await bridge.epubOpen("book-1")
    expect(opened.sessionId.length).toBeGreaterThan(0)
    expect(opened.entries["META-INF/container.xml"]).toBeGreaterThan(0)
    expect(opened.entries["OEBPS/content.opf"]).toBeGreaterThan(0)
    expect(opened.entries["OEBPS/ch1.xhtml"]).toBeGreaterThan(0)
    expect(opened.entries["OEBPS/ch2.xhtml"]).toBeGreaterThan(0)
    expect(opened.entries["OEBPS/ch3.xhtml"]).toBeGreaterThan(0)

    const containerBuf = await bridge.epubReadEntry(
      opened.sessionId,
      "META-INF/container.xml"
    )
    const containerXml = new TextDecoder().decode(containerBuf)
    expect(containerXml).toContain("rootfile")
    expect(containerXml).toContain("OEBPS/content.opf")

    const reattached = await bridge.epubAttachOrOpen("book-1", opened.sessionId)
    expect(reattached.sessionId).toBe(opened.sessionId)

    const mismatch = await bridge.epubAttachOrOpen("book-2", opened.sessionId)
    expect(mismatch.sessionId).not.toBe(opened.sessionId)

    await expect(
      bridge.epubReadEntry(opened.sessionId, "META-INF/encryption.xml")
    ).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof CommandError &&
        err.payload.code === READER_ERROR_CODES.ENTRY_NOT_FOUND
    )

    await bridge.epubClose(opened.sessionId)
    await expect(
      bridge.epubReadEntry(opened.sessionId, "mimetype")
    ).rejects.toSatisfy(
      (err: unknown) =>
        err instanceof CommandError &&
        err.payload.code === READER_ERROR_CODES.SESSION_NOT_FOUND
    )
  })

  it("provides mock navigation and notifies settings listeners on global/book override changes", async () => {
    const bridge = createMockReaderBridge()
    const nav = await bridge.bookNavigation("book-1")
    expect(nav.toc).toHaveLength(3)
    expect(nav.linear_units).toHaveLength(3)
    expect(nav.linear_units.map((u) => u.cumulative_weight)).toEqual([0, 350, 720])
    expect(nav.total_weight).toBe(1000)

    const listener = vi.fn()
    const unlisten = await bridge.listenReaderSettings(listener)

    const initial = await bridge.readerGetSettings()
    expect(initial.global.typography.fontSizePx).toBe(18)

    const updatedGlobal = await bridge.readerUpdateGlobalSettings({
      ...initial.global,
      typography: {
        ...initial.global.typography,
        fontSizePx: 22,
      },
    })
    expect(updatedGlobal.global.typography.fontSizePx).toBe(22)
    expect(listener).toHaveBeenCalledTimes(1)

    const bookHash = "AB".repeat(32)
    const withOverride = await bridge.readerSetBookOverride(bookHash, {
      typography: { lineHeight: 2.0 },
    })
    expect(
      withOverride.bookOverrides[bookHash.toLowerCase()]?.typography?.lineHeight
    ).toBe(2.0)

    const reset = await bridge.readerResetBookOverride(bookHash)
    expect(reset.bookOverrides[bookHash.toLowerCase()]).toBeUndefined()
    unlisten()
  })
})
