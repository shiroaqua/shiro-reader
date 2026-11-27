import { describe, expect, it, vi } from "vitest"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import type {
  BookNavigationData,
  NavigationLocator,
  RelocationSnapshot,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import { TOC_FLAG_EXTERNAL_LINK } from "@shiro/ui-shared/features/reader/contracts/engine"
import type { CompiledReaderStyle } from "@shiro/ui-shared/features/reader/contracts/settings"
import {
  collapseUnicodeWhitespace,
  computeActiveTocIndex,
  computePageStats,
  FoliateEpubEngine,
  flattenFoliateToc,
} from "@shiro/ui-shared/features/reader/engine/epub/foliate-epub-engine"
import { injectSyntheticTranslation } from "@shiro/ui-shared/features/reader/engine/epub/xhtml-dom"

const DEFAULT_TEST_STYLE: CompiledReaderStyle = {
  surface: "paper_light",
  isDarkSurface: false,
  surfaceBackgroundHex: "#fcfcfa",
  viewportCssVars: {},
  viewportPaddingInlinePx: 0,
  paginatorAttributes: {
    flow: "paginated",
    margin: "36px",
    gap: "6%",
    maxInlineSize: "760px",
    maxColumnCount: "2",
    animated: true,
  },
  showPageNumber: true,
  seamlessScroll: false,
  chapterCss: "",
  customFontFace: null,
  usesFontFaceFallback: false,
}

function createStrictChapterDoc(): Document {
  const xhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <body>
    <p id="p-1">First original paragraph.</p>
    <p id="p-2">Second original paragraph.</p>
  </body>
</html>`
  return new DOMParser().parseFromString(xhtml, "application/xhtml+xml")
}

class FakeRenderer extends EventTarget {
  page = 2
  pages = 6
  scrolled = false
  size = 0
  start = 0
  end = 0
  viewSize = 0
  attrs = new Map<string, string>()
  styles: string | [string, string] | null = null
  lastGoToTarget: {
    index: number
    anchor?: ((doc: Document) => Range | Element | number | null) | undefined
  } | null = null
  docs: Array<{ index?: number | undefined; doc: Document }> = []

  getAttribute(name: string): string | null {
    return this.attrs.get(name) ?? null
  }
  setAttribute(name: string, value: string): void {
    this.attrs.set(name, value)
  }
  removeAttribute(name: string): void {
    this.attrs.delete(name)
  }
  hasAttribute(name: string): boolean {
    return this.attrs.has(name)
  }
  setStyles(styles: string | [string, string]): void {
    this.styles = styles
  }
  render(): void {}
  async goTo(target: {
    index: number
    anchor?: ((doc: Document) => Range | Element | number | null) | undefined
  }): Promise<void> {
    this.lastGoToTarget = target
  }
  getContents() {
    return this.docs
  }
}

interface FakeViewHandle extends HTMLElement {
  renderer: FakeRenderer
  book: unknown
  closed: boolean
  removed: boolean
  initOptions: unknown
  goLeftCalls: number
  goRightCalls: number
  open(book: unknown): Promise<void>
  init(options: unknown): Promise<void>
  close(): void
  goLeft(): Promise<void>
  goRight(): Promise<void>
  prev(): Promise<void>
  next(): Promise<void>
  goTo(): Promise<void>
  goToFraction(): Promise<void>
  getCFI(): string
  resolveCFI(): { index: number; anchor: () => null }
}

function createFakeView(): FakeViewHandle {
  const el = document.createElement("div") as unknown as FakeViewHandle
  el.renderer = new FakeRenderer()
  el.book = null
  el.closed = false
  el.removed = false
  el.initOptions = null
  el.goLeftCalls = 0
  el.goRightCalls = 0

  el.open = async (book: unknown) => {
    el.book = book
  }
  el.init = async (options: unknown) => {
    el.initOptions = options
  }
  el.close = () => {
    el.closed = true
  }
  const origRemove = el.remove.bind(el)
  el.remove = () => {
    el.removed = true
    origRemove()
  }
  el.goLeft = async () => {
    el.goLeftCalls += 1
  }
  el.goRight = async () => {
    el.goRightCalls += 1
  }
  el.prev = async () => {}
  el.next = async () => {}
  el.goTo = async () => {}
  el.goToFraction = async () => {}
  el.getCFI = () => "epubcfi(/6/4!/4/999)"
  el.resolveCFI = () => ({ index: 1, anchor: () => null })

  return el
}

class FakeViewCtor {}

describe("foliate-epub-engine helpers & lifecycle", () => {
  it("replicates Rust active_toc_index linear scan with non-monotonic TOC, external links, and non-linear units", () => {
    const nav: BookNavigationData = {
      total_weight: 300,
      linear_units: [
        {
          index: 0,
          is_linear: false,
          weight: 0,
          cumulative_weight: 0,
          display_label: "Cover",
        },
        {
          index: 1,
          is_linear: true,
          weight: 100,
          cumulative_weight: 100,
          display_label: "Ch 1",
        },
        {
          index: 2,
          is_linear: true,
          weight: 200,
          cumulative_weight: 300,
          display_label: "Ch 2",
        },
      ],
      toc: [
        {
          parent: null,
          depth: 0,
          subtree_end: 1,
          title: "External",
          locator: null,
          unit_index: null,
          unit_offset: 0,
          start_progression: null,
          flags: TOC_FLAG_EXTERNAL_LINK,
        },
        {
          parent: null,
          depth: 0,
          subtree_end: 2,
          title: "Appendix Jump (out of order)",
          locator: null,
          unit_index: 2,
          unit_offset: 0.6,
          start_progression: 0.8,
          flags: 0,
        },
        {
          parent: null,
          depth: 0,
          subtree_end: 4,
          title: "Chapter 1",
          locator: null,
          unit_index: 1,
          unit_offset: 0,
          start_progression: 0,
          flags: 0,
        },
        {
          parent: 2,
          depth: 1,
          subtree_end: 4,
          title: "Chapter 1.2",
          locator: null,
          unit_index: 1,
          unit_offset: 0.5,
          start_progression: 0.16,
          flags: 0,
        },
        {
          parent: null,
          depth: 0,
          subtree_end: 5,
          title: "Non-linear Cover",
          locator: null,
          unit_index: 0,
          unit_offset: 0,
          start_progression: null,
          flags: 0,
        },
      ],
    }

    // Linear section 1 at offset 0.7 picks Chapter 1.2 (index 3), ignoring non-linear cover and future Appendix
    expect(computeActiveTocIndex(nav, 1, 0.7)).toBe(3)
    // Linear section 2 at offset 0.2 ignores Appendix (offset 0.6 > 0.2) and picks Chapter 1.2 (index 3)
    expect(computeActiveTocIndex(nav, 2, 0.2)).toBe(3)
    // Linear section 2 at offset 0.8 picks Appendix (index 1) despite non-monotonic position in TOC array
    expect(computeActiveTocIndex(nav, 2, 0.8)).toBe(1)
    // Non-linear section 0 only matches TOC nodes pointing at section 0
    expect(computeActiveTocIndex(nav, 0, 0)).toBe(4)
  })

  it("flattens fallback foliate TOC into preorder Arena nodes and computes pageStats accurately", () => {
    const fakeBook = {
      sections: [
        { id: "ch1.xhtml", size: 100, linear: "yes" },
        { id: "ch2.xhtml", size: 200, linear: "yes" },
      ],
      resolveHref: (href: string) => {
        if (href.startsWith("ch1.xhtml")) return { index: 0, anchor: () => 0 }
        if (href.startsWith("ch2.xhtml")) return { index: 1, anchor: () => 0 }
        return null
      },
      isExternal: (href: string) => href.startsWith("https://"),
    }

    const nav = flattenFoliateToc(
      [
        {
          label: "Part 1",
          href: "ch1.xhtml#top",
          subitems: [{ label: "Sec 1.1", href: "ch1.xhtml#s1" }],
        },
        { label: "Website", href: "https://example.com" },
      ],
      fakeBook
    )

    expect(nav.toc).toHaveLength(3)
    expect(nav.toc[0]).toMatchObject({
      parent: null,
      depth: 0,
      subtree_end: 2,
      unit_index: 0,
      flags: 0,
    })
    expect(nav.toc[1]).toMatchObject({
      parent: 0,
      depth: 1,
      subtree_end: 2,
      locator: {
        kind: "epub",
        spine_index: 0,
        href: "ch1.xhtml",
        target: { kind: "fragment", fragment: "s1" },
      },
    })
    expect(nav.toc[2]).toMatchObject({
      flags: TOC_FLAG_EXTERNAL_LINK,
      locator: null,
      unit_index: null,
    })

    const renderer = new FakeRenderer()
    renderer.page = 2
    renderer.pages = 6 // 4 real pages + 2 padding pages
    expect(computePageStats(renderer as never, "paginated", false)).toEqual({
      current: 2,
      total: 4,
    })
    expect(computePageStats(renderer as never, "scrolled", false)).toBeNull()
    expect(computePageStats(renderer as never, "paginated", true)).toBeNull()
  })

  it("ignores raw detail.cfi on relocate, routes goToLocator through resolveVisualAnchor, and intercepts external-link", async () => {
    const bridge = createMockReaderBridge()
    const fakeView = createFakeView()
    const fakeBook = {
      transformTarget: new EventTarget(),
      metadata: { title: { en: "Book Title" }, author: [{ name: "Alice" }], language: ["en"] },
      dir: "rtl" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [
        { id: "OEBPS/ch0.xhtml", size: 100, cfi: "/6/2[ch0]" },
        { id: "OEBPS/ch1.xhtml", size: 200, cfi: "/6/4[ch1]" },
      ],
      resolveHref: () => ({ index: 0, anchor: () => 0 }),
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return fakeBook
          }
        } as never,
      }),
      createView: () => fakeView as never,
    })

    const relocations: RelocationSnapshot[] = []
    const externalLinks: string[] = []
    const container = document.createElement("div")

    const instance = await engine.open({
      tabId: "tab-1",
      bookId: "book-1",
      bookHash: "hash-1",
      fallbackTitle: "Fallback",
      preloadedNavigation: {
        toc: [
          {
            parent: null,
            depth: 0,
            subtree_end: 1,
            title: "Chapter 1",
            locator: null,
            unit_index: 1,
            unit_offset: 0,
            start_progression: 0,
            flags: 0,
          },
        ],
        linear_units: [
          { index: 0, is_linear: true, weight: 100, cumulative_weight: 100, display_label: null },
          { index: 1, is_linear: true, weight: 200, cumulative_weight: 300, display_label: null },
        ],
        total_weight: 300,
      },
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container,
      events: {
        onRelocate: (snap) => relocations.push(snap),
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: (url) => externalLinks.push(url),
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    expect(instance.metadata).toEqual({
      title: "Book Title",
      author: "Alice",
      language: "en",
      direction: "rtl",
      isFixedLayout: false,
    })
    expect(instance.capabilities.supportsFullTextTranslation).toBe(true)

    // Load chapter 1 with synthetic translation injected
    const doc = createStrictChapterDoc()
    fakeView.dispatchEvent(new CustomEvent("load", { detail: { doc, index: 1 } }))

    const p2 = doc.getElementById("p-2")!
    expect(p2.getAttribute("data-shiro-block-id")).toBeTruthy()
    injectSyntheticTranslation(doc, p2, "译文第二段")

    const visibleRange = doc.createRange()
    visibleRange.setStart(p2.firstChild!, 0)
    visibleRange.setEnd(p2.firstChild!, 6)

    // Dispatch relocate with bogus raw cfi -> engine must compute canonical CFI instead
    fakeView.dispatchEvent(
      new CustomEvent("relocate", {
        detail: {
          cfi: "epubcfi(/6/4!/999/BOGUS)",
          fraction: 0.45,
          section: { current: 1, total: 2 },
          range: visibleRange,
        },
      })
    )

    expect(relocations).toHaveLength(1)
    const snap = relocations[0]!
    expect(snap.locator.kind).toBe("epub")
    if (snap.locator.kind === "epub") {
      expect(snap.locator.target.kind).toBe("cfi")
      if (snap.locator.target.kind === "cfi") {
        expect(snap.locator.target.cfi).not.toBe("epubcfi(/6/4!/999/BOGUS)")
        expect(snap.locator.target.cfi).toContain("epubcfi(/6/4[ch1]!")
      }
    }
    expect(snap.activeTocIndex).toBe(0)
    expect(snap.pageStats).toEqual({ current: 2, total: 4 })

    // goToLocator must use renderer.goTo({ index, anchor }) and NEVER view.goTo(cfi)
    const targetLocator: NavigationLocator = snap.locator
    await instance.goToLocator(targetLocator)
    expect(fakeView.renderer.lastGoToTarget?.index).toBe(1)
    expect(typeof fakeView.renderer.lastGoToTarget?.anchor).toBe("function")
    const resolvedAnchor = fakeView.renderer.lastGoToTarget?.anchor?.(doc)
    expect(resolvedAnchor).toBeInstanceOf(Range)

    // External link with href_ compatibility
    const extEvent = new CustomEvent("external-link", {
      cancelable: true,
      detail: { a: document.createElement("a"), href_: "https://shiro.example" },
    })
    fakeView.dispatchEvent(extEvent)
    expect(extEvent.defaultPrevented).toBe(true)
    expect(externalLinks).toEqual(["https://shiro.example"])

    await instance.goLeft()
    await instance.goRight()
    expect(fakeView.goLeftCalls).toBe(1)
    expect(fakeView.goRightCalls).toBe(1)

    await instance.destroy()
    expect(fakeView.closed).toBe(true)
    expect(fakeView.removed).toBe(true)
    expect(fakeBook.destroy).toHaveBeenCalledTimes(1)
  })

  it("cleans up DOM nodes and backend session if aborted mid-open (StrictMode unmount)", async () => {
    const baseBridge = createMockReaderBridge()
    const epubCloseSpy = vi.fn(baseBridge.epubClose)
    let resolveOpen!: (res: Awaited<ReturnType<typeof baseBridge.epubOpen>>) => void
    const bridge = {
      ...baseBridge,
      epubOpen: () =>
        new Promise<Awaited<ReturnType<typeof baseBridge.epubOpen>>>((resolve) => {
          resolveOpen = resolve
        }),
      epubClose: epubCloseSpy,
    }

    const controller = new AbortController()
    const container = document.createElement("div")
    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return { sections: [], toc: [] }
          }
        } as never,
      }),
      createView: () => createFakeView() as never,
    })

    const openPromise = engine.open({
      tabId: "tab-strict",
      bookId: "book-1",
      bookHash: "hash-1",
      fallbackTitle: "StrictMode",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container,
      signal: controller.signal,
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    // Simulate React StrictMode unmount before epubOpen resolves
    controller.abort()
    resolveOpen({
      sessionId: "sess-leaked-guard",
      bookId: "book-1",
      fileSize: 1024,
      entries: {},
    })

    await expect(openPromise).rejects.toMatchObject({ name: "AbortError" })
    // Allow microtask cleanup of delayed loaderPromise resolution
    await Promise.resolve()
    await Promise.resolve()

    expect(container.childElementCount).toBe(0)
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-leaked-guard")
  })

  it("closes untracked desktop session on StrictMode abort but preserves reattached coordinator session", async () => {
    const baseBridge = createMockReaderBridge()
    const epubCloseSpy = vi.fn(async () => {})
    let resolveOpen!: (res: Awaited<ReturnType<typeof baseBridge.epubOpen>>) => void
    const bridge = {
      ...baseBridge,
      epubOpen: () =>
        new Promise<Awaited<ReturnType<typeof baseBridge.epubOpen>>>((resolve) => {
          resolveOpen = resolve
        }),
      epubAttachOrOpen: async (bookId: string, sessionId: string | null) => ({
        sessionId: sessionId ?? "fallback-new",
        bookId,
        fileSize: 1024,
        entries: {},
      }),
      epubClose: epubCloseSpy,
    }

    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return { sections: [], toc: [] }
          }
        } as never,
      }),
      createView: () => createFakeView() as never,
    })

    // Case 1: Desktop fresh open aborted before onSessionChange -> must close untracked session.
    const c1 = new AbortController()
    const p1 = engine.open({
      tabId: "tab-desktop-1",
      bookId: "book-1",
      bookHash: "hash-1",
      fallbackTitle: "Desktop Fresh",
      preloadedNavigation: null,
      manageSessionLifecycleInFrontend: false,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      signal: c1.signal,
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
        onSessionChange: () => {},
      },
    })
    c1.abort()
    resolveOpen({
      sessionId: "sess-desktop-untracked",
      bookId: "book-1",
      fileSize: 1024,
      entries: {},
    })
    await expect(p1).rejects.toMatchObject({ name: "AbortError" })
    await Promise.resolve()
    await Promise.resolve()
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-desktop-untracked")

    // Case 2: Desktop reattach returning existing coordinator session aborted -> must NOT close it.
    epubCloseSpy.mockClear()
    const c2 = new AbortController()
    let resolveNav!: (nav: BookNavigationData) => void
    const bridge2 = {
      ...bridge,
      bookNavigation: () =>
        new Promise<BookNavigationData>((resolve) => {
          resolveNav = resolve
        }),
    }
    const engine2 = new FoliateEpubEngine({
      bridge: bridge2,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return { sections: [], toc: [] }
          }
        } as never,
      }),
      createView: () => createFakeView() as never,
    })
    const p2 = engine2.open({
      tabId: "tab-desktop-2",
      bookId: "book-1",
      bookHash: "hash-1",
      fallbackTitle: "Desktop Reattach",
      preloadedNavigation: null,
      reattachSessionId: "sess-coordinator-owned",
      manageSessionLifecycleInFrontend: false,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      signal: c2.signal,
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
        onSessionChange: () => {},
      },
    })
    await Promise.resolve()
    c2.abort()
    resolveNav({ toc: [], linear_units: [], total_weight: 0 })
    await expect(p2).rejects.toMatchObject({ name: "AbortError" })
    await Promise.resolve()
    await Promise.resolve()
    expect(epubCloseSpy).not.toHaveBeenCalled()
  })

  it("handles mixed author metadata, non-double-wrapped fake base CFI, bad initialLocator fallback, live scroll unitOffset, and internal link history", async () => {
    const bridge = createMockReaderBridge()
    const fakeView = createFakeView()
    fakeView.resolveCFI = () => ({ index: -1, anchor: () => null })
    fakeView.renderer.scrolled = true
    fakeView.renderer.start = 600
    fakeView.renderer.viewSize = 1000

    const fakeBook = {
      transformTarget: new EventTarget(),
      metadata: {
        title: "Mixed Authors Book",
        author: ["Alice", { name: { en: "Bob" } }, { ja: "夏目漱石" }],
        language: "en",
      },
      dir: "ltr" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [
        { id: "OEBPS/ch0.xhtml", size: 100 },
        { id: "OEBPS/ch1.xhtml", size: 200 },
      ],
      resolveHref: () => null,
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return fakeBook
          }
        } as never,
      }),
      createView: () => fakeView as never,
    })

    const relocations: RelocationSnapshot[] = []
    const internalLinks: Array<{ href: string; before: RelocationSnapshot | null }> = []
    const container = document.createElement("div")

    const instance = await engine.open({
      tabId: "tab-2",
      bookId: "book-2",
      bookHash: "hash-2",
      fallbackTitle: "Fallback",
      preloadedNavigation: {
        toc: [
          {
            parent: null,
            depth: 0,
            subtree_end: 1,
            title: "Section 0 Start",
            locator: null,
            unit_index: 0,
            unit_offset: 0,
            start_progression: 0,
            flags: 0,
          },
          {
            parent: null,
            depth: 0,
            subtree_end: 2,
            title: "Section 0 Mid",
            locator: null,
            unit_index: 0,
            unit_offset: 0.5,
            start_progression: 0.25,
            flags: 0,
          },
        ],
        linear_units: [
          { index: 0, is_linear: true, weight: 100, cumulative_weight: 100, display_label: null },
          { index: 1, is_linear: true, weight: 200, cumulative_weight: 300, display_label: null },
        ],
        total_weight: 300,
      },
      initialLocator: {
        kind: "epub",
        spine_index: null,
        href: "missing.xhtml",
        target: {
          kind: "cfi",
          cfi: "epubcfi(/6/999!/4)",
        },
      },
      initialStyle: DEFAULT_TEST_STYLE,
      container,
      events: {
        onRelocate: (snap) => relocations.push(snap),
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onInternalLink: (href, before) => internalLinks.push({ href, before }),
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    // 1. String + object + foliate tidy()-unwrapped language-map author list normalized
    expect(instance.metadata.author).toBe("Alice, Bob, 夏目漱石")
    // 2. Bad initialLocator fell back to activeView.init({ showTextStart: true })
    expect(fakeView.initOptions).toEqual({ showTextStart: true })

    // 3. Load section 0 (without section.cfi) and fire relocate -> zero-lag live renderer.start/viewSize + non-double-wrapped CFI
    const doc = createStrictChapterDoc()
    fakeView.dispatchEvent(new CustomEvent("load", { detail: { doc, index: 0 } }))
    const p1 = doc.getElementById("p-1")!
    const visibleRange = doc.createRange()
    visibleRange.setStart(p1.firstChild!, 0)
    visibleRange.setEnd(p1.firstChild!, 4)

    fakeView.dispatchEvent(
      new CustomEvent("relocate", {
        detail: {
          fraction: 0.2,
          section: { current: 0, total: 2 },
          range: visibleRange,
        },
      })
    )

    expect(relocations).toHaveLength(1)
    if (relocations[0]!.locator.kind === "epub") {
      expect(relocations[0]!.locator.target.kind).toBe("cfi")
      if (relocations[0]!.locator.target.kind === "cfi") {
        expect(relocations[0]!.locator.target.cfi).toMatch(/^epubcfi\(\/6\/2!/)
        expect(relocations[0]!.locator.target.cfi).not.toContain(
          "epubcfi(epubcfi("
        )
      }
    }
    // Live scrolled offset = 600/1000 = 0.6 -> picks Section 0 Mid (index 1) without lag
    expect(relocations[0]!.activeTocIndex).toBe(1)

    // 4. Native view "link" event invokes onInternalLink with pre-jump snapshot
    fakeView.dispatchEvent(
      new CustomEvent("link", {
        detail: { a: document.createElement("a"), href: "#footnote-1" },
      })
    )
    expect(internalLinks).toHaveLength(1)
    expect(internalLinks[0]!.href).toBe("#footnote-1")
    expect(internalLinks[0]!.before).toEqual(relocations[0])

    await instance.destroy()
  })

  it("uses iframe-realm FontFace with variable weight descriptors, guards stale async loads, and replaces same-family fonts", async () => {
    const baseBridge = createMockReaderBridge()
    let resolveFontA!: (buf: ArrayBuffer) => void
    const bridge = {
      ...baseBridge,
      readerReadCustomFont: vi.fn((fontId: string) => {
        if (fontId === "font-a") {
          return new Promise<ArrayBuffer>((resolve) => {
            resolveFontA = resolve
          })
        }
        return Promise.resolve(new Uint8Array([1, 2, 3, 4]).buffer)
      }),
    }

    const fakeView = createFakeView()
    const fakeBook = {
      transformTarget: new EventTarget(),
      metadata: { title: "Font Test", language: "en" },
      dir: "ltr" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [{ id: "OEBPS/ch0.xhtml", size: 100 }],
      resolveHref: () => null,
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return fakeBook
          }
        } as never,
      }),
      createView: () => fakeView as never,
    })

    const styleWithFont = (fontId: string, family: string): CompiledReaderStyle => ({
      ...DEFAULT_TEST_STYLE,
      usesFontFaceFallback: true,
      customFontFace: {
        fontId,
        family,
        url: `shiro-font://localhost/font/${fontId}`,
        format: "ttf",
      },
    })

    const instance = await engine.open({
      tabId: "tab-font",
      bookId: "book-font",
      bookHash: "hash-font",
      fallbackTitle: "Font Test",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: styleWithFont("font-a", "SharedFamily"),
      container: document.createElement("div"),
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    const doc = createStrictChapterDoc()
    const constructed: Array<{
      family: string
      descriptors?: FontFaceDescriptors | undefined
    }> = []
    class FakeIframeFontFace {
      readonly family: string
      readonly descriptors?: FontFaceDescriptors | undefined
      constructor(
        family: string,
        _source: BufferSource | string,
        descriptors?: FontFaceDescriptors | undefined
      ) {
        this.family = family
        this.descriptors = descriptors
        constructed.push({ family, descriptors })
      }
      async load() {
        return this
      }
    }
    const addedFaces = new Set<FakeIframeFontFace>()
    Object.defineProperty(doc, "fonts", {
      value: {
        add: vi.fn((face: FakeIframeFontFace) => {
          addedFaces.add(face)
        }),
        delete: vi.fn((face: FakeIframeFontFace) => {
          addedFaces.delete(face)
        }),
      },
    })
    Object.defineProperty(doc, "defaultView", {
      value: {
        FontFace: FakeIframeFontFace,
        frameElement: null,
        getSelection: () => null,
      },
    })

    // Trigger load while font-a is active (async read pending), then switch to font-b before font-a resolves
    fakeView.dispatchEvent(new CustomEvent("load", { detail: { doc, index: 0 } }))
    instance.applyStyle(styleWithFont("font-b", "SharedFamily"))

    // Resolve stale font-a -> must be ignored because font-b is now active
    resolveFontA(new Uint8Array([9, 9]).buffer)
    await new Promise((r) => setTimeout(r, 0))

    expect(constructed).toHaveLength(1)
    expect(constructed[0]).toEqual({
      family: '"SharedFamily"',
      descriptors: { weight: "100 900", style: "normal" },
    })
    expect(addedFaces.size).toBe(1)

    // Switch to font-c with the SAME family name -> must delete old FontFace before adding new one
    instance.applyStyle(styleWithFont("font-c", "SharedFamily"))
    await new Promise((r) => setTimeout(r, 0))
    expect(constructed).toHaveLength(2)
    expect(addedFaces.size).toBe(1)

    // Digit-leading / quote-containing family name must be CSS-escaped and quoted for FontFace()
    instance.applyStyle(styleWithFont("font-d", '123-Test"Font'))
    await new Promise((r) => setTimeout(r, 0))
    expect(constructed[2]?.family).toBe('"123-Test\\"Font"')

    await instance.destroy()
  })

  it("remaps #fragment TOC and internal links to visible synthetic blocks in translated_only mode and emits point relocate CFI", async () => {
    const bridge = createMockReaderBridge()
    const fakeView = createFakeView()
    const fakeBook = {
      transformTarget: new EventTarget(),
      metadata: { title: "Translated Nav", language: "en" },
      dir: "ltr" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [{ id: "OEBPS/ch0.xhtml", size: 100, cfi: "/6/2!" }],
      resolveHref: (href: string) => {
        if (href.includes("#p-2")) {
          return {
            index: 0,
            anchor: (d: Document) => d.getElementById("p-2"),
          }
        }
        return null
      },
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return fakeBook
          }
        } as never,
      }),
      createView: () => fakeView as never,
    })

    const relocations: RelocationSnapshot[] = []
    const instance = await engine.open({
      tabId: "tab-trans-nav",
      bookId: "book-trans-nav",
      bookHash: "hash-trans-nav",
      fallbackTitle: "Translated Nav",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialViewState: {
        translationMode: "translated_only",
        targetLanguage: "zh-CN",
      },
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      events: {
        onRelocate: (snap) => relocations.push(snap),
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    const doc = createStrictChapterDoc()
    fakeView.dispatchEvent(new CustomEvent("load", { detail: { doc, index: 0 } }))
    const p1 = doc.getElementById("p-1")!
    const p2 = doc.getElementById("p-2")!
    const syn2 = injectSyntheticTranslation(doc, p2, "第二段译文")

    // 1. Stretched relocate range (p-1 -> hidden p-2) emits collapsed point CFI (no comma range)
    const stretched = doc.createRange()
    stretched.setStart(p1.firstChild!, 0)
    stretched.setEnd(p2.firstChild!, 3)
    fakeView.dispatchEvent(
      new CustomEvent("relocate", {
        detail: { fraction: 0, section: { current: 0, total: 1 }, range: stretched },
      })
    )
    expect(relocations).toHaveLength(1)
    if (relocations[0]!.locator.kind === "epub") {
      expect(relocations[0]!.locator.target.kind).toBe("cfi")
      if (relocations[0]!.locator.target.kind === "cfi") {
        expect(relocations[0]!.locator.target.cfi).not.toContain(",")
      }
    }

    // 2. goToLocator with href#p-2 resolves visual anchor to syn2 instead of hidden p-2
    await instance.goToLocator({
      kind: "epub",
      spine_index: 0,
      href: "OEBPS/ch0.xhtml",
      target: {
        kind: "fragment",
        fragment: "p-2",
      },
    })
    expect(fakeView.renderer.lastGoToTarget?.anchor?.(doc)).toBe(syn2)

    // 3. Clicking internal #p-2 link in translated_only mode intercepts and anchors on syn2
    const goToSpy = vi.spyOn(fakeView.renderer, "goTo")
    const linkEv = new CustomEvent("link", {
      cancelable: true,
      detail: { a: document.createElement("a"), href: "OEBPS/ch0.xhtml#p-2" },
    })
    fakeView.dispatchEvent(linkEv)
    expect(linkEv.defaultPrevented).toBe(true)
    expect(goToSpy).toHaveBeenCalledTimes(1)
    expect(goToSpy.mock.calls[0]?.[0]?.anchor?.(doc)).toBe(syn2)

    // Verify exclusive cumulative_weight prefix sums from flattenFoliateToc match Rust BookNavigation
    const flattened = flattenFoliateToc(
      [
        { label: "Ch1", href: "OEBPS/ch0.xhtml" },
        { label: "Ch2", href: "OEBPS/ch1.xhtml" },
        { label: "Home", href: "https://example.org/home" },
      ],
      {
        sections: [
          { id: "OEBPS/ch0.xhtml", size: 300 },
          { id: "OEBPS/ch1.xhtml", size: 700 },
        ],
        resolveHref: (href: string) => ({
          index: href.includes("ch1") ? 1 : 0,
          anchor: () => null,
        }),
        isExternal: (href: string) => /^https?:\/\//i.test(href),
      }
    )
    expect(flattened.linear_units.map((u) => u.cumulative_weight)).toEqual([0, 300])
    expect(flattened.total_weight).toBe(1000)
    expect(flattened.toc[0]?.start_progression).toBeCloseTo(0, 5)
    expect(flattened.toc[1]?.start_progression).toBeCloseTo(0.3, 5)
    expect(flattened.toc[2]?.external_url).toBe("https://example.org/home")

    await instance.destroy()
  })

  it("registers external SectionAugmenters from engine deps/open params and hydrates external TOC URLs", async () => {
    const baseBridge = createMockReaderBridge()
    const fakeView = createFakeView()
    const fakeBook = {
      metadata: { title: "Ext Book", language: "en" },
      dir: "ltr" as const,
      sections: [{ id: "OEBPS/ch0.xhtml", cfi: "/6/2", size: 500 }],
      toc: [
        { label: "Chapter 1", href: "OEBPS/ch0.xhtml" },
        { label: "Author Website", href: "https://example.org/author" },
      ],
      resolveHref: () => ({ index: 0, anchor: () => null }),
      isExternal: (href: string) => /^https?:\/\//i.test(href),
      destroy: vi.fn(),
    }

    const depMounted: number[] = []
    const openParamMounted: number[] = []
    const engine = new FoliateEpubEngine({
      bridge: baseBridge,
      augmenters: [
        {
          id: "dep-augmenter",
          onSectionMount: (ctx) => depMounted.push(ctx.sectionIndex),
          onVisibleWindowChange: () => {},
          onViewStateChange: () => {},
          onSectionUnmount: () => {},
        },
      ],
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return fakeBook
          }
        } as never,
      }),
      createView: () => fakeView as never,
    })

    const instance = await engine.open({
      tabId: "tab-ext",
      bookId: "book-ext",
      bookHash: "hash-ext",
      fallbackTitle: "Ext Book",
      preloadedNavigation: {
        toc: [
          {
            parent: null,
            depth: 0,
            subtree_end: 1,
            title: "Chapter 1",
            locator: {
              kind: "epub",
              spine_index: 0,
              href: "OEBPS/ch0.xhtml",
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
            subtree_end: 2,
            title: "Author Website",
            locator: null,
            unit_index: null,
            unit_offset: 0,
            start_progression: null,
            flags: 2,
          },
        ],
        linear_units: [
          {
            index: 0,
            is_linear: true,
            weight: 500,
            cumulative_weight: 0,
            display_label: "OEBPS/ch0.xhtml",
          },
        ],
        total_weight: 500,
      },
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      augmenters: [
        {
          id: "param-augmenter",
          onSectionMount: (ctx) => openParamMounted.push(ctx.sectionIndex),
          onVisibleWindowChange: () => {},
          onViewStateChange: () => {},
          onSectionUnmount: () => {},
        },
      ],
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    // External TOC node from backend BookNavigation must be hydrated with URL from foliate book.toc.
    expect(instance.navigation.toc[1]?.external_url).toBe("https://example.org/author")

    const doc = createStrictChapterDoc()
    fakeView.dispatchEvent(new CustomEvent("load", { detail: { doc, index: 0 } }))
    expect(depMounted).toEqual([0])
    expect(openParamMounted).toEqual([0])

    await instance.destroy()
  })

  it("exposes fixed-layout capabilities and suppresses reflow style overrides for pre-paginated EPUBs", async () => {
    const baseBridge = createMockReaderBridge()
    const fakeView = createFakeView()
    const fakeBook = {
      metadata: { title: "Manga Vol. 1", language: "ja" },
      rendition: { layout: "pre-paginated" },
      dir: "rtl" as const,
      sections: [{ id: "OEBPS/p01.xhtml", cfi: "/6/2", size: 200 }],
      toc: [{ label: "Cover", href: "OEBPS/p01.xhtml" }],
      resolveHref: () => ({ index: 0, anchor: () => null }),
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const engine = new FoliateEpubEngine({
      bridge: baseBridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return fakeBook
          }
        } as never,
      }),
      createView: () => fakeView as never,
    })

    const instance = await engine.open({
      tabId: "tab-fxl",
      bookId: "book-fxl",
      bookHash: "hash-fxl",
      fallbackTitle: "Manga Vol. 1",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    expect(instance.metadata.isFixedLayout).toBe(true)
    expect(instance.metadata.direction).toBe("rtl")
    expect(instance.capabilities).toMatchObject({
      supportsReflowTypography: false,
      supportsPublisherStyleOverride: false,
      supportsFlowSwitch: false,
      supportsColumnSwitch: false,
      supportsFixedZoom: true,
      supportsFullTextTranslation: false,
      supportsBilingualInterleave: false,
    })
    expect(fakeView.renderer.styles).toBe(DEFAULT_TEST_STYLE.chapterCss)
    expect(fakeView.renderer.getAttribute("flow")).toBeNull()
    expect(fakeView.renderer.getAttribute("margin")).toBeNull()
    expect(fakeView.renderer.getAttribute("max-column-count")).toBeNull()

    await instance.destroy()
  })

  it("closes newly reopened 3101 session when open() aborts mid-init and handles cross-Realm #fragment anchors", async () => {
    const { CommandError } = await import("@shiro/ui-shared/api/tauri-bridge")
    const { READER_ERROR_CODES } = await import(
      "@shiro/ui-shared/api/reader-bridge"
    )
    const baseBridge = createMockReaderBridge()
    let openCalls = 0
    let resolveReopen!: (res: Awaited<ReturnType<typeof baseBridge.epubOpen>>) => void
    const epubCloseSpy = vi.fn(async () => {})
    const onSessionChangeSpy = vi.fn()

    const bridgeWithReopen = {
      ...baseBridge,
      epubOpen: (bookId: string) => {
        openCalls += 1
        if (openCalls === 1) {
          return Promise.resolve({
            sessionId: "sess-initial-s1",
            bookId,
            fileSize: 1024,
            entries: { "META-INF/container.xml": 12 },
          })
        }
        return new Promise<Awaited<ReturnType<typeof baseBridge.epubOpen>>>(
          (resolve) => {
            resolveReopen = resolve
          }
        )
      },
      epubReadEntry: async () => {
        throw new CommandError({
          operationId: "op-read",
          errorId: "err-3101",
          code: READER_ERROR_CODES.SESSION_NOT_FOUND,
          key: "client.reader.session_not_found",
          itemId: null,
        })
      },
      epubClose: epubCloseSpy,
    }

    const controller = new AbortController()
    const engine = new FoliateEpubEngine({
      bridge: bridgeWithReopen,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          private readonly loader: { loadText: (name: string) => Promise<string | null> }
          constructor(loader: { loadText: (name: string) => Promise<string | null> }) {
            this.loader = loader
          }
          async init() {
            await this.loader.loadText("META-INF/container.xml")
            return { sections: [], toc: [] }
          }
        } as never,
      }),
      createView: () => createFakeView() as never,
    })

    const openPromise = engine.open({
      tabId: "tab-reopen-abort",
      bookId: "book-reopen-abort",
      bookHash: "hash-reopen-abort",
      fallbackTitle: "Reopen Abort",
      preloadedNavigation: { toc: [], linear_units: [], total_weight: 0 },
      manageSessionLifecycleInFrontend: false,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      signal: controller.signal,
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
        onSessionChange: onSessionChangeSpy,
      },
    })

    // Wait until loader has triggered the 3101 reopen epubOpen("sess-reopened-s2")
    await vi.waitFor(() => {
      expect(openCalls).toBe(2)
    })
    controller.abort()
    resolveReopen({
      sessionId: "sess-reopened-s2",
      bookId: "book-reopen-abort",
      fileSize: 1024,
      entries: {},
    })

    await expect(openPromise).rejects.toMatchObject({ name: "AbortError" })
    await Promise.resolve()
    await Promise.resolve()

    expect(onSessionChangeSpy).toHaveBeenCalledTimes(1)
    expect(onSessionChangeSpy).toHaveBeenCalledWith("sess-initial-s1")
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-reopened-s2")

    // Cross-Realm Element anchor in #fragment navigation must resolve without TypeError
    const doc = createStrictChapterDoc()
    const p2 = doc.getElementById("p-2")!
    const crossRealmP2 = {
      nodeType: 1,
      parentNode: p2.parentNode,
      hasAttribute: (name: string) => p2.hasAttribute(name),
      getAttribute: (name: string) => p2.getAttribute(name),
    } as unknown as Element
    const navView = createFakeView()
    const navBook = {
      transformTarget: new EventTarget(),
      metadata: { title: "CrossRealm Nav", language: "en" },
      dir: "ltr" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [{ id: "OEBPS/ch0.xhtml", size: 100, cfi: "/6/2!" }],
      resolveHref: () => ({
        index: 0,
        anchor: () => crossRealmP2,
      }),
      isExternal: () => false,
      destroy: vi.fn(),
    }
    const navEngine = new FoliateEpubEngine({
      bridge: baseBridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return navBook
          }
        } as never,
      }),
      createView: () => navView as never,
    })
    const navInstance = await navEngine.open({
      tabId: "tab-cross-realm",
      bookId: "book-cross-realm",
      bookHash: "hash-cross-realm",
      fallbackTitle: "CrossRealm Nav",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container: document.createElement("div"),
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    await expect(
      navInstance.goToLocator({
        kind: "epub",
        spine_index: 0,
        href: "OEBPS/ch0.xhtml",
        target: {
          kind: "fragment",
          fragment: "p-2",
        },
      })
    ).resolves.toBeUndefined()
    expect(navView.renderer.lastGoToTarget?.anchor?.(doc)).toBe(crossRealmP2)
    await navInstance.destroy()
  })

  it("switches chapter at scrolled boundaries in non-seamless mode and toggles seamlessly into ContinuousScrollRenderer preserving locator", async () => {
    const baseBridge = createMockReaderBridge()
    const boundaryView = createFakeView()
    boundaryView.renderer.scrolled = true
    boundaryView.renderer.start = 500
    boundaryView.renderer.size = 400
    boundaryView.renderer.end = 900
    boundaryView.renderer.viewSize = 900

    const goToCalls: Array<{
      index: number
      anchorResult: unknown
    }> = []
    let goToSettledCount = 0
    const origGoTo = boundaryView.renderer.goTo.bind(boundaryView.renderer)
    boundaryView.renderer.goTo = async (target: {
      index: number
      anchor?: ((doc: Document) => Range | Element | number | null) | undefined
    }) => {
      goToCalls.push({
        index: target.index,
        anchorResult: target.anchor?.(document),
      })
      await origGoTo(target)
      goToSettledCount += 1
    }

    const boundaryBook = {
      transformTarget: new EventTarget(),
      metadata: { title: "Scroll Book", language: "en" },
      dir: "ltr" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [
        { id: "OEBPS/ch0.xhtml", size: 100, cfi: "/6/2!", linear: null },
        { id: "OEBPS/notes.xhtml", size: 50, cfi: "/6/4!", linear: "no" },
        { id: "OEBPS/ch1.xhtml", size: 200, cfi: "/6/6!", linear: null },
      ],
      resolveHref: () => ({ index: 0, anchor: () => 0 }),
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const continuousGoToTargets: Array<{ index: number }> = []
    let continuousDestroyed = 0
    const fakeContinuous = {
      scrolled: true,
      render: vi.fn(),
      setStyles: vi.fn(),
      applyLayout: vi.fn(),
      goTo: vi.fn(async (target: { index: number }) => {
        continuousGoToTargets.push({ index: target.index })
      }),
      goToFraction: vi.fn().mockResolvedValue(undefined),
      scrollByPage: vi.fn().mockResolvedValue(undefined),
      getContents: vi.fn(() => []),
      destroy: vi.fn(() => {
        continuousDestroyed += 1
      }),
    }

    let nowMs = 1000
    const engine = new FoliateEpubEngine({
      bridge: baseBridge,
      now: () => nowMs,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return boundaryBook
          }
        } as never,
      }),
      createView: () => boundaryView as never,
      createContinuousRenderer: () => fakeContinuous as never,
    })

    const scrolledNonSeamlessStyle: CompiledReaderStyle = {
      ...DEFAULT_TEST_STYLE,
      paginatorAttributes: {
        ...DEFAULT_TEST_STYLE.paginatorAttributes,
        flow: "scrolled",
      },
      seamlessScroll: false,
    }

    const container = document.createElement("div")
    const relocations: RelocationSnapshot[] = []
    const instance = await engine.open({
      tabId: "tab-scroll-modes",
      bookId: "book-scroll-modes",
      bookHash: "hash-scroll-modes",
      fallbackTitle: "Scroll Book",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: scrolledNonSeamlessStyle,
      container,
      events: {
        onRelocate: (snap) => relocations.push(snap),
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    const doc0 = createStrictChapterDoc()
    boundaryView.dispatchEvent(
      new CustomEvent("load", { detail: { doc: doc0, index: 0 } })
    )
    boundaryView.dispatchEvent(
      new CustomEvent("relocate", {
        detail: {
          cfi: "/6/2!",
          fraction: 0.95,
          section: { current: 0, total: 3 },
        },
      })
    )

    // Mode A: Wheel overscroll at bottom skips non-linear section 1 and goes to section 2 at top (anchor() === 0)
    nowMs += 300
    doc0.dispatchEvent(new WheelEvent("wheel", { deltaY: 100, cancelable: true }))
    await vi.waitFor(() => {
      expect(goToSettledCount).toBe(1)
    })
    await Promise.resolve()
    expect(goToCalls[0]).toEqual({ index: 2, anchorResult: 0 })

    // Relocate to section 2 top -> wheel up switches back to section 0 bottom (anchor() === 1)
    const doc2 = createStrictChapterDoc()
    boundaryView.dispatchEvent(
      new CustomEvent("load", { detail: { doc: doc2, index: 2 } })
    )
    boundaryView.dispatchEvent(
      new CustomEvent("relocate", {
        detail: {
          cfi: "/6/6!",
          fraction: 0.5,
          section: { current: 2, total: 3 },
        },
      })
    )
    boundaryView.renderer.start = 0
    boundaryView.renderer.end = 400
    nowMs += 600
    doc2.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, cancelable: true }))
    await vi.waitFor(() => {
      expect(goToSettledCount).toBe(2)
    })
    await Promise.resolve()
    expect(goToCalls[1]).toEqual({ index: 0, anchorResult: 1 })
    expect(relocations).toHaveLength(2)

    // Mode B: Toggling seamlessScroll = true hides foliate-view and opens ContinuousScrollRenderer at last locator (section 2)
    instance.applyStyle({
      ...scrolledNonSeamlessStyle,
      seamlessScroll: true,
    })
    await vi.waitFor(() => {
      expect(continuousGoToTargets).toEqual([{ index: 2 }])
    })
    expect(boundaryView.style.display).toBe("none")

    // Toggling seamlessScroll = false destroys ContinuousScrollRenderer and restores foliate-view
    instance.applyStyle(scrolledNonSeamlessStyle)
    await vi.waitFor(() => {
      expect(continuousDestroyed).toBe(1)
    })
    expect(boundaryView.style.display).toBe("block")
    expect(goToCalls.at(-1)?.index).toBe(2)

    await instance.destroy()
  })

  it("strips inline publisher backgrounds on mounted reflowable sections (while preserving hr/img) and reattaches reused sections on seamlessScroll exit", async () => {
    const baseBridge = createMockReaderBridge()
    const view = createFakeView()
    view.renderer.scrolled = true

    const styledXhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" style="background: rgb(228, 222, 204) !important;">
  <body style="background: rgb(228, 222, 204) url('old-paper.jpg') !important; color: #111;">
    <p id="p-bg" style="background-color: rgb(123, 190, 238) !important; background-image: url('bg.png'); margin-top: 4px;">Text</p>
    <hr id="hr-keep" style="background-color: rgb(153, 153, 153);" />
    <img id="img-keep" alt="x" style="background-color: rgb(255, 255, 255);" />
  </body>
</html>`
    const sectionDoc = new DOMParser().parseFromString(
      styledXhtml,
      "application/xhtml+xml"
    )
    view.renderer.docs = [{ index: 0, doc: sectionDoc }]

    const book = {
      transformTarget: new EventTarget(),
      metadata: { title: "Bg Book", language: "zh" },
      dir: "ltr" as const,
      rendition: { layout: "reflowable" as const },
      toc: [],
      sections: [{ id: "OEBPS/ch0.xhtml", size: 100, cfi: "/6/2!", linear: null }],
      resolveHref: () => ({ index: 0, anchor: () => 0 }),
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const fakeContinuous = {
      scrolled: true,
      render: vi.fn(),
      setStyles: vi.fn(),
      applyLayout: vi.fn(),
      goTo: vi.fn().mockResolvedValue(undefined),
      goToFraction: vi.fn().mockResolvedValue(undefined),
      scrollByPage: vi.fn().mockResolvedValue(undefined),
      getContents: vi.fn(() => []),
      destroy: vi.fn(),
    }

    const engine = new FoliateEpubEngine({
      bridge: baseBridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return book
          }
        } as never,
      }),
      createView: () => view as never,
      createContinuousRenderer: () => fakeContinuous as never,
    })

    const scrolledStyle: CompiledReaderStyle = {
      ...DEFAULT_TEST_STYLE,
      paginatorAttributes: {
        ...DEFAULT_TEST_STYLE.paginatorAttributes,
        flow: "scrolled",
      },
      seamlessScroll: false,
    }

    const container = document.createElement("div")
    const instance = await engine.open({
      tabId: "tab-bg",
      bookId: "book-bg",
      bookHash: "hash-bg",
      fallbackTitle: "Bg Book",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: scrolledStyle,
      container,
      events: {
        onRelocate: () => {},
        onSelectionChange: () => {},
        onViewportTap: () => {},
        onExternalLink: () => {},
        onKeyCommand: () => {},
        onFatalError: () => {},
      },
    })

    // Initial load of reflowable section strips inline backgrounds on html, body, and descendants except hr/img
    view.dispatchEvent(
      new CustomEvent("load", { detail: { doc: sectionDoc, index: 0 } })
    )
    expect(sectionDoc.documentElement.style.getPropertyValue("background")).toBe("")
    expect(sectionDoc.documentElement.style.getPropertyValue("background-color")).toBe("")
    expect(sectionDoc.body.style.getPropertyValue("background")).toBe("")
    expect(sectionDoc.body.style.getPropertyValue("background-image")).toBe("")
    expect(sectionDoc.body.style.color).toBeTruthy()
    const pEl = sectionDoc.getElementById("p-bg") as HTMLElement
    expect(pEl.style.backgroundColor).toBe("")
    expect(pEl.style.backgroundImage).toBe("")
    expect(pEl.style.marginTop).toBe("4px")
    const hrEl = sectionDoc.getElementById("hr-keep") as HTMLElement
    const imgEl = sectionDoc.getElementById("img-keep") as HTMLElement
    expect(hrEl.style.backgroundColor).toBe("rgb(153, 153, 153)")
    expect(imgEl.style.backgroundColor).toBe("rgb(255, 255, 255)")

    // Relocate so lastEmittedRelocation points to section 0, then toggle seamlessScroll true -> false
    view.dispatchEvent(
      new CustomEvent("relocate", {
        detail: {
          cfi: "/6/2!",
          fraction: 0.25,
          section: { current: 0, total: 1 },
        },
      })
    )

    instance.applyStyle({ ...scrolledStyle, seamlessScroll: true })
    await vi.waitFor(() => {
      expect(fakeContinuous.goTo).toHaveBeenCalledWith(
        expect.objectContaining({ index: 0 })
      )
    })

    // Re-dirty body inline style to confirm reattachActiveViewContents re-attaches and cleans reused doc
    // even when FakeRenderer.goTo does not emit a new "load" event
    sectionDoc.body.style.setProperty("background", "rgb(228, 222, 204)", "important")
    instance.applyStyle(scrolledStyle)
    await vi.waitFor(() => {
      expect(fakeContinuous.destroy).toHaveBeenCalledTimes(1)
      expect(sectionDoc.body.style.getPropertyValue("background")).toBe("")
    })

    await instance.destroy()
  })

  it("patches cover fallback without null Blob, collapses Unicode TOC whitespace, sanitizes &nbsp; createDocument, supports fixed zoom, and extracts EPUB3 footnotes", async () => {
    expect(collapseUnicodeWhitespace("  第一章\u00a0\u3000引言\u2002 ")).toBe(
      "第一章 引言"
    )

    const baseBridge = createMockReaderBridge()
    const coverPng = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]).buffer
    const brokenXhtml = `<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><body><p>Hello&nbsp;world</p><script>alert(1)</script></body></html>`
    const notesXhtml = `<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
  <body>
    <aside id="fn-1" epub:type="footnote">
      <p>Footnote body text. <a epub:type="backlink" href="ch0.xhtml#ref-1">↩</a></p>
    </aside>
  </body>
</html>`

    const bridge: typeof baseBridge = {
      ...baseBridge,
      async epubOpen(bookId) {
        const opened = await baseBridge.epubOpen(bookId)
        return {
          ...opened,
          entries: {
            ...opened.entries,
            "OEBPS/Images/Cover.jpg": coverPng.byteLength,
            "OEBPS/ch0.xhtml": brokenXhtml.length,
            "OEBPS/notes.xhtml": notesXhtml.length,
          },
        }
      },
      async epubReadEntry(sessionId, path) {
        if (path === "OEBPS/Images/Cover.jpg") return coverPng
        if (path === "OEBPS/ch0.xhtml") {
          return new TextEncoder().encode(brokenXhtml).buffer
        }
        if (path === "OEBPS/notes.xhtml") {
          return new TextEncoder().encode(notesXhtml).buffer
        }
        return baseBridge.epubReadEntry(sessionId, path)
      },
    }

    const view = createFakeView()
    const book = {
      transformTarget: new EventTarget(),
      metadata: { title: "Compat Book", language: "en" },
      dir: "ltr" as const,
      rendition: {},
      resources: {
        cover: { id: "missing", href: "OEBPS/missing.jpg", mediaType: "image/jpeg" },
        manifest: [
          { id: "cov", href: "OEBPS/Images/Cover.jpg", mediaType: "image/jpeg" },
          { id: "c0", href: "OEBPS/ch0.xhtml", mediaType: "application/xhtml+xml" },
          { id: "n1", href: "OEBPS/notes.xhtml", mediaType: "application/xhtml+xml" },
        ],
        spine: [
          { idref: "c0", linear: "yes", properties: ["rendition:layout-pre-paginated"] },
          { idref: "n1", linear: "yes", properties: ["rendition:layout-pre-paginated"] },
        ],
      },
      toc: [{ label: "Preface\u00a0\u30001", href: "OEBPS/ch0.xhtml" }],
      sections: [
        { id: "OEBPS/ch0.xhtml", size: 100, cfi: "/6/2!", linear: "yes", createDocument: vi.fn() },
        { id: "OEBPS/notes.xhtml", size: 100, cfi: "/6/4!", linear: "yes", createDocument: vi.fn() },
      ],
      resolveHref: (href: string) => {
        if (href.startsWith("OEBPS/notes.xhtml")) {
          const frag = href.split("#")[1] ?? ""
          return {
            index: 1,
            anchor: (doc: Document) => (frag ? doc.getElementById(frag) : 0),
          }
        }
        return { index: 0, anchor: () => 0 }
      },
      isExternal: () => false,
      destroy: vi.fn(),
    }

    const footnotes: Array<{ readonly text: string; readonly kind: string; readonly html: string }> = []
    const engine = new FoliateEpubEngine({
      bridge,
      loadModules: async () => ({
        View: FakeViewCtor as never,
        EPUB: class {
          async init() {
            return book
          }
        } as never,
      }),
      createView: () => view as never,
    })

    const container = document.createElement("div")
    const instance = await engine.open({
      tabId: "t-compat",
      bookId: "b-compat",
      bookHash: "d".repeat(64),
      fallbackTitle: "Fallback",
      preloadedNavigation: null,
      initialLocator: undefined,
      initialStyle: DEFAULT_TEST_STYLE,
      container,
      events: {
        onRelocate: vi.fn(),
        onSelectionChange: vi.fn(),
        onViewportTap: vi.fn(),
        onExternalLink: vi.fn(),
        onFootnote: (fn) => footnotes.push(fn),
        onKeyCommand: vi.fn(),
        onFatalError: vi.fn(),
      },
    })

    // Per-section pre-paginated promoted whole book to fixed layout with zoom support
    expect(instance.metadata.isFixedLayout).toBe(true)
    expect(instance.capabilities.supportsFixedZoom).toBe(true)
    expect(view.renderer.getAttribute("zoom")).toBe("fit-page")
    instance.setFixedZoom?.(1.5)
    expect(view.renderer.getAttribute("zoom")).toBe("1.5")
    expect(instance.getFixedZoom?.()).toBe(1.5)

    // TOC Unicode whitespace collapsed
    expect(book.toc[0]?.label).toBe("Preface 1")

    // Fallback cover resolves Cover.jpg instead of returning 4-byte "null" Blob
    const coverBlob = await (book as unknown as { getCover(): Promise<Blob | null> }).getCover()
    expect(coverBlob).not.toBeNull()
    expect(coverBlob?.size).toBe(coverPng.byteLength)

    // section.createDocument() falls back &nbsp; XHTML cleanly and strips <script>
    const doc0 = await book.sections[0]!.createDocument!()
    expect(doc0.querySelector("parsererror")).toBeNull()
    expect(doc0.querySelector("script")).toBeNull()
    expect(doc0.body.textContent).toContain("Hello")

    // EPUB3 noteref link intercepts navigation and emits sanitized footnote payload without backlink
    const linkHost = document.createElement("div")
    linkHost.innerHTML = '<a id="ref-1" epub:type="noteref" href="notes.xhtml#fn-1">1</a>'
    const anchor = linkHost.querySelector("a")!
    const linkEvent = new CustomEvent("link", {
      cancelable: true,
      detail: { a: anchor, href: "OEBPS/notes.xhtml#fn-1" },
    })
    view.dispatchEvent(linkEvent)
    expect(linkEvent.defaultPrevented).toBe(true)

    await vi.waitFor(() => {
      expect(footnotes).toHaveLength(1)
    })
    expect(footnotes[0]?.kind).toBe("footnote")
    expect(footnotes[0]?.text).toContain("Footnote body text.")
    expect(footnotes[0]?.html).not.toContain("backlink")

    await instance.destroy()
  })
})
