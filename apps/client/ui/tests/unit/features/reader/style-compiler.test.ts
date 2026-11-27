import { describe, expect, it, vi } from "vitest"
import { setReaderBridgeForTest } from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import type { CustomFontMeta } from "@shiro/ui-shared/features/reader/contracts/settings"
import {
  getDefaultReaderProfile,
  READER_SURFACE_PRESETS,
} from "@shiro/ui-shared/features/reader/settings/defaults"
import {
  deleteCustomFont,
  ensureReaderSettingsLoaded,
  flushPendingReaderSettingsWrites,
  getReaderSettingsSnapshot,
  loadSharedCustomFontBuffer,
  pickAndImportCustomFont,
  resetBookReaderOverrides,
  setBookReaderOverrideField,
  updateGlobalReaderField,
} from "@shiro/ui-shared/features/reader/settings/reader-settings-store"
import {
  compileReaderStyle,
  escapeCssString,
} from "@shiro/ui-shared/features/reader/settings/style-compiler"
import { APP_THEMES } from "@shiro/ui-shared/lib/theme"

describe("StyleCompiler and reader settings store", () => {
  it("compiles complete CSS variables for all 6 presets including follow_app across all 5 app themes", () => {
    const base = getDefaultReaderProfile(false)

    for (const preset of READER_SURFACE_PRESETS) {
      const themes = preset === "follow_app" ? APP_THEMES : (["default"] as const)
      for (const appTheme of themes) {
        const compiled = compileReaderStyle(
          { ...base, appearance: { surface: preset } },
          appTheme,
          [],
          "shiro-font://localhost/font/",
          false,
          1200
        )
        expect(compiled.viewportCssVars["--reader-bg"]).toBeTruthy()
        expect(compiled.viewportCssVars["--reader-fg"]).toBeTruthy()
        expect(compiled.viewportCssVars["--reader-muted"]).toBeTruthy()
        expect(compiled.viewportCssVars["--reader-border"]).toBeTruthy()
        expect(compiled.viewportCssVars["--reader-selection-bg"]).toBeTruthy()
        expect(compiled.viewportCssVars["--reader-selection-fg"]).toBeTruthy()
        expect(compiled.chapterCss).toContain(
          `::selection {\n  background-color: ${compiled.viewportCssVars["--reader-selection-bg"]};\n  color: ${compiled.viewportCssVars["--reader-selection-fg"]};`
        )
        if (preset === "night_dark" || preset === "amoled_black") {
          expect(compiled.isDarkSurface).toBe(true)
          expect(compiled.chapterCss).toContain("color-scheme: dark")
          expect(compiled.viewportCssVars["--background"]).toBeTruthy()
          expect(compiled.viewportCssVars["--foreground"]).toBeTruthy()
          expect(compiled.viewportCssVars["--popover"]).toBeTruthy()
          expect(compiled.viewportCssVars["--muted"]).toBeTruthy()
          expect(compiled.viewportCssVars["--border"]).toBeTruthy()
          expect(compiled.viewportCssVars["--primary"]).toBeTruthy()
          expect(compiled.viewportCssVars["--destructive"]).toBe("#f87171")
          expect(compiled.viewportCssVars["--reader-hover-bg"]).toContain("255")
        } else {
          expect(compiled.isDarkSurface).toBe(false)
          if (preset === "follow_app") {
            expect(compiled.viewportCssVars["--background"]).toBeUndefined()
          } else {
            expect(compiled.viewportCssVars["--background"]).toBeTruthy()
            expect(compiled.viewportCssVars["--primary"]).toBeTruthy()
            expect(compiled.viewportCssVars["--primary-foreground"]).toBe("#ffffff")
          }
        }
      }
    }
  })

  it("generates shiro-font @font-face rules with strict CSS string escaping and supports FontFace fallback", () => {
    const base = getDefaultReaderProfile(false)
    const maliciousName = `Evil"Font\\Name';}</style><script>`
    const customFonts: CustomFontMeta[] = [
      {
        fontId: "font-abc",
        familyName: maliciousName,
        fileName: "evil.otf",
        format: "otf",
        sizeBytes: 12345,
      },
    ]

    const profile = {
      ...base,
      typography: {
        ...base.typography,
        font: { kind: "custom" as const, fontId: "font-abc" },
      },
    }

    const compiled = compileReaderStyle(
      profile,
      "default",
      customFonts,
      "shiro-font://localhost/font/",
      false,
      1280
    )

    expect(compiled.customFontFace).toEqual({
      family: maliciousName,
      fontId: "font-abc",
      url: "shiro-font://localhost/font/font-abc",
      format: "otf",
    })
    const escaped = escapeCssString(maliciousName)
    expect(compiled.chapterCss).toContain("@font-face")
    expect(compiled.chapterCss).toContain(`font-family: "${escaped}";`)
    expect(compiled.chapterCss).toContain(
      `url("shiro-font://localhost/font/font-abc") format("opentype")`
    )

    const fallbackCompiled = compileReaderStyle(
      profile,
      "default",
      customFonts,
      "shiro-font://localhost/font/",
      false,
      1280,
      { useFontFaceFallback: true }
    )
    expect(fallbackCompiled.customFontFace).not.toBeNull()
    expect(fallbackCompiled.usesFontFaceFallback).toBe(true)
    expect(fallbackCompiled.chapterCss).not.toContain("@font-face")
    expect(compiled.usesFontFaceFallback).toBe(false)
  })

  it("protects code/pre/rt under overridePublisherStyles, keeps zero inline padding in scrolled mode, and compiles 3-state columnCount & bilingual rules", () => {
    const base = getDefaultReaderProfile(false)

    // Override publisher styles + scrolled mode
    const scrolledOverride = compileReaderStyle(
      {
        ...base,
        typography: { ...base.typography, overridePublisherStyles: true },
        behavior: { ...base.behavior, flow: "scrolled" },
      },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1280
    )
    expect(scrolledOverride.viewportPaddingInlinePx).toBe(0)
    expect(scrolledOverride.paginatorAttributes.flow).toBe("scrolled")
    expect(scrolledOverride.chapterCss).toContain("padding-inline:")
    expect(scrolledOverride.chapterCss).toContain("box-sizing: border-box;")
    expect(scrolledOverride.chapterCss).toContain(
      "pre, pre *, code, code *, kbd, kbd *, samp, samp *"
    )
    expect(scrolledOverride.chapterCss).toContain("math, math *, svg, svg *")
    expect(scrolledOverride.chapterCss).toContain("rt, rt *, rp, rp *")
    expect(scrolledOverride.chapterCss).toContain(
      'html[data-shiro-trans-mode="original"] [data-shiro-synthetic="translation"]'
    )
    expect(scrolledOverride.chapterCss).toContain(
      'html[data-shiro-trans-mode="bilingual"] [data-shiro-has-translation="ready"]'
    )
    expect(scrolledOverride.chapterCss).toContain("margin-block-end: 0 !important;")
    expect(scrolledOverride.chapterCss).toContain(
      'html[data-shiro-trans-mode="bilingual"] [data-shiro-synthetic="translation"]'
    )
    expect(scrolledOverride.chapterCss).toContain(
      'color: var(--shiro-trans-fg-muted) !important;'
    )
    expect(scrolledOverride.chapterCss).toContain(
      'html[data-shiro-trans-mode="translated_only"] [data-shiro-has-translation="ready"]'
    )
    expect(scrolledOverride.chapterCss).toContain(
      'li[data-shiro-synthetic="translation"]'
    )

    // Dark surface preserves translation colors via :not([data-shiro-synthetic="translation"])
    const darkCompiled = compileReaderStyle(
      { ...base, appearance: { surface: "night_dark" } },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1280
    )
    expect(darkCompiled.chapterCss).toContain(
      'p:not([data-shiro-synthetic="translation"])'
    )

    // columnCount: 1 | auto | 2 (landscape clamp vs portrait un-clamped) and mobile forcing 1
    const col1 = compileReaderStyle(
      { ...base, layout: { ...base.layout, columnCount: 1 } },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1280
    )
    expect(col1.paginatorAttributes.maxColumnCount).toBe("1")
    expect(col1.paginatorAttributes.maxInlineSize).toBe("760px")

    const colAuto = compileReaderStyle(
      { ...base, layout: { ...base.layout, columnCount: "auto" } },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1280
    )
    expect(colAuto.paginatorAttributes.maxColumnCount).toBe("2")
    expect(colAuto.paginatorAttributes.maxInlineSize).toBe("760px")

    const col2LandscapeNarrow = compileReaderStyle(
      {
        ...base,
        layout: { ...base.layout, columnCount: 2, maxInlineSizePx: 760 },
      },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1000,
      { viewportHeightPx: 700 }
    )
    expect(col2LandscapeNarrow.paginatorAttributes.maxColumnCount).toBe("2")
    expect(col2LandscapeNarrow.paginatorAttributes.maxInlineSize).toBe("499px")

    const col2PortraitNarrow = compileReaderStyle(
      {
        ...base,
        layout: { ...base.layout, columnCount: 2, maxInlineSizePx: 760 },
      },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      800,
      { viewportHeightPx: 1200 }
    )
    expect(col2PortraitNarrow.paginatorAttributes.maxColumnCount).toBe("2")
    expect(col2PortraitNarrow.paginatorAttributes.maxInlineSize).toBe("760px")

    const mobileCol2 = compileReaderStyle(
      { ...base, layout: { ...base.layout, columnCount: 2 } },
      "default",
      [],
      "shiro-font://localhost/font/",
      true,
      1000
    )
    expect(mobileCol2.paginatorAttributes.maxColumnCount).toBe("1")

    // Fixed-layout EPUB always coerces effective flow to paginated with zero inline padding
    const fxlWithScrolledPref = compileReaderStyle(
      { ...base, behavior: { ...base.behavior, flow: "scrolled" } },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1280,
      { isFixedLayout: true }
    )
    expect(fxlWithScrolledPref.paginatorAttributes.flow).toBe("paginated")
    expect(fxlWithScrolledPref.viewportPaddingInlinePx).toBe(0)

    // Mobile inline margin scales through full 8..120px range without a 96..120px dead zone
    const mobileMargin96 = compileReaderStyle(
      { ...base, layout: { ...base.layout, pageMarginInlinePx: 96 } },
      "default",
      [],
      "shiro-font://localhost/font/",
      true,
      400
    )
    const mobileMargin120 = compileReaderStyle(
      { ...base, layout: { ...base.layout, pageMarginInlinePx: 120 } },
      "default",
      [],
      "shiro-font://localhost/font/",
      true,
      400
    )
    expect(mobileMargin96.viewportPaddingInlinePx).toBe(48)
    expect(mobileMargin120.viewportPaddingInlinePx).toBe(60)

    // Bilingual mode collapses original block bottom margin to 0 so --shiro-bilingual-inner-gap controls pair gap
    const prevTransMode = document.documentElement.getAttribute("data-shiro-trans-mode")
    const bilingualStyle = document.createElement("style")
    bilingualStyle.textContent = col1.chapterCss
    const bilingualHost = document.createElement("div")
    bilingualHost.innerHTML = `<p id="orig-translated" data-shiro-has-translation="ready">Hello</p><p id="orig-streaming" data-shiro-has-translation="streaming">Streaming</p><p id="synth" data-shiro-synthetic="translation">你好</p><p id="orig-plain">Plain</p>`
    document.documentElement.setAttribute("data-shiro-trans-mode", "bilingual")
    document.head.appendChild(bilingualStyle)
    document.body.appendChild(bilingualHost)
    try {
      expect(getComputedStyle(bilingualHost.querySelector("#orig-translated")!).marginBlockEnd).toBe("0px")
      expect(getComputedStyle(bilingualHost.querySelector("#orig-streaming")!).marginBlockEnd).toBe("0px")
      // 18px * 0.75em = 13.5px
      expect(getComputedStyle(bilingualHost.querySelector("#orig-plain")!).marginBlockEnd).toBe("13.5px")
    } finally {
      bilingualStyle.remove()
      bilingualHost.remove()
      if (prevTransMode === null) {
        document.documentElement.removeAttribute("data-shiro-trans-mode")
      } else {
        document.documentElement.setAttribute("data-shiro-trans-mode", prevTransMode)
      }
    }
  })

  it("ensures overridePublisherStyles beats publisher !important without clobbering code weights, svg/math fonts, or ruby alignment", () => {
    const base = getDefaultReaderProfile(false)
    const compiled = compileReaderStyle(
      {
        ...base,
        typography: {
          ...base.typography,
          font: { kind: "preset", preset: "serif" },
          fontWeight: 400,
          lineHeight: 2,
          letterSpacingEm: 0.12,
          wordSpacingEm: 0.2,
          textAlign: "justify",
          overridePublisherStyles: true,
        },
      },
      "default",
      [],
      "shiro-font://localhost/font/",
      false,
      1280
    )

    expect(compiled.chapterCss).toContain("body:not([data-shiro-synthetic=\"translation\"]),")
    expect(compiled.chapterCss).not.toContain(":where(body,")
    expect(compiled.chapterCss).not.toContain("font-family: initial !important;")

    const publisherStyleEl = document.createElement("style")
    publisherStyleEl.textContent = `
      p { line-height: 1.2 !important; }
      .kw { font-weight: 700; }
      svg text { font-family: Georgia; }
      rt { font-family: "RubyFont"; }
    `
    const styleEl = document.createElement("style")
    styleEl.textContent = compiled.chapterCss
    const host = document.createElement("div")
    host.innerHTML = `<p id="prose"><span id="prose-span">Prose</span></p><pre><code><span id="code-span" class="kw">const</span></code></pre><svg><text id="svg-text">SVG</text></svg><math><mi id="math-mi">x</mi></math><ruby>漢<rt id="rt-el"><span id="rt-span">かん</span></rt></ruby>`
    document.head.appendChild(publisherStyleEl)
    document.head.appendChild(styleEl)
    document.body.appendChild(host)
    try {
      const prose = host.querySelector("#prose")!
      const proseSpan = host.querySelector("#prose-span")!
      const codeSpan = host.querySelector("#code-span")!
      const svgText = host.querySelector("#svg-text")!
      const mathMi = host.querySelector("#math-mi")!
      const rtEl = host.querySelector("#rt-el")!
      const rtSpan = host.querySelector("#rt-span")!
      const proseComputed = getComputedStyle(proseSpan)
      const codeComputed = getComputedStyle(codeSpan)
      const svgComputed = getComputedStyle(svgText)
      const mathComputed = getComputedStyle(mathMi)
      const rtElComputed = getComputedStyle(rtEl)
      const rtComputed = getComputedStyle(rtSpan)

      // User override beats publisher `p { line-height: 1.2 !important }` (18px * 2 = 36px)
      expect(getComputedStyle(prose).lineHeight).toBe("36px")
      expect(proseComputed.textAlign).toBe("justify")
      expect(proseComputed.fontWeight).toBe("400")
      // Code syntax highlight bold weight & monospace preserved
      expect(codeComputed.fontFamily).toContain("monospace")
      expect(codeComputed.fontWeight).toBe("700")
      expect(codeComputed.letterSpacing).toBe("normal")
      expect(codeComputed.textAlign).toBe("start")
      // SVG and Math fonts not forced to `initial !important`
      expect(svgComputed.fontFamily).toBe("Georgia")
      expect(mathComputed.fontFamily).not.toBe("initial")
      // Ruby keeps publisher font-family and center alignment
      expect(rtElComputed.fontFamily).toContain("RubyFont")
      expect(rtComputed.textAlign).toBe("center")
      expect(rtComputed.letterSpacing).toBe("normal")
      // 18px * 1.2 = 21.6px (not overridden by body 2 => 36px)
      expect(rtComputed.lineHeight).toBe("21.6px")
    } finally {
      publisherStyleEl.remove()
      styleEl.remove()
      host.remove()
    }
  })

  it("provides 0ms optimistic updates and 200ms debounced IPC writes in reader-settings-store", async () => {
    vi.useFakeTimers()
    const baseBridge = createMockReaderBridge()
    const updateGlobalSpy = vi.spyOn(baseBridge, "readerUpdateGlobalSettings")
    const setOverrideSpy = vi.spyOn(baseBridge, "readerSetBookOverride")
    setReaderBridgeForTest(baseBridge)

    await ensureReaderSettingsLoaded()

    // Rapid slider changes trigger 0ms optimistic snapshot updates without immediate IPC calls
    updateGlobalReaderField("typography", "fontSizePx", 20)
    updateGlobalReaderField("typography", "fontSizePx", 22)
    updateGlobalReaderField("typography", "fontSizePx", 24)
    expect(getReaderSettingsSnapshot().global.typography.fontSizePx).toBe(24)
    expect(updateGlobalSpy).not.toHaveBeenCalled()

    setBookReaderOverrideField("book-hash-1", "appearance", "surface", "night_dark")
    expect(
      getReaderSettingsSnapshot().bookOverrides["book-hash-1"]?.appearance?.surface
    ).toBe("night_dark")
    expect(setOverrideSpy).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()

    expect(updateGlobalSpy).toHaveBeenCalledTimes(1)
    expect(updateGlobalSpy.mock.calls[0]?.[0].typography.fontSizePx).toBe(24)
    expect(setOverrideSpy).toHaveBeenCalledTimes(1)
    expect(setOverrideSpy).toHaveBeenCalledWith("book-hash-1", {
      appearance: { surface: "night_dark" },
    })
  })

  it("clears pending global state and recovers authoritative snapshot when persistence rejects", async () => {
    vi.useFakeTimers()
    const baseBridge = createMockReaderBridge()
    const authoritativeGlobal = getDefaultReaderProfile(false)
    let shouldFail = true
    const updateGlobalSpy = vi
      .spyOn(baseBridge, "readerUpdateGlobalSettings")
      .mockImplementation(async (profile) => {
        if (shouldFail) {
          throw new Error("simulated IPC failure")
        }
        return {
          ...(await baseBridge.readerGetSettings()),
          global: profile,
        }
      })
    setReaderBridgeForTest(baseBridge)

    await ensureReaderSettingsLoaded()

    updateGlobalReaderField("typography", "fontSizePx", 28)
    expect(getReaderSettingsSnapshot().global.typography.fontSizePx).toBe(28)

    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()
    // Let secondary refreshAuthoritativeSnapshot resolve
    await Promise.resolve()
    await Promise.resolve()

    expect(updateGlobalSpy).toHaveBeenCalledTimes(1)
    expect(getReaderSettingsSnapshot().global.typography.fontSizePx).toBe(
      authoritativeGlobal.typography.fontSizePx
    )

    // Subsequent edit succeeds without replaying stale rejected write
    shouldFail = false
    updateGlobalReaderField("typography", "fontSizePx", 21)
    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()

    expect(updateGlobalSpy).toHaveBeenCalledTimes(2)
    expect(getReaderSettingsSnapshot().global.typography.fontSizePx).toBe(21)
  })

  it("rolls back failed resetBookReaderOverrides, normalizes bookHash, sanitizes pending deleted font refs, and evicts peer font buffers", async () => {
    vi.useFakeTimers()
    const baseBridge = createMockReaderBridge()
    setReaderBridgeForTest(baseBridge)
    await ensureReaderSettingsLoaded()

    // Uppercase hash normalizes to lowercase and rolls back on IPC failure
    setBookReaderOverrideField("BOOK-HASH-UPPER", "typography", "fontSizePx", 26)
    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()
    expect(
      getReaderSettingsSnapshot().bookOverrides["book-hash-upper"]?.typography?.fontSizePx
    ).toBe(26)

    const resetSpy = vi
      .spyOn(baseBridge, "readerResetBookOverride")
      .mockRejectedValueOnce(new Error("simulated reset failure"))

    await expect(resetBookReaderOverrides("BOOK-HASH-UPPER")).rejects.toThrow(
      "simulated reset failure"
    )
    await Promise.resolve()
    await Promise.resolve()
    expect(resetSpy).toHaveBeenCalledWith("book-hash-upper")
    expect(
      getReaderSettingsSnapshot().bookOverrides["book-hash-upper"]?.typography?.fontSizePx
    ).toBe(26)

    // Custom font deletion sanitizes pending debounced custom font references & evicts cached buffer
    const fontId = "font-del-1"
    const baseSnap = await baseBridge.readerGetSettings()
    vi.spyOn(baseBridge, "readerPickAndImportFont").mockResolvedValueOnce({
      ...baseSnap,
      customFonts: [
        {
          fontId,
          familyName: "Mock Custom",
          fileName: "mock.ttf",
          format: "ttf",
          sizeBytes: 8,
        },
      ],
    })
    const readFontSpy = vi
      .spyOn(baseBridge, "readerReadCustomFont")
      .mockResolvedValue(new Uint8Array([1, 2, 3, 4]).buffer)
    vi.spyOn(baseBridge, "readerDeleteCustomFont").mockResolvedValueOnce({
      ...baseSnap,
      customFonts: [],
    })

    await pickAndImportCustomFont()
    await loadSharedCustomFontBuffer(
      fontId,
      `shiro-font://localhost/font/${fontId}`,
      baseBridge
    )
    expect(readFontSpy).toHaveBeenCalledTimes(1)

    // Queue a debounced global write referencing fontId, then delete fontId before timer fires
    updateGlobalReaderField("typography", "font", { kind: "custom", fontId })
    await deleteCustomFont(fontId)
    expect(getReaderSettingsSnapshot().global.typography.font).toEqual({
      kind: "publisher",
    })

    // Cache must have been evicted so a subsequent load re-queries the bridge
    await loadSharedCustomFontBuffer(
      fontId,
      `shiro-font://localhost/font/${fontId}`,
      baseBridge
    )
    expect(readFontSpy).toHaveBeenCalledTimes(2)
  })

  it("prevents stale in-flight commitGlobalSettings from resurrecting a deleted font and shields resetBookReaderOverrides from push events", async () => {
    vi.useFakeTimers()
    const baseBridge = createMockReaderBridge()
    const initialSnap = await baseBridge.readerGetSettings()
    const deletedFontId = "font-race-1"
    const withFontSnap = {
      ...initialSnap,
      customFonts: [
        {
          fontId: deletedFontId,
          familyName: "RaceFont",
          fileName: "race.ttf",
          format: "ttf" as const,
          sizeBytes: 16,
        },
      ],
    }

    const pushSettingsRef: {
      current: ((incoming: typeof initialSnap) => void) | null
    } = { current: null }
    vi.spyOn(baseBridge, "listenReaderSettings").mockImplementation(
      async (handler) => {
        pushSettingsRef.current = handler
        return () => {
          pushSettingsRef.current = null
        }
      }
    )

    let resolveStaleGlobal!: (snap: typeof initialSnap) => void
    vi.spyOn(baseBridge, "readerUpdateGlobalSettings").mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveStaleGlobal = resolve
        })
    )

    let serverFonts = [...withFontSnap.customFonts]
    const getSettingsSpy = vi
      .spyOn(baseBridge, "readerGetSettings")
      .mockImplementation(async () => ({
        ...initialSnap,
        customFonts: serverFonts,
      }))
    vi.spyOn(baseBridge, "readerDeleteCustomFont").mockImplementation(async () => {
      serverFonts = []
      return {
        ...initialSnap,
        customFonts: [],
      }
    })

    setReaderBridgeForTest(baseBridge)
    await ensureReaderSettingsLoaded()
    expect(getReaderSettingsSnapshot().customFonts).toHaveLength(1)

    // Start debounced global settings write that resolves AFTER deleteCustomFont completes.
    updateGlobalReaderField("typography", "fontSizePx", 23)
    await vi.advanceTimersByTimeAsync(200)
    await deleteCustomFont(deletedFontId)
    expect(getReaderSettingsSnapshot().customFonts).toHaveLength(0)

    // Stale commitGlobalSettings resolves carrying the deleted font -> must NOT resurrect it.
    resolveStaleGlobal({
      ...withFontSnap,
      global: {
        ...withFontSnap.global,
        typography: { ...withFontSnap.global.typography, fontSizePx: 23 },
      },
    })
    await Promise.resolve()
    await Promise.resolve()
    expect(getSettingsSpy).toHaveBeenCalledTimes(2)
    expect(getReaderSettingsSnapshot().customFonts).toHaveLength(0)

    // In-flight resetBookReaderOverrides holds null sentinel so peer push events do not flash stale override.
    setBookReaderOverrideField("race-book", "typography", "fontSizePx", 25)
    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()

    let resolveReset!: (snap: typeof initialSnap) => void
    vi.spyOn(baseBridge, "readerResetBookOverride").mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveReset = resolve
        })
    )
    const resetPromise = resetBookReaderOverrides("race-book")
    expect(getReaderSettingsSnapshot().bookOverrides["race-book"]).toBeUndefined()

    pushSettingsRef.current?.({
      ...initialSnap,
      customFonts: [],
      bookOverrides: {
        "race-book": { typography: { fontSizePx: 25 } },
      },
    })
    expect(getReaderSettingsSnapshot().bookOverrides["race-book"]).toBeUndefined()

    resolveReset({ ...initialSnap, customFonts: [], bookOverrides: {} })
    await resetPromise
    expect(getReaderSettingsSnapshot().bookOverrides["race-book"]).toBeUndefined()
  })
})
