import {
  READER_ERROR_CODES,
  setPreviewReaderBridge,
  type OpenEpubResponse,
  type ReaderBridge,
  type ReaderWindowSnapshot,
  type ReaderWindowTabDto,
  type TabDropPreviewEvent,
} from "./reader-bridge"
import { CommandError } from "./tauri-bridge"
import type { BookNavigationData } from "@shiro/ui-shared/features/reader/contracts/engine"
import type {
  CustomFontMeta,
  ReaderProfile,
  ReaderProfilePatch,
  ReaderSettingsSnapshot,
  SystemFontFamily,
} from "@shiro/ui-shared/features/reader/contracts/settings"
import { getDefaultReaderProfile } from "@shiro/ui-shared/features/reader/settings/defaults"

const MAX_MOCK_OPEN_SESSIONS = 8

interface MockArchiveFile {
  readonly path: string
  readonly bytes: Uint8Array
}

function buildCrc32Table(): Uint32Array {
  const table = new Uint32Array(256)
  for (let i = 0; i < 256; i++) {
    let crc = i
    for (let j = 0; j < 8; j++) {
      crc = (crc & 1) !== 0 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1
    }
    table[i] = crc >>> 0
  }
  return table
}

const CRC32_TABLE = buildCrc32Table()

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff
  for (let i = 0; i < bytes.length; i++) {
    const byte = bytes[i] ?? 0
    crc = (CRC32_TABLE[(crc ^ byte) & 0xff] ?? 0) ^ (crc >>> 8)
  }
  return (crc ^ 0xffffffff) >>> 0
}

function concatUint8Arrays(chunks: readonly Uint8Array[]): Uint8Array {
  let totalLength = 0
  for (const chunk of chunks) {
    totalLength += chunk.byteLength
  }
  const output = new Uint8Array(totalLength)
  let offset = 0
  for (const chunk of chunks) {
    output.set(chunk, offset)
    offset += chunk.byteLength
  }
  return output
}

function buildStoredZip(files: readonly MockArchiveFile[]): Uint8Array {
  const encoder = new TextEncoder()
  const localChunks: Uint8Array[] = []
  const centralChunks: Uint8Array[] = []
  let localOffset = 0

  for (const file of files) {
    const nameBytes = encoder.encode(file.path)
    const checksum = crc32(file.bytes)
    const size = file.bytes.byteLength

    const localHeader = new Uint8Array(30 + nameBytes.byteLength)
    const lView = new DataView(localHeader.buffer)
    lView.setUint32(0, 0x04034b50, true)
    lView.setUint16(4, 20, true)
    lView.setUint16(6, 0x0800, true)
    lView.setUint16(8, 0, true) // Stored
    lView.setUint16(10, 0, true)
    lView.setUint16(12, 0, true)
    lView.setUint32(14, checksum, true)
    lView.setUint32(18, size, true)
    lView.setUint32(22, size, true)
    lView.setUint16(26, nameBytes.byteLength, true)
    lView.setUint16(28, 0, true)
    localHeader.set(nameBytes, 30)

    localChunks.push(localHeader, file.bytes)

    const centralHeader = new Uint8Array(46 + nameBytes.byteLength)
    const cView = new DataView(centralHeader.buffer)
    cView.setUint32(0, 0x02014b50, true)
    cView.setUint16(4, 20, true)
    cView.setUint16(6, 20, true)
    cView.setUint16(8, 0x0800, true)
    cView.setUint16(10, 0, true)
    cView.setUint16(12, 0, true)
    cView.setUint16(14, 0, true)
    cView.setUint32(16, checksum, true)
    cView.setUint32(20, size, true)
    cView.setUint32(24, size, true)
    cView.setUint16(28, nameBytes.byteLength, true)
    cView.setUint16(30, 0, true)
    cView.setUint16(32, 0, true)
    cView.setUint16(34, 0, true)
    cView.setUint16(36, 0, true)
    cView.setUint32(38, 0, true)
    cView.setUint32(42, localOffset, true)
    centralHeader.set(nameBytes, 46)

    centralChunks.push(centralHeader)
    localOffset += localHeader.byteLength + file.bytes.byteLength
  }

  const centralDirectory = concatUint8Arrays(centralChunks)
  const eocd = new Uint8Array(22)
  const eView = new DataView(eocd.buffer)
  eView.setUint32(0, 0x06054b50, true)
  eView.setUint16(4, 0, true)
  eView.setUint16(6, 0, true)
  eView.setUint16(8, files.length, true)
  eView.setUint16(10, files.length, true)
  eView.setUint32(12, centralDirectory.byteLength, true)
  eView.setUint32(16, localOffset, true)
  eView.setUint16(20, 0, true)

  return concatUint8Arrays([...localChunks, centralDirectory, eocd])
}

interface MockSampleEpub {
  readonly zipBytes: Uint8Array
  readonly entriesByPath: ReadonlyMap<string, Uint8Array>
  readonly entrySizes: Readonly<Record<string, number>>
  readonly navigation: BookNavigationData
}

function createSampleMockEpub(): MockSampleEpub {
  const encoder = new TextEncoder()
  const rawEntries: ReadonlyArray<readonly [string, string]> = [
    ["mimetype", "application/epub+zip"],
    [
      "META-INF/container.xml",
      `<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>`,
    ],
    [
      "OEBPS/styles.css",
      `body { font-family: serif; line-height: 1.7; margin: 0; }
h1 { font-size: 1.5em; margin-bottom: 0.75em; }
p { margin-bottom: 0.8em; }
.accent { font-style: italic; }`,
    ],
    [
      "OEBPS/content.opf",
      `<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:uuid:00000000-0000-4000-8000-000000000001</dc:identifier>
    <dc:title>The Book of Tea (Preview)</dc:title>
    <dc:creator>Okakura Kakuzo</dc:creator>
    <dc:language>zh-CN</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="css" href="styles.css" media-type="text/css"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch3" href="ch3.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
    <itemref idref="ch3"/>
  </spine>
</package>`,
    ],
    [
      "OEBPS/nav.xhtml",
      `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
  <head><title>Table of Contents</title></head>
  <body>
    <nav epub:type="toc" id="toc">
      <ol>
        <li><a href="ch1.xhtml">第一章 · The Cup of Humanity</a></li>
        <li><a href="ch2.xhtml">第二章 · The Schools of Tea</a></li>
        <li><a href="ch3.xhtml">第三章 · Taoism and Zennism</a></li>
      </ol>
    </nav>
  </body>
</html>`,
    ],
    [
      "OEBPS/toc.ncx",
      `<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:00000000-0000-4000-8000-000000000001"/></head>
  <docTitle><text>The Book of Tea (Preview)</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1"><navLabel><text>第一章 · The Cup of Humanity</text></navLabel><content src="ch1.xhtml"/></navPoint>
    <navPoint id="n2" playOrder="2"><navLabel><text>第二章 · The Schools of Tea</text></navLabel><content src="ch2.xhtml"/></navPoint>
    <navPoint id="n3" playOrder="3"><navLabel><text>第三章 · Taoism and Zennism</text></navLabel><content src="ch3.xhtml"/></navPoint>
  </navMap>
</ncx>`,
    ],
    [
      "OEBPS/ch1.xhtml",
      `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="zh-CN">
  <head>
    <title>第一章 · The Cup of Humanity</title>
    <link rel="stylesheet" type="text/css" href="styles.css"/>
  </head>
  <body>
    <h1 id="ch1-title">第一章 人文之杯 · The Cup of Humanity</h1>
    <p id="ch1-p1">茶最初是药用饮品，后来渐渐成为艺术与生活的仪式。</p>
    <p id="ch1-p2" class="accent">Teaism is a religion of the art of life, founded on the adoration of the beautiful among the sordid facts of everyday existence.</p>
    <ol>
      <li>煎茶之道在于清静与节制。</li>
      <li>点茶与抹茶讲究器物与心境之和。</li>
    </ol>
  </body>
</html>`,
    ],
    [
      "OEBPS/ch2.xhtml",
      `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="zh-CN">
  <head>
    <title>第二章 · The Schools of Tea</title>
    <link rel="stylesheet" type="text/css" href="styles.css"/>
  </head>
  <body>
    <h1 id="ch2-title">第二章 茶之流派 · The Schools of Tea</h1>
    <p id="ch2-p1">从唐代的饼茶煎饮，到宋代的点茶斗茶，再到明代的散茶冲泡，饮茶礼法随时代而变。</p>
    <p id="ch2-p2">Each epoch refined simplicity into a quiet discipline shared across studies and gardens.</p>
    <p><a href="#ch2-note">查看章末注释</a></p>
    <p id="ch2-note">注释：陆羽《茶经》成书于唐代，系统总结了制茶与烹茶技艺。</p>
  </body>
</html>`,
    ],
    [
      "OEBPS/ch3.xhtml",
      `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="zh-CN">
  <head>
    <title>第三章 · Taoism and Zennism</title>
    <link rel="stylesheet" type="text/css" href="styles.css"/>
  </head>
  <body>
    <h1 id="ch3-title">第三章 道与禅 · Taoism and Zennism</h1>
    <p id="ch3-p1">留白并非空无一物，而是让意义得以自由呼吸的空间。</p>
    <p id="ch3-p2">True beauty could be discovered only by one who mentally completed the incomplete.</p>
  </body>
</html>`,
    ],
  ]

  const archiveFiles: MockArchiveFile[] = rawEntries.map(([path, text]) => ({
    path,
    bytes: encoder.encode(text),
  }))

  const entriesByPath = new Map<string, Uint8Array>()
  const entrySizes: Record<string, number> = {}
  for (const file of archiveFiles) {
    entriesByPath.set(file.path, file.bytes)
    entrySizes[file.path] = file.bytes.byteLength
  }

  const navigation: BookNavigationData = {
    toc: [
      {
        parent: null,
        depth: 0,
        subtree_end: 0,
        title: "第一章 · The Cup of Humanity",
        locator: {
          kind: "epub",
          spine_index: 0,
          href: "OEBPS/ch1.xhtml",
          target: { kind: "start" },
        },
        unit_index: 0,
        unit_offset: 0,
        start_progression: 0,
        flags: 0,
      },
      {
        parent: null,
        depth: 0,
        subtree_end: 1,
        title: "第二章 · The Schools of Tea",
        locator: {
          kind: "epub",
          spine_index: 1,
          href: "OEBPS/ch2.xhtml",
          target: { kind: "start" },
        },
        unit_index: 1,
        unit_offset: 0,
        start_progression: 0.35,
        flags: 0,
      },
      {
        parent: null,
        depth: 0,
        subtree_end: 2,
        title: "第三章 · Taoism and Zennism",
        locator: {
          kind: "epub",
          spine_index: 2,
          href: "OEBPS/ch3.xhtml",
          target: { kind: "start" },
        },
        unit_index: 2,
        unit_offset: 0,
        start_progression: 0.72,
        flags: 0,
      },
    ],
    linear_units: [
      {
        index: 0,
        is_linear: true,
        weight: 350,
        cumulative_weight: 0,
        display_label: "1",
      },
      {
        index: 1,
        is_linear: true,
        weight: 370,
        cumulative_weight: 350,
        display_label: "2",
      },
      {
        index: 2,
        is_linear: true,
        weight: 280,
        cumulative_weight: 720,
        display_label: "3",
      },
    ],
    total_weight: 1000,
  }

  return {
    zipBytes: buildStoredZip(archiveFiles),
    entriesByPath,
    entrySizes,
    navigation,
  }
}

export function createDefaultMockReaderProfile(
  isMobile = false
): ReaderProfile {
  return getDefaultReaderProfile(isMobile)
}

function createReaderCommandError(code: number, key: string): CommandError {
  return new CommandError({
    operationId: crypto.randomUUID(),
    errorId: crypto.randomUUID(),
    code,
    key,
    itemId: null,
  })
}

function isPatchEmpty(patch: ReaderProfilePatch): boolean {
  const keysEmpty = (obj: object | undefined): boolean =>
    obj === undefined ||
    Object.values(obj).every((value) => value === undefined)

  return (
    keysEmpty(patch.appearance) &&
    keysEmpty(patch.typography) &&
    keysEmpty(patch.layout) &&
    keysEmpty(patch.behavior)
  )
}

export interface MockReaderBridgeOptions {
  readonly isMobile?: boolean | undefined
}

export function createMockReaderBridge(
  options?: MockReaderBridgeOptions
): ReaderBridge {
  const isMobile = options?.isMobile ?? false
  const sample = createSampleMockEpub()
  const sessions = new Map<
    string,
    { bookId: string; lastAccessed: number }
  >()
  let accessClock = 1

  let globalProfile = createDefaultMockReaderProfile(isMobile)
  const bookOverrides: Record<string, ReaderProfilePatch> = {}
  const customFonts: CustomFontMeta[] = []
  const customFontBuffers = new Map<string, ArrayBuffer>()
  const settingsListeners = new Set<
    (snapshot: ReaderSettingsSnapshot) => void
  >()
  const windowStateListeners = new Set<
    (snapshot: ReaderWindowSnapshot) => void
  >()
  const dropPreviewListeners = new Set<
    (preview: TabDropPreviewEvent) => void
  >()

  interface MockWindowEntry {
    readonly windowId: string
    readonly windowLabel: string
    tabs: ReaderWindowTabDto[]
    activeTabId: string | null
    tick: number
  }
  const mockWindows = new Map<string, MockWindowEntry>()
  let windowTick = 1

  const buildWindowSnapshot = (win: MockWindowEntry): ReaderWindowSnapshot => {
    const otherWindows = Array.from(mockWindows.values())
      .filter((other) => other.windowId !== win.windowId)
      .map((other) => {
        const active =
          other.tabs.find((t) => t.tabId === other.activeTabId) ??
          other.tabs[0]
        return {
          windowId: other.windowId,
          activeTitle: active?.title ?? "",
          tabCount: other.tabs.length,
        }
      })
    return {
      windowId: win.windowId,
      windowLabel: win.windowLabel,
      tabs: win.tabs.map((t) => ({ ...t })),
      activeTabId: win.activeTabId,
      otherWindows,
      canCrossWindowPointerHitTest: !isMobile,
    }
  }

  const emitAllWindowSnapshots = () => {
    for (const win of mockWindows.values()) {
      const snap = buildWindowSnapshot(win)
      windowStateListeners.forEach((listener) => listener(snap))
    }
  }

  const buildSnapshot = (): ReaderSettingsSnapshot => ({
    global: structuredClone(globalProfile),
    bookOverrides: structuredClone(bookOverrides),
    customFonts: customFonts.map((font) => ({ ...font })),
    fontUrlBase: "shiro-font://localhost/font/",
    canEnumerateSystemFonts: !isMobile,
  })

  const emitSettings = (): ReaderSettingsSnapshot => {
    const snap = buildSnapshot()
    settingsListeners.forEach((listener) => listener(snap))
    return snap
  }

  const openSession = (bookId: string): OpenEpubResponse => {
    if (sessions.size >= MAX_MOCK_OPEN_SESSIONS) {
      let oldestId: string | null = null
      let oldestTick = Number.POSITIVE_INFINITY
      for (const [id, entry] of sessions.entries()) {
        if (entry.lastAccessed < oldestTick) {
          oldestTick = entry.lastAccessed
          oldestId = id
        }
      }
      if (oldestId !== null) {
        sessions.delete(oldestId)
      }
    }
    const sessionId = crypto.randomUUID()
    sessions.set(sessionId, { bookId, lastAccessed: accessClock++ })
    return {
      sessionId,
      bookId,
      fileSize: sample.zipBytes.byteLength,
      entries: { ...sample.entrySizes },
    }
  }

  return {
    async epubOpen(bookId) {
      return openSession(bookId)
    },

    async epubAttachOrOpen(bookId, sessionId) {
      if (sessionId !== null) {
        const existing = sessions.get(sessionId)
        if (existing && existing.bookId === bookId) {
          existing.lastAccessed = accessClock++
          return {
            sessionId,
            bookId,
            fileSize: sample.zipBytes.byteLength,
            entries: { ...sample.entrySizes },
          }
        }
      }
      return openSession(bookId)
    },

    async epubReadEntry(sessionId, path) {
      const session = sessions.get(sessionId)
      if (!session) {
        throw createReaderCommandError(
          READER_ERROR_CODES.SESSION_NOT_FOUND,
          "client.reader.session_not_found"
        )
      }
      session.lastAccessed = accessClock++
      const bytes = sample.entriesByPath.get(path)
      if (!bytes) {
        throw createReaderCommandError(
          READER_ERROR_CODES.ENTRY_NOT_FOUND,
          "client.reader.entry_not_found"
        )
      }
      return bytes.slice().buffer
    },

    async epubClose(sessionId) {
      sessions.delete(sessionId)
    },

    async bookNavigation() {
      return structuredClone(sample.navigation)
    },

    async readerGetSettings() {
      return buildSnapshot()
    },

    async readerUpdateGlobalSettings(settings) {
      globalProfile = structuredClone(settings)
      return emitSettings()
    },

    async readerSetBookOverride(bookHash, patch) {
      const normalized = bookHash.trim().toLowerCase()
      if (isPatchEmpty(patch)) {
        delete bookOverrides[normalized]
      } else {
        bookOverrides[normalized] = structuredClone(patch)
      }
      return emitSettings()
    },

    async readerResetBookOverride(bookHash) {
      delete bookOverrides[bookHash.trim().toLowerCase()]
      return emitSettings()
    },

    async readerListSystemFonts(): Promise<SystemFontFamily[]> {
      if (isMobile) return []
      return [
        { family: "Inter" },
        { family: "Noto Sans CJK SC" },
        { family: "Noto Serif CJK SC" },
      ]
    },

    async readerPickAndImportFont() {
      return buildSnapshot()
    },

    async readerDeleteCustomFont(fontId) {
      const normalized = fontId.trim().toLowerCase()
      const idx = customFonts.findIndex((f) => f.fontId === normalized)
      if (idx < 0) {
        throw createReaderCommandError(
          READER_ERROR_CODES.FONT_NOT_FOUND,
          "client.reader.font_not_found"
        )
      }
      customFonts.splice(idx, 1)
      customFontBuffers.delete(normalized)
      if (
        globalProfile.typography.font.kind === "custom" &&
        globalProfile.typography.font.fontId === normalized
      ) {
        globalProfile = {
          ...globalProfile,
          typography: {
            ...globalProfile.typography,
            font: { kind: "publisher" },
          },
        }
      }
      for (const [bookHash, patch] of Object.entries(bookOverrides)) {
        if (
          patch.typography?.font?.kind === "custom" &&
          patch.typography.font.fontId === normalized
        ) {
          bookOverrides[bookHash] = {
            ...patch,
            typography: {
              ...patch.typography,
              font: { kind: "publisher" },
            },
          }
        }
      }
      return emitSettings()
    },

    async readerReadCustomFont(fontId) {
      const bytes = customFontBuffers.get(fontId.trim().toLowerCase())
      if (!bytes) {
        throw createReaderCommandError(
          READER_ERROR_CODES.FONT_NOT_FOUND,
          "client.reader.font_not_found"
        )
      }
      return bytes.slice(0)
    },

    async listenReaderSettings(handler) {
      settingsListeners.add(handler)
      return () => {
        settingsListeners.delete(handler)
      }
    },

    async readerOpenBook(req) {
      if (req.format && req.format.trim().toUpperCase() !== "EPUB") {
        throw createReaderCommandError(
          READER_ERROR_CODES.UNSUPPORTED_FORMAT,
          "client.reader.unsupported_format"
        )
      }
      let targetWin: MockWindowEntry | undefined
      if (!req.newWindow && mockWindows.size > 0) {
        targetWin = Array.from(mockWindows.values()).sort(
          (a, b) => b.tick - a.tick
        )[0]
      }
      if (!targetWin) {
        const windowId = crypto.randomUUID()
        targetWin = {
          windowId,
          windowLabel: `reader-${windowId}`,
          tabs: [],
          activeTabId: null,
          tick: windowTick++,
        }
        mockWindows.set(windowId, targetWin)
      } else {
        targetWin.tick = windowTick++
      }

      const existing = targetWin.tabs.find((t) => t.bookId === req.bookId)
      if (existing) {
        targetWin.activeTabId = existing.tabId
      } else {
        const newTab: ReaderWindowTabDto = {
          tabId: `tab-${crypto.randomUUID()}`,
          bookId: req.bookId,
          bookHash: req.bookHash ?? `hash-${req.bookId}`,
          title: req.title ?? "Mock Book",
          format: "EPUB",
          sessionId: null,
          currentLocator: null,
          fraction: 0,
          translationMode: "original",
          targetLanguage: null,
        }
        targetWin.tabs = [...targetWin.tabs, newTab]
        targetWin.activeTabId = newTab.tabId
      }
      emitAllWindowSnapshots()
      return buildWindowSnapshot(targetWin)
    },

    async readerGetWindowSnapshot(windowId) {
      const existing = mockWindows.get(windowId)
      if (existing) {
        return buildWindowSnapshot(existing)
      }
      const fallback: MockWindowEntry = {
        windowId,
        windowLabel: `reader-${windowId}`,
        tabs: [],
        activeTabId: null,
        tick: windowTick++,
      }
      mockWindows.set(windowId, fallback)
      return buildWindowSnapshot(fallback)
    },

    async readerActivateTab(windowId, tabId) {
      const win = mockWindows.get(windowId)
      if (!win) {
        throw createReaderCommandError(9002, "common.internal_server_error")
      }
      if (win.tabs.some((t) => t.tabId === tabId)) {
        win.activeTabId = tabId
      }
      emitAllWindowSnapshots()
      return buildWindowSnapshot(win)
    },

    async readerReorderTabs(windowId, tabIds) {
      const win = mockWindows.get(windowId)
      if (!win) {
        throw createReaderCommandError(9002, "common.internal_server_error")
      }
      const byId = new Map(win.tabs.map((t) => [t.tabId, t]))
      const reordered: ReaderWindowTabDto[] = []
      for (const id of tabIds) {
        const found = byId.get(id)
        if (found) {
          reordered.push(found)
          byId.delete(id)
        }
      }
      for (const leftover of byId.values()) reordered.push(leftover)
      win.tabs = reordered
      emitAllWindowSnapshots()
      return buildWindowSnapshot(win)
    },

    async readerCloseTab(windowId, tabId) {
      const win = mockWindows.get(windowId)
      if (!win) {
        return {
          windowId,
          windowLabel: `reader-${windowId}`,
          tabs: [],
          activeTabId: null,
          otherWindows: [],
          canCrossWindowPointerHitTest: !isMobile,
        }
      }
      const idx = win.tabs.findIndex((t) => t.tabId === tabId)
      if (idx >= 0) {
        const removed = win.tabs[idx]!
        if (removed.sessionId) sessions.delete(removed.sessionId)
        win.tabs = win.tabs.filter((t) => t.tabId !== tabId)
        if (win.tabs.length === 0) {
          mockWindows.delete(windowId)
          emitAllWindowSnapshots()
          return {
            windowId,
            windowLabel: win.windowLabel,
            tabs: [],
            activeTabId: null,
            otherWindows: [],
            canCrossWindowPointerHitTest: !isMobile,
          }
        }
        if (win.activeTabId === tabId) {
          const next = win.tabs[Math.min(idx, win.tabs.length - 1)]
          win.activeTabId = next?.tabId ?? null
        }
      }
      emitAllWindowSnapshots()
      return buildWindowSnapshot(win)
    },

    async readerUpdateTabState(req) {
      const win = mockWindows.get(req.windowId)
      if (!win) return
      win.tabs = win.tabs.map((tab) =>
        tab.tabId !== req.tabId
          ? tab
          : {
              ...tab,
              sessionId: req.sessionId ?? tab.sessionId,
              currentLocator: req.currentLocator ?? tab.currentLocator,
              fraction: req.fraction ?? tab.fraction,
              translationMode: req.translationMode ?? tab.translationMode,
              targetLanguage:
                req.targetLanguage === undefined
                  ? tab.targetLanguage
                  : req.targetLanguage === null
                    ? null
                    : req.targetLanguage.trim() || null,
            }
      )
    },

    async readerReportTabStripMetrics() {},

    async readerMoveTabToWindow(
      sourceWindowId,
      tabId,
      targetWindowId,
      targetIndex
    ) {
      const src = mockWindows.get(sourceWindowId)
      const dst = mockWindows.get(targetWindowId)
      if (!src || !dst) {
        throw createReaderCommandError(9002, "common.internal_server_error")
      }
      const idx = src.tabs.findIndex((t) => t.tabId === tabId)
      if (idx < 0) {
        throw createReaderCommandError(9002, "common.internal_server_error")
      }
      const moved = src.tabs[idx]!
      src.tabs = src.tabs.filter((t) => t.tabId !== tabId)
      if (src.tabs.length === 0) {
        mockWindows.delete(sourceWindowId)
      } else if (src.activeTabId === tabId) {
        src.activeTabId =
          src.tabs[Math.min(idx, src.tabs.length - 1)]?.tabId ?? null
      }
      const insertAt = Math.min(
        targetIndex ?? dst.tabs.length,
        dst.tabs.length
      )
      const nextDstTabs = [...dst.tabs]
      nextDstTabs.splice(insertAt, 0, moved)
      dst.tabs = nextDstTabs
      dst.activeTabId = moved.tabId
      dst.tick = windowTick++
      emitAllWindowSnapshots()
      return buildWindowSnapshot(dst)
    },

    async readerTearOffTab(sourceWindowId, tabId) {
      const src = mockWindows.get(sourceWindowId)
      if (!src) {
        throw createReaderCommandError(9002, "common.internal_server_error")
      }
      if (src.tabs.length <= 1) {
        return buildWindowSnapshot(src)
      }
      const idx = src.tabs.findIndex((t) => t.tabId === tabId)
      if (idx < 0) {
        throw createReaderCommandError(9002, "common.internal_server_error")
      }
      const moved = src.tabs[idx]!
      src.tabs = src.tabs.filter((t) => t.tabId !== tabId)
      if (src.activeTabId === tabId) {
        src.activeTabId =
          src.tabs[Math.min(idx, src.tabs.length - 1)]?.tabId ?? null
      }
      const newWindowId = crypto.randomUUID()
      const created: MockWindowEntry = {
        windowId: newWindowId,
        windowLabel: `reader-${newWindowId}`,
        tabs: [moved],
        activeTabId: moved.tabId,
        tick: windowTick++,
      }
      mockWindows.set(newWindowId, created)
      emitAllWindowSnapshots()
      return buildWindowSnapshot(created)
    },

    async readerDragStart() {},
    async readerDragOver() {},
    async readerDragCommit() {},
    async readerDragCancel() {},
    async readerOpenExternalUrl(url) {
      if (typeof window !== "undefined" && typeof window.open === "function") {
        window.open(url, "_blank", "noopener,noreferrer")
      }
    },

    async listenReaderWindowState(handler) {
      windowStateListeners.add(handler)
      return () => {
        windowStateListeners.delete(handler)
      }
    },

    async listenReaderTabDropPreview(handler) {
      dropPreviewListeners.add(handler)
      return () => {
        dropPreviewListeners.delete(handler)
      }
    },
  }
}

export function enableReaderPreview(options?: MockReaderBridgeOptions): void {
  setPreviewReaderBridge(createMockReaderBridge(options))
}
