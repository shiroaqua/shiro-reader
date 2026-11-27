import { createElement, useState } from "react"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { SelectionToolbar } from "@shiro/ui-shared/features/reader/components/selection-toolbar"
import type { EngineCapabilities } from "@shiro/ui-shared/features/reader/contracts/engine"
import type {
  SelectionActionPlugin,
  SelectionContext,
} from "@shiro/ui-shared/features/reader/contracts/selection"
import { SelectionActionRegistry } from "@shiro/ui-shared/features/reader/selection/action-registry"
import {
  copyActionPlugin,
  writeTextToClipboard,
} from "@shiro/ui-shared/features/reader/selection/plugins/copy-action"
import { SelectionObserver } from "@shiro/ui-shared/features/reader/selection/selection-observer"

const BASE_CAPS: EngineCapabilities = {
  supportsReflowTypography: true,
  supportsPublisherStyleOverride: true,
  supportsFlowSwitch: true,
  supportsColumnSwitch: true,
  supportsTextSelection: true,
  supportsToc: true,
  supportsFixedZoom: false,
  supportsFullTextTranslation: true,
  supportsBilingualInterleave: true,
}

function makeRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  }
}

function createSelectionTestDoc(text: string): {
  readonly doc: Document
  readonly p: HTMLElement
  setSelectionState: (collapsed: boolean, value: string) => void
  removeAllRangesSpy: ReturnType<typeof vi.fn>
} {
  const doc = new DOMParser().parseFromString(
    `<?xml version="1.0" encoding="utf-8"?>
     <html xmlns="http://www.w3.org/1999/xhtml">
       <body><p id="p-1">${text}</p></body>
     </html>`,
    "application/xhtml+xml"
  )
  const p = doc.getElementById("p-1")!
  const range = doc.createRange()
  range.setStart(p.firstChild!, 0)
  range.setEnd(p.firstChild!, Math.min(text.length, 10))
  range.getClientRects = () => [makeRect(200, 180, 120, 22)] as unknown as DOMRectList
  range.getBoundingClientRect = () => makeRect(200, 180, 120, 22)

  let isCollapsed = true
  let currentText = ""
  const removeAllRangesSpy = vi.fn(() => {
    isCollapsed = true
    currentText = ""
  })

  Object.defineProperty(doc, "defaultView", {
    value: {
      frameElement: null,
      getSelection: () => ({
        get isCollapsed() {
          return isCollapsed
        },
        get rangeCount() {
          return isCollapsed ? 0 : 1
        },
        toString: () => currentText,
        getRangeAt: () => range,
        removeAllRanges: removeAllRangesSpy,
      }),
    },
    configurable: true,
  })

  return {
    doc,
    p,
    setSelectionState: (collapsed, value) => {
      isCollapsed = collapsed
      currentText = value
    },
    removeAllRangesSpy,
  }
}

describe("SelectionObserver, SelectionToolbar, and copyActionPlugin", () => {
  it("ignores collapsed carets, hides while dragging, renders Copy on release, preserves selection on mousedown, and copies text", async () => {
    const writeTextMock = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: writeTextMock },
      configurable: true,
    })

    const container = document.createElement("div")
    container.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)

    const { doc, setSelectionState, removeAllRangesSpy } = createSelectionTestDoc(
      "Hello ShiroReader selection world"
    )

    const registry = new SelectionActionRegistry([copyActionPlugin])
    let observer!: SelectionObserver

    function Harness() {
      const [selection, setSelection] = useState<SelectionContext | null>(null)
      if (!observer) {
        observer = new SelectionObserver({
          tabId: "tab-1",
          bookId: "book-1",
          bookHash: "hash-1",
          format: "EPUB",
          getContainer: () => container,
          getSectionHref: () => "ch1.xhtml",
          onSelectionChange: setSelection,
        })
        observer.attachSection(0, doc, "epubcfi(/6/2[ch1]!)")
      }
      return createElement(SelectionToolbar, {
        selection,
        capabilities: BASE_CAPS,
        registry,
      })
    }

    render(createElement(Harness))

    // 1. Collapsed selection (e.g. paginator anchor jump) must NOT open toolbar
    setSelectionState(true, "")
    doc.dispatchEvent(new Event("pointerup"))
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument()

    // 2. While pointer is down (dragging), selectionchange must NOT flash toolbar
    doc.dispatchEvent(new Event("pointerdown"))
    setSelectionState(false, "Hello ShiroReader")
    doc.dispatchEvent(new Event("selectionchange"))
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument()

    // 3. On pointerup, toolbar appears with Copy action
    doc.dispatchEvent(new Event("pointerup"))
    const copyButton = await screen.findByRole("button", { name: /Copy|复制/i })
    expect(copyButton).toBeInTheDocument()

    // 4. mousedown on button must preventDefault so iframe selection is not lost
    const mousedownEvent = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
    })
    copyButton.dispatchEvent(mousedownEvent)
    expect(mousedownEvent.defaultPrevented).toBe(true)
    expect(removeAllRangesSpy).not.toHaveBeenCalled()

    // 5. Clicking Copy writes clipboard and calls clearSelection()
    fireEvent.click(copyButton)
    await waitFor(() => {
      expect(writeTextMock).toHaveBeenCalledWith("Hello ShiroReader")
      expect(removeAllRangesSpy).toHaveBeenCalledTimes(1)
    })
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument()

    observer.dispose()
  })

  it("supports custom plugins returning open_subpanel to render inline secondary panels", async () => {
    const customPlugin: SelectionActionPlugin = {
      id: "ext.note",
      group: "annotation",
      order: 20,
      labelKey: "local.reader.selection.copy",
      renderIcon: () => createElement("span", null, "N"),
      execute: async () => ({ outcome: "open_subpanel" }),
      renderSubpanel: (ctx, controls) =>
        createElement(
          "div",
          null,
          createElement("span", null, `Note for: ${ctx.text}`),
          createElement("input", {
            "aria-label": "Note input",
            type: "text",
          }),
          createElement(
            "button",
            { type: "button", onClick: () => controls.collapseSubpanel() },
            "Collapse"
          )
        ),
    }

    const registry = new SelectionActionRegistry([copyActionPlugin, customPlugin])
    const clearSpy = vi.fn()
    const selection: SelectionContext = {
      tabId: "tab-1",
      bookId: "book-1",
      bookHash: "hash-1",
      format: "EPUB",
      sectionIndex: 0,
      locator: null,
      textOrigin: "original",
      text: "Selected excerpt",
      anchorBlockOriginalText: null,
      anchor: {
        preferredPlacement: "top",
        anchorX: 300,
        topY: 200,
        bottomY: 240,
      },
      clearSelection: clearSpy,
    }

    render(
      createElement(SelectionToolbar, {
        selection,
        capabilities: BASE_CAPS,
        registry,
        viewportCssVars: {
          "--reader-chrome-bg": "#1a1a1a",
          "--reader-fg": "#f5f5f5",
        },
        isDarkSurface: true,
      })
    )

    const toolbar = screen.getByRole("toolbar")
    expect(toolbar).toHaveAttribute("data-dark-surface", "true")
    expect(toolbar.style.getPropertyValue("--reader-chrome-bg")).toBe("#1a1a1a")
    expect(toolbar.style.getPropertyValue("--reader-fg")).toBe("#f5f5f5")

    const buttons = screen.getAllByRole("button")
    expect(buttons).toHaveLength(2)

    fireEvent.click(buttons[1]!)
    const subpanel = await screen.findByTestId("selection-toolbar-subpanel")
    expect(subpanel).toHaveTextContent("Note for: Selected excerpt")

    // mousedown on subpanel input must NOT be preventDefaulted so typing/focus works
    const noteInput = screen.getByRole("textbox", { name: "Note input" })
    const inputMouseDown = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
    })
    noteInput.dispatchEvent(inputMouseDown)
    expect(inputMouseDown.defaultPrevented).toBe(false)

    fireEvent.click(screen.getByRole("button", { name: "Collapse" }))
    expect(
      screen.queryByTestId("selection-toolbar-subpanel")
    ).not.toBeInTheDocument()

    // Clicking outside the toolbar clears the active selection
    fireEvent.pointerDown(document.body)
    expect(clearSpy).toHaveBeenCalledTimes(1)
  })

  it("preserves toolbar on unrelated sidebar/subpanel scroll and refcounts Android selection menu suppression", async () => {
    const setSuppressSpy = vi.fn()
    const prevBridge = window.__SHIRO_ANDROID_READER__
    window.__SHIRO_ANDROID_READER__ = {
      setSuppressSelectionMenu: setSuppressSpy,
    }

    const container = document.createElement("div")
    container.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)
    const unrelatedAside = document.createElement("aside")
    document.body.appendChild(container)
    document.body.appendChild(unrelatedAside)

    const doc = new DOMParser().parseFromString(
      `<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><body><p id="p1">Scrollable test text</p><input id="chap-input" type="text" /></body></html>`,
      "application/xhtml+xml"
    )
    const p1 = doc.getElementById("p1")!
    const range = doc.createRange()
    range.setStart(p1.firstChild!, 0)
    range.setEnd(p1.firstChild!, 10)
    range.getClientRects = () => [makeRect(100, 200, 120, 20)] as unknown as DOMRectList

    const iframe = document.createElement("iframe")
    Object.defineProperty(iframe, "offsetWidth", { value: 1000 })
    Object.defineProperty(iframe, "offsetHeight", { value: 800 })
    iframe.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)
    Object.defineProperty(doc, "defaultView", {
      value: {
        frameElement: iframe,
        getSelection: () => ({
          isCollapsed: false,
          rangeCount: 1,
          toString: () => "Scrollable",
          getRangeAt: () => range,
          removeAllRanges: vi.fn(),
        }),
      },
    })

    let latestSel: SelectionContext | null = null
    const obsA = new SelectionObserver({
      tabId: "tab-a",
      bookId: "b1",
      bookHash: "h1",
      format: "EPUB",
      getContainer: () => container,
      getSectionHref: () => "ch1.xhtml",
      isAndroid: () => true,
      onSelectionChange: (sel) => {
        latestSel = sel
      },
    })
    obsA.attachSection(0, doc, "/6/2!")
    doc.dispatchEvent(new Event("pointerup"))
    expect(latestSel).not.toBeNull()

    // Scrolling an unrelated sibling <aside> must NOT dismiss the active selection
    unrelatedAside.dispatchEvent(new Event("scroll", { bubbles: false }))
    expect(latestSel).not.toBeNull()

    // Scrolling the viewport container DOES dismiss it
    container.dispatchEvent(new Event("scroll", { bubbles: false }))
    expect(latestSel).toBeNull()

    // Overlapping Android observers: disposing obsA while obsB is active keeps suppression true
    setSuppressSpy.mockClear()
    const obsB = new SelectionObserver({
      tabId: "tab-b",
      bookId: "b2",
      bookHash: "h2",
      format: "EPUB",
      getContainer: () => container,
      getSectionHref: () => "ch1.xhtml",
      isAndroid: () => true,
      onSelectionChange: () => {},
    })
    expect(setSuppressSpy).toHaveBeenLastCalledWith(true)

    obsA.dispose()
    expect(setSuppressSpy).toHaveBeenLastCalledWith(true)

    // Focusing an editable input inside outer doc temporarily disables suppression
    const outerInput = document.createElement("input")
    document.body.appendChild(outerInput)
    outerInput.focus()
    outerInput.dispatchEvent(new FocusEvent("focusin", { bubbles: true }))
    expect(setSuppressSpy).toHaveBeenLastCalledWith(false)
    outerInput.blur()
    outerInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true }))
    await Promise.resolve()
    expect(setSuppressSpy).toHaveBeenLastCalledWith(true)

    // Disposing the last observer restores suppression to false
    obsB.dispose()
    expect(setSuppressSpy).toHaveBeenLastCalledWith(false)

    outerInput.remove()
    container.remove()
    unrelatedAside.remove()
    window.__SHIRO_ANDROID_READER__ = prevBridge
  })

  it("clamps SelectionToolbar horizontally within viewportBounds so sidebars are never overlapped", () => {
    const registry = new SelectionActionRegistry([copyActionPlugin])
    const selection: SelectionContext = {
      tabId: "tab-bounds",
      bookId: "book-1",
      bookHash: "hash-1",
      format: "EPUB",
      sectionIndex: 0,
      text: "Sidebar edge selection",
      textOrigin: "original",
      anchorBlockOriginalText: null,
      locator: {
        kind: "epub",
        spine_index: 0,
        href: "ch1.xhtml",
        target: {
          cfi: "epubcfi(/6/2!/4/2,/1:0,/1:22)",
        },
      },
      anchor: {
        preferredPlacement: "top",
        anchorX: 295, // Very close to left edge of viewport when 288px TOC sidebar is open
        topY: 200,
        bottomY: 230,
      },
      clearSelection: vi.fn(),
    }

    render(
      <SelectionToolbar
        selection={selection}
        capabilities={BASE_CAPS}
        registry={registry}
        viewportBounds={{ left: 288, top: 40, width: 600, height: 700 }}
      />
    )

    const toolbar = screen.getByRole("toolbar")
    const leftPx = Number.parseFloat(toolbar.style.left)
    // Must respect left sidebar bound (288 + EDGE_MARGIN_PX 12 = 300), not window left (0)
    expect(leftPx).toBeGreaterThanOrEqual(300)
  })

  it("debounces selectionchange by 140ms and falls back to hidden textarea execCommand when Clipboard API rejects", async () => {
    vi.useFakeTimers()
    try {
      const container = document.createElement("div")
      container.getBoundingClientRect = () => makeRect(0, 0, 1000, 800)
      const { doc, setSelectionState } = createSelectionTestDoc(
        "Debounced selection excerpt"
      )

      const emitted: Array<SelectionContext | null> = []
      const observer = new SelectionObserver({
        tabId: "tab-debounce",
        bookId: "b1",
        bookHash: "h1",
        format: "EPUB",
        getContainer: () => container,
        getSectionHref: () => "ch1.xhtml",
        onSelectionChange: (sel) => emitted.push(sel),
      })
      observer.attachSection(0, doc, "epubcfi(/6/2!)")

      setSelectionState(false, "Debounced")
      doc.dispatchEvent(new Event("selectionchange"))
      vi.advanceTimersByTime(100)
      expect(emitted).toHaveLength(0)

      vi.advanceTimersByTime(45)
      expect(emitted).toHaveLength(1)
      expect(emitted[0]?.text).toBe("Debounced")

      observer.dispose()
    } finally {
      vi.useRealTimers()
    }

    // Clipboard API rejects -> writeTextToClipboard falls back to temporary textarea + execCommand('copy')
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: vi.fn().mockRejectedValue(new Error("WebView focus denied")),
      },
      configurable: true,
    })
    const prevExecCommand = document.execCommand
    const execCommandSpy = vi.fn(() => true)
    document.execCommand = execCommandSpy

    try {
      await writeTextToClipboard("Fallback clipboard text")
      expect(execCommandSpy).toHaveBeenCalledWith("copy")
      // Temporary off-screen textarea must be cleaned up from DOM
      expect(document.querySelectorAll("textarea")).toHaveLength(0)
    } finally {
      document.execCommand = prevExecCommand
    }
  })
})
