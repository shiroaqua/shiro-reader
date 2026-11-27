import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ReaderViewport } from "@shiro/ui-shared/features/reader/components/reader-viewport"
import type {
  FootnotePayload,
  ReaderDocumentInstance,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import { readerEngineRegistry } from "@shiro/ui-shared/features/reader/engine/engine-registry"
import { UnsupportedFormatError } from "@shiro/ui-shared/features/reader/engine/unsupported-pdf-engine"
import { getDefaultReaderProfile } from "@shiro/ui-shared/features/reader/settings/defaults"
import { compileReaderStyle } from "@shiro/ui-shared/features/reader/settings/style-compiler"
import {
  resolveReaderKeyCommand,
  resolveViewportTapCommand,
} from "@shiro/ui-shared/features/reader/workspace/input-commands"

function makeCompiledStyle() {
  return compileReaderStyle(
    getDefaultReaderProfile(false),
    "default",
    [],
    "shiro-font://localhost/font/",
    false,
    1024
  )
}

function makeFakeInstance(title: string): ReaderDocumentInstance {
  return {
    format: "EPUB",
    capabilities: {
      supportsReflowTypography: true,
      supportsPublisherStyleOverride: true,
      supportsFlowSwitch: true,
      supportsColumnSwitch: true,
      supportsTextSelection: true,
      supportsToc: true,
      supportsFixedZoom: false,
      supportsFullTextTranslation: true,
      supportsBilingualInterleave: true,
    },
    metadata: {
      title,
      author: null,
      language: "en",
      direction: "ltr",
      isFixedLayout: false,
    },
    navigation: { toc: [], linear_units: [], total_weight: 100 },
    applyStyle: vi.fn(),
    setViewState: vi.fn(),
    goLeft: vi.fn().mockResolvedValue(undefined),
    goRight: vi.fn().mockResolvedValue(undefined),
    goPrev: vi.fn().mockResolvedValue(undefined),
    goNext: vi.fn().mockResolvedValue(undefined),
    goToLocator: vi.fn().mockResolvedValue(undefined),
    goToFraction: vi.fn().mockResolvedValue(undefined),
    clearSelection: vi.fn(),
    destroy: vi.fn().mockResolvedValue(undefined),
  }
}

describe("input-commands and ReaderViewport states", () => {
  it("resolves tap zones and keyboard shortcuts accurately across paginated and scrolled flows", () => {
    expect(resolveViewportTapCommand(0.1, "paginated", true)).toBe("go_left")
    expect(resolveViewportTapCommand(0.5, "paginated", true)).toBe("toggle_chrome")
    expect(resolveViewportTapCommand(0.85, "paginated", true)).toBe("go_right")
    expect(resolveViewportTapCommand(Number.NaN, "paginated", true)).toBe("toggle_chrome")
    expect(resolveViewportTapCommand(0.1, "scrolled", true)).toBe("toggle_chrome")
    expect(resolveViewportTapCommand(0.9, "paginated", false)).toBe("toggle_chrome")

    const key = (
      k: string,
      mods: Partial<Record<"ctrlKey" | "metaKey" | "altKey" | "shiftKey", boolean>> = {}
    ) =>
      resolveReaderKeyCommand({
        key: k,
        ctrlKey: mods.ctrlKey ?? false,
        metaKey: mods.metaKey ?? false,
        altKey: mods.altKey ?? false,
        shiftKey: mods.shiftKey ?? false,
      })

    expect(key("ArrowLeft")).toBe("go_left")
    expect(key("ArrowRight")).toBe("go_right")
    expect(key("PageUp")).toBe("go_prev")
    expect(key("PageDown")).toBe("go_next")
    expect(key(" ")).toBe("go_next")
    expect(key(" ", { shiftKey: true })).toBe("go_prev")
    expect(key("Home")).toBe("go_start")
    expect(key("End")).toBe("go_end")
    expect(key("F11")).toBe("toggle_zen")
    expect(key("Escape")).toBe("escape")
    expect(key("c", { ctrlKey: true })).toBe("copy")
    expect(key("C", { metaKey: true })).toBe("copy")
    expect(key("ArrowRight", { ctrlKey: true })).toBe("none")
    expect(key("ArrowRight", { altKey: true })).toBe("none")
    expect(key("ArrowRight", { shiftKey: true })).toBe("none")
  })

  it("renders loading skeleton while opening, shows error alert with working Retry, and routes 3103 UnsupportedFormatError to onUnsupportedFormat", async () => {
    let attempt = 0
    let resolveSecond!: (inst: ReaderDocumentInstance) => void
    const onInstanceReady = vi.fn()
    const openSpy = vi
      .spyOn(readerEngineRegistry, "open")
      .mockImplementation(() => {
        attempt += 1
        if (attempt === 1) {
          return Promise.reject(new Error("Transient disk read error"))
        }
        return new Promise<ReaderDocumentInstance>((resolve) => {
          resolveSecond = resolve
        })
      })

    const { rerender } = render(
      <ReaderViewport
        tabId="vp-tab-1"
        bookId="book-1"
        bookHash={"a".repeat(64)}
        format="EPUB"
        title="Viewport Test"
        compiledStyle={makeCompiledStyle()}
        onInstanceReady={onInstanceReady}
      />
    )

    // Attempt 1 fails -> error alert & Retry button render
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("Transient disk read error")
    const retryBtn = screen.getByRole("button", { name: /Retry|重试/i })

    // Clicking Retry starts attempt 2 -> shows loading skeleton while pending
    fireEvent.click(retryBtn)
    expect(screen.getByTestId("reader-loading-skeleton")).toBeInTheDocument()

    // Resolving attempt 2 clears skeleton and notifies parent
    const fakeInst = makeFakeInstance("Viewport Test")
    await act(async () => {
      resolveSecond(fakeInst)
    })
    await waitFor(() => {
      expect(
        screen.queryByTestId("reader-loading-skeleton")
      ).not.toBeInTheDocument()
    })
    expect(onInstanceReady).toHaveBeenCalledWith(fakeInst)

    // 3103 UnsupportedFormatError routes cleanly to onUnsupportedFormat instead of error alert
    openSpy.mockRejectedValueOnce(new UnsupportedFormatError("PDF"))
    const onUnsupportedFormat = vi.fn()
    rerender(
      <ReaderViewport
        tabId="vp-tab-pdf"
        bookId="book-pdf"
        bookHash={"b".repeat(64)}
        format="PDF"
        title="Unsupported PDF"
        compiledStyle={makeCompiledStyle()}
        onUnsupportedFormat={onUnsupportedFormat}
      />
    )
    await waitFor(() => {
      expect(onUnsupportedFormat).toHaveBeenCalledTimes(1)
    })
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("renders bottom-right page number in paginated mode, percentage in scrolled mode, and hides when disabled", async () => {
    let capturedEvents:
      | Parameters<typeof readerEngineRegistry.open>[1]["events"]
      | null = null
    vi.spyOn(readerEngineRegistry, "open").mockImplementation(
      async (_format, params) => {
        capturedEvents = params.events
        return makeFakeInstance("Page Number Test")
      }
    )

    const baseProfile = getDefaultReaderProfile(false)
    const compileWith = (flow: "paginated" | "scrolled", showPageNumber: boolean) =>
      compileReaderStyle(
        {
          ...baseProfile,
          behavior: {
            ...baseProfile.behavior,
            flow,
            showPageNumber,
          },
        },
        "default",
        [],
        "shiro-font://localhost/font/",
        false,
        1024
      )

    const { rerender } = render(
      <ReaderViewport
        tabId="vp-page-num"
        bookId="book-page-num"
        bookHash={"c".repeat(64)}
        format="EPUB"
        title="Page Number Test"
        compiledStyle={compileWith("paginated", true)}
      />
    )

    await waitFor(() => {
      expect(capturedEvents).not.toBeNull()
    })

    act(() => {
      capturedEvents?.onRelocate({
        locator: {
          kind: "epub",
          spine_index: 0,
          href: "ch1.xhtml",
          target: { kind: "start" },
        },
        fraction: 0.424,
        sectionIndex: 0,
        sectionTotal: 5,
        activeTocIndex: 0,
        pageStats: { current: 3, total: 12 },
      })
    })

    expect(screen.getByTestId("reader-page-number-overlay")).toHaveTextContent(
      "3 / 12"
    )

    // Scrolled mode shows reading percentage
    rerender(
      <ReaderViewport
        tabId="vp-page-num"
        bookId="book-page-num"
        bookHash={"c".repeat(64)}
        format="EPUB"
        title="Page Number Test"
        compiledStyle={compileWith("scrolled", true)}
      />
    )
    expect(screen.getByTestId("reader-page-number-overlay")).toHaveTextContent(
      "42%"
    )

    // Disabled hides the overlay completely
    rerender(
      <ReaderViewport
        tabId="vp-page-num"
        bookId="book-page-num"
        bookHash={"c".repeat(64)}
        format="EPUB"
        title="Page Number Test"
        compiledStyle={compileWith("paginated", false)}
      />
    )
    expect(
      screen.queryByTestId("reader-page-number-overlay")
    ).not.toBeInTheDocument()
  })

  it("renders fixed-layout zoom controls and EPUB3 footnote popover with jump & Escape dismissal", async () => {
    const setFixedZoom = vi.fn()
    const fixedInstance: ReaderDocumentInstance = {
      ...makeFakeInstance("FXL & Footnote Book"),
      capabilities: {
        ...makeFakeInstance("x").capabilities,
        supportsFixedZoom: true,
      },
      metadata: {
        title: "FXL & Footnote Book",
        author: null,
        language: "en",
        direction: "ltr",
        isFixedLayout: true,
      },
      getFixedZoom: () => "fit-page",
      setFixedZoom,
    }

    let capturedOnFootnote: ((fn: FootnotePayload) => void) | undefined
    const onInternalLink = vi.fn()

    const spy = vi
      .spyOn(readerEngineRegistry, "open")
      .mockImplementation(async (_format, params) => {
        capturedOnFootnote = params.events.onFootnote as typeof capturedOnFootnote
        return fixedInstance
      })

    render(
      <ReaderViewport
        tabId="vp-fxl-fn"
        bookId="book-fxl-fn"
        bookHash={"f".repeat(64)}
        format="EPUB"
        title="FXL & Footnote Book"
        compiledStyle={makeCompiledStyle()}
        onInternalLink={onInternalLink}
      />
    )

    await waitFor(() => {
      expect(screen.getByTestId("reader-fixed-zoom-controls")).toBeInTheDocument()
    })

    // Fixed zoom controls drive setFixedZoom
    const fitToggle = screen.getByRole("button", { name: "Fit width" })
    expect(fitToggle).toHaveTextContent("Fit page")
    fireEvent.click(fitToggle)
    expect(setFixedZoom).toHaveBeenCalledWith("fit-width")
    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }))
    expect(setFixedZoom).toHaveBeenCalledWith(1.25)

    // Footnote popover displays sanitized HTML and jumps on action click
    act(() => {
      capturedOnFootnote?.({
        href: "OEBPS/notes.xhtml#fn-1",
        locator: {
          kind: "epub",
          spine_index: 2,
          href: "OEBPS/notes.xhtml",
          target: { kind: "fragment", fragment: "fn-1" },
        },
        kind: "footnote",
        html: "<p>Detailed explanation here.</p>",
        text: "Detailed explanation here.",
        anchorRect: { left: 100, top: 80, width: 14, height: 18 },
      })
    })

    expect(screen.getByTestId("reader-footnote-popover")).toBeInTheDocument()
    expect(screen.getByTestId("reader-footnote-content")).toHaveTextContent(
      "Detailed explanation here."
    )

    fireEvent.click(screen.getByRole("button", { name: /Jump to note/i }))
    expect(onInternalLink).toHaveBeenCalledWith("OEBPS/notes.xhtml#fn-1", null)
    expect(fixedInstance.goToLocator).toHaveBeenCalledWith({
      kind: "epub",
      spine_index: 2,
      href: "OEBPS/notes.xhtml",
      target: { kind: "fragment", fragment: "fn-1" },
    })
    expect(screen.queryByTestId("reader-footnote-popover")).not.toBeInTheDocument()

    // Escape key dismisses open footnote popover
    act(() => {
      capturedOnFootnote?.({
        href: "OEBPS/notes.xhtml#fn-2",
        locator: {
          kind: "epub",
          spine_index: 2,
          href: "OEBPS/notes.xhtml",
          target: { kind: "fragment", fragment: "fn-2" },
        },
        kind: "footnote",
        html: "<p>Second note.</p>",
        text: "Second note.",
        anchorRect: null,
      })
    })
    expect(screen.getByTestId("reader-footnote-popover")).toBeInTheDocument()
    fireEvent.keyDown(document, { key: "Escape" })
    expect(screen.queryByTestId("reader-footnote-popover")).not.toBeInTheDocument()

    spy.mockRestore()
  })
})
