import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { CompiledReaderStyle } from "@shiro/ui-shared/features/reader/contracts/settings"
import {
  ContinuousScrollRenderer,
  type ContinuousScrollRelocateDetail,
} from "@shiro/ui-shared/features/reader/engine/epub/continuous-scroll-renderer"
import { getDefaultReaderProfile } from "@shiro/ui-shared/features/reader/settings/defaults"
import { compileReaderStyle } from "@shiro/ui-shared/features/reader/settings/style-compiler"

function makeScrolledStyle(seamlessScroll = true): CompiledReaderStyle {
  const base = getDefaultReaderProfile(false)
  return compileReaderStyle(
    {
      ...base,
      layout: {
        ...base.layout,
        pageMarginBlockPx: 20,
      },
      behavior: {
        ...base.behavior,
        flow: "scrolled",
        seamlessScroll,
      },
    },
    "default",
    [],
    "shiro-font://localhost/font/",
    false,
    1024
  )
}

function makeMockSectionDoc(index: number, heightPx: number): Document {
  const doc = new DOMParser().parseFromString(
    `<?xml version="1.0" encoding="utf-8"?>
     <html xmlns="http://www.w3.org/1999/xhtml">
       <head></head>
       <body>
         <p id="p-${index}">Chapter ${index} paragraph</p>
         <a id="int-${index}" href="ch${index + 1}.xhtml">Next</a>
         <a id="ext-${index}" href="https://example.com/${index}">Ext</a>
       </body>
     </html>`,
    "application/xhtml+xml"
  )
  Object.defineProperty(doc.documentElement, "scrollHeight", {
    value: heightPx,
    configurable: true,
  })
  Object.defineProperty(doc.body, "scrollHeight", {
    value: heightPx,
    configurable: true,
  })
  return doc
}

describe("ContinuousScrollRenderer", () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it("mounts initial chapter, preloads adjacent linear chapters (skipping linear='no'), compensates prepend scrollTop, and updates section on scroll", async () => {
    const container = document.createElement("div")
    const unloadSpies = Array.from({ length: 5 }, () => vi.fn())
    const sections = [
      { id: "ch0.xhtml", size: 100, linear: null, unload: unloadSpies[0] },
      { id: "notes.xhtml", size: 50, linear: "no", unload: unloadSpies[1] },
      { id: "ch1.xhtml", size: 200, linear: null, unload: unloadSpies[2] },
      { id: "ch2.xhtml", size: 200, linear: null, unload: unloadSpies[3] },
      { id: "ch3.xhtml", size: 200, linear: null, unload: unloadSpies[4] },
    ]
    const docsByIndex = new Map<number, Document>([
      [0, makeMockSectionDoc(0, 600)],
      [1, makeMockSectionDoc(1, 300)],
      [2, makeMockSectionDoc(2, 700)],
      [3, makeMockSectionDoc(3, 700)],
      [4, makeMockSectionDoc(4, 700)],
    ])
    const fakeBook = {
      metadata: { language: "en" },
      sections,
      isExternal: (href: string) => href.startsWith("https://"),
    }

    const loadedOrder: number[] = []
    const unmountedOrder: number[] = []
    const relocations: ContinuousScrollRelocateDetail[] = []
    const externalLinks: string[] = []
    const internalLinks: string[] = []

    const renderer = new ContinuousScrollRenderer({
      container,
      book: fakeBook as never,
      initialStyle: makeScrolledStyle(true),
      loadSectionSrc: async (_section, index) => `mock://sec/${index}`,
      mountIframeDoc: async (_iframe, _src, index) =>
        docsByIndex.get(index) ?? null,
      onSectionLoad: (idx) => loadedOrder.push(idx),
      onSectionUnmount: (idx) => unmountedOrder.push(idx),
      onRelocate: (detail) => relocations.push(detail),
      onExternalLink: (url) => externalLinks.push(url),
      onInternalLink: (href) => internalLinks.push(href),
      relocateDebounceMs: 100,
      maxMountedSections: 3,
    })

    Object.defineProperty(renderer.scrollElement, "clientHeight", {
      value: 400,
      configurable: true,
    })

    await renderer.goTo({ index: 2 })

    // Section 2 mounted first; linear neighbor 3 appended and 0 prepended (skipping non-linear 1)
    expect(loadedOrder).toContain(2)
    expect(loadedOrder).toContain(3)
    expect(loadedOrder).toContain(0)
    expect(loadedOrder).not.toContain(1)

    // Prepending section 0 (600 + 2*12 vertical padding = 624px) compensated scrollTop so chapter 2 stays anchored
    const slot0Height = 600 + 24
    expect(renderer.scrollElement.scrollTop).toBe(slot0Height)

    // Style updates propagate to all mounted iframes
    const updatedStyle = makeScrolledStyle(true)
    renderer.applyLayout({
      ...updatedStyle,
      chapterCss: "body { color: #123456; }",
    })
    for (const { doc } of renderer.getContents()) {
      expect(
        doc.querySelector("[data-shiro-continuous-style]")?.textContent
      ).toContain("#123456")
    }

    // Internal and external links inside mounted section docs route cleanly
    const doc2 = docsByIndex.get(2)!
    doc2
      .getElementById("ext-2")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))
    doc2
      .getElementById("int-2")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))
    expect(externalLinks).toEqual(["https://example.com/2"])
    expect(internalLinks).toEqual(["ch3.xhtml"])

    // Scroll into section 3 -> relocates to section 3, loads section 4, and prunes distant section 0
    const slot2Height = 700 + 24
    renderer.scrollElement.scrollTop = slot0Height + slot2Height + 120
    renderer.scrollElement.dispatchEvent(new Event("scroll"))
    await vi.advanceTimersByTimeAsync(150)

    expect(relocations.at(-1)?.sectionIndex).toBe(3)
    expect(loadedOrder).toContain(4)

    // Scroll into section 4 so section 0 is >= 2 slots away & far outside preload margin -> pruned
    const slot3Height = 700 + 24
    renderer.scrollElement.scrollTop =
      slot0Height + slot2Height + slot3Height + 100
    renderer.scrollElement.dispatchEvent(new Event("scroll"))
    await vi.advanceTimersByTimeAsync(150)

    expect(unmountedOrder).toContain(0)
    expect(unloadSpies[0]).toHaveBeenCalled()

    renderer.destroy()
  })

  it("protects sections with active selection from pruning", async () => {
    const container = document.createElement("div")
    const sections = [0, 1, 2, 3, 4].map((i) => ({
      id: `ch${i}.xhtml`,
      size: 200,
      linear: null,
      unload: vi.fn(),
    }))
    const docsByIndex = new Map<number, Document>(
      [0, 1, 2, 3, 4].map((i) => [i, makeMockSectionDoc(i, 700)])
    )
    const fakeBook = {
      metadata: { language: "en" },
      sections,
    }

    const unmounted: number[] = []
    const renderer = new ContinuousScrollRenderer({
      container,
      book: fakeBook as never,
      initialStyle: makeScrolledStyle(true),
      loadSectionSrc: async (_section, index) => `mock://sec/${index}`,
      mountIframeDoc: async (_iframe, _src, index) =>
        docsByIndex.get(index) ?? null,
      onSectionLoad: vi.fn(),
      onSectionUnmount: (idx) => unmounted.push(idx),
      onRelocate: vi.fn(),
      onExternalLink: vi.fn(),
      onInternalLink: vi.fn(),
      hasSectionSelection: (idx) => idx === 0,
      maxMountedSections: 3,
    })

    Object.defineProperty(renderer.scrollElement, "clientHeight", {
      value: 400,
      configurable: true,
    })

    await renderer.goTo({ index: 0 })
    renderer.scrollElement.scrollTop = 2900
    renderer.scrollElement.dispatchEvent(new Event("scroll"))
    await vi.advanceTimersByTimeAsync(150)

    expect(unmounted).not.toContain(0)
    renderer.destroy()
  })
})
