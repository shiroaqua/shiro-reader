import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ReaderTabStrip } from "@shiro/desktop-ui/reader/ReaderTabStrip"
import type {
  OtherReaderWindowSummary,
  ReaderWindowTabDto,
} from "@shiro/ui-shared/api/reader-bridge"

function makeTab(id: string, title: string): ReaderWindowTabDto {
  return {
    tabId: id,
    bookId: `book-${id}`,
    bookHash: `hash-${id}`,
    title,
    format: "EPUB",
    sessionId: `sess-${id}`,
    currentLocator: null,
    fraction: 0,
    translationMode: "original",
    targetLanguage: null,
  }
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

function renderStrip(overrides: {
  readonly canCrossWindowPointerHitTest?: boolean
  readonly tabs?: readonly ReaderWindowTabDto[]
  readonly otherWindows?: readonly OtherReaderWindowSummary[]
  readonly dropPreview?: {
    readonly windowId: string
    readonly insertIndex: number | null
    readonly draggingTabTitle: string | null
  } | null
} = {}) {
  const tabs = overrides.tabs ?? [
    makeTab("t1", "First Book"),
    makeTab("t2", "Second Book"),
    makeTab("t3", "Third Book"),
  ]
  const handlers = {
    onActivateTab: vi.fn(),
    onReorderTabs: vi.fn(),
    onCloseTab: vi.fn(),
    onCloseOtherTabs: vi.fn(),
    onCloseRightTabs: vi.fn(),
    onMoveTabToWindow: vi.fn(),
    onTearOffTab: vi.fn(),
    onDragStart: vi.fn(),
    onDragOver: vi.fn(),
    onDragCommit: vi.fn(),
    onDragCancel: vi.fn(),
    onReportMetrics: vi.fn(),
    onToggleToc: vi.fn(),
    onToggleSettings: vi.fn(),
    onToggleZen: vi.fn(),
  }

  const utils = render(
    <ReaderTabStrip
      windowId="win-1"
      tabs={tabs}
      activeTabId={tabs[0]?.tabId ?? null}
      otherWindows={overrides.otherWindows ?? []}
      canCrossWindowPointerHitTest={
        overrides.canCrossWindowPointerHitTest ?? true
      }
      dropPreview={overrides.dropPreview ?? null}
      tocOpen={false}
      settingsOpen={false}
      zenMode={false}
      {...handlers}
    />
  )

  const tablist = screen.getByRole("tablist")
  const header = tablist.parentElement!
  header.getBoundingClientRect = () => makeRect(0, 0, 900, 40)

  const tabElements = screen.getAllByRole("tab")
  tabElements.forEach((el, idx) => {
    el.getBoundingClientRect = () => makeRect(idx * 150, 4, 140, 32)
  })

  return { ...utils, handlers, tabElements }
}

describe("use-tab-drag & ReaderTabStrip", () => {
  it("activates tab on < 6px click and reorders in-strip on horizontal drag without cross-window tear-off", () => {
    const { handlers, tabElements } = renderStrip({
      canCrossWindowPointerHitTest: false,
    })

    // < 6px movement -> activates tab 2 without starting reorder
    fireEvent.pointerDown(tabElements[1]!, {
      button: 0,
      pointerId: 1,
      isPrimary: true,
      clientX: 200,
      clientY: 20,
    })
    fireEvent.pointerMove(tabElements[1]!, {
      pointerId: 1,
      clientX: 203,
      clientY: 21,
    })
    fireEvent.pointerUp(tabElements[1]!, {
      button: 0,
      pointerId: 1,
      clientX: 203,
      clientY: 21,
    })

    expect(handlers.onActivateTab).toHaveBeenCalledWith("t2")
    expect(handlers.onReorderTabs).not.toHaveBeenCalled()
    expect(handlers.onTearOffTab).not.toHaveBeenCalled()

    // Horizontal drag inside strip swaps first tab past second tab
    fireEvent.pointerDown(tabElements[0]!, {
      button: 0,
      pointerId: 2,
      isPrimary: true,
      clientX: 50,
      clientY: 20,
    })
    fireEvent.pointerMove(tabElements[0]!, {
      pointerId: 2,
      clientX: 250, // past midpoint of tab 2 (220) -> before tab 3
      clientY: 24,
    })
    fireEvent.pointerUp(tabElements[0]!, {
      button: 0,
      pointerId: 2,
      clientX: 250,
      clientY: 24,
    })
    // Normal pointerup in browser triggers lostpointercapture immediately after -> must NEVER tear off!
    fireEvent(tabElements[0]!, new Event("lostpointercapture"))

    expect(handlers.onReorderTabs).toHaveBeenCalledWith(["t2", "t1", "t3"])
    expect(handlers.onTearOffTab).not.toHaveBeenCalled()
  })

  it("reports coordinates in Windows/X11 cross-window drag and handles Escape cancellation", () => {
    vi.useFakeTimers()
    const { handlers, tabElements } = renderStrip({
      canCrossWindowPointerHitTest: true,
    })

    fireEvent.pointerDown(tabElements[0]!, {
      button: 0,
      pointerId: 3,
      isPrimary: true,
      clientX: 60,
      clientY: 20,
    })
    // Move outside ±36px strip band (strip bottom=40 -> 90 > 76)
    fireEvent.pointerMove(tabElements[0]!, {
      pointerId: 3,
      clientX: 320,
      clientY: 90,
    })
    vi.runAllTimers()

    expect(handlers.onDragStart).toHaveBeenCalledWith("win-1", "t1")
    expect(handlers.onDragOver).toHaveBeenCalledWith("win-1", 320, 90)

    fireEvent.pointerUp(tabElements[0]!, {
      button: 0,
      pointerId: 3,
      clientX: 325,
      clientY: 95,
    })
    expect(handlers.onDragCommit).toHaveBeenCalledWith("win-1", 325, 95)

    // Start another cross-window drag and cancel with Escape
    fireEvent.pointerDown(tabElements[1]!, {
      button: 0,
      pointerId: 4,
      isPrimary: true,
      clientX: 200,
      clientY: 20,
    })
    fireEvent.pointerMove(tabElements[1]!, {
      pointerId: 4,
      clientX: 220,
      clientY: 110,
    })
    fireEvent.keyDown(window, { key: "Escape" })
    expect(handlers.onDragCancel).toHaveBeenCalledWith("win-1")
    expect(handlers.onDragCommit).toHaveBeenCalledTimes(1)
  })

  it("handles Wayland TearOffPreview hysteresis and right-click Move to other window menu", async () => {
    const { handlers, tabElements } = renderStrip({
      canCrossWindowPointerHitTest: false,
      otherWindows: [
        { windowId: "win-2", activeTitle: "Target Book", tabCount: 2 },
      ],
    })

    // 1. Wayland: leave ±36px -> TearOffPreview; return inside band -> cancels preview; leave again + pointerup -> tearOffTab
    fireEvent.pointerDown(tabElements[0]!, {
      button: 0,
      pointerId: 5,
      isPrimary: true,
      clientX: 60,
      clientY: 20,
    })
    fireEvent.pointerMove(tabElements[0]!, {
      pointerId: 5,
      clientX: 80,
      clientY: 100,
    })
    expect(
      screen.getByTestId("reader-tear-off-preview-banner")
    ).toBeInTheDocument()

    // Drag back into tolerance band -> preview disappears
    fireEvent.pointerMove(tabElements[0]!, {
      pointerId: 5,
      clientX: 80,
      clientY: 30,
    })
    expect(
      screen.queryByTestId("reader-tear-off-preview-banner")
    ).not.toBeInTheDocument()

    // Leave band again and release pointerup -> triggers tearOffTab
    fireEvent.pointerMove(tabElements[0]!, {
      pointerId: 5,
      clientX: 90,
      clientY: 110,
    })
    fireEvent.pointerUp(tabElements[0]!, {
      button: 0,
      pointerId: 5,
      clientX: 90,
      clientY: 110,
    })
    expect(handlers.onTearOffTab).toHaveBeenCalledWith("t1")

    // 2. Right-click context menu lists other window and calls onMoveTabToWindow
    fireEvent.contextMenu(tabElements[1]!)
    const moveTarget = await screen.findByText(/Target Book \(2\)/)
    fireEvent.click(moveTarget)
    expect(handlers.onMoveTabToWindow).toHaveBeenCalledWith("t2", "win-2")
  })

  it("exposes full-height titlebar drag regions and suppresses middle-click autoscroll before closing tab", () => {
    const { handlers, tabElements } = renderStrip()
    const tablist = screen.getByRole("tablist")
    const spacer = screen.getByTestId("reader-tab-drag-spacer")

    expect(tablist).toHaveAttribute("data-tauri-drag-region", "")
    expect(tablist.className).toContain("h-full")
    expect(spacer).toHaveAttribute("data-tauri-drag-region", "")
    expect(spacer.className).toContain("h-full")

    // Middle-click mousedown must call preventDefault to suppress WebView2 autoscroll icon.
    const middleDown = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
      button: 1,
    })
    tabElements[0]!.dispatchEvent(middleDown)
    expect(middleDown.defaultPrevented).toBe(true)

    fireEvent(
      tabElements[0]!,
      new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 })
    )
    expect(handlers.onCloseTab).toHaveBeenCalledWith("t1")
  })

  it("guards Wayland single-tab tear-off, cancels drag on blur/pointercancel, renders end drop slot, and scrolls tablist on wheel", () => {
    // 1. Wayland single-tab guard: must NOT show preview banner or trigger tear-off
    const single = renderStrip({
      canCrossWindowPointerHitTest: false,
      tabs: [makeTab("solo", "Solo Book")],
    })
    fireEvent.pointerDown(single.tabElements[0]!, {
      button: 0,
      pointerId: 10,
      isPrimary: true,
      clientX: 60,
      clientY: 20,
    })
    fireEvent.pointerMove(single.tabElements[0]!, {
      pointerId: 10,
      clientX: 80,
      clientY: 120,
    })
    expect(
      screen.queryByTestId("reader-tear-off-preview-banner")
    ).not.toBeInTheDocument()
    fireEvent.pointerUp(single.tabElements[0]!, {
      button: 0,
      pointerId: 10,
      clientX: 80,
      clientY: 120,
    })
    expect(single.handlers.onTearOffTab).not.toHaveBeenCalled()
    single.unmount()

    // 2. Trailing dropPreview slot (insertIndex === tabs.length), wheel horizontal scroll, and blur/pointercancel cancellation
    const multi = renderStrip({
      canCrossWindowPointerHitTest: true,
      dropPreview: {
        windowId: "win-1",
        insertIndex: 3,
        draggingTabTitle: "Incoming Book",
      },
    })
    expect(screen.getByTestId("reader-tab-drop-slot")).toBeInTheDocument()

    const tablist = screen.getByRole("tablist")
    tablist.scrollLeft = 0
    fireEvent.wheel(tablist, { deltaY: 48 })
    expect(tablist.scrollLeft).toBe(48)

    // Window blur cancels active cross-window drag
    fireEvent.pointerDown(multi.tabElements[0]!, {
      button: 0,
      pointerId: 11,
      isPrimary: true,
      clientX: 60,
      clientY: 20,
    })
    fireEvent.pointerMove(multi.tabElements[0]!, {
      pointerId: 11,
      clientX: 120,
      clientY: 120,
    })
    fireEvent.blur(window)
    expect(multi.handlers.onDragCancel).toHaveBeenCalledWith("win-1")

    // pointercancel also cancels active cross-window drag
    fireEvent.pointerDown(multi.tabElements[1]!, {
      button: 0,
      pointerId: 12,
      isPrimary: true,
      clientX: 200,
      clientY: 20,
    })
    fireEvent.pointerMove(multi.tabElements[1]!, {
      pointerId: 12,
      clientX: 220,
      clientY: 120,
    })
    fireEvent.pointerCancel(multi.tabElements[1]!, { pointerId: 12 })
    expect(multi.handlers.onDragCancel).toHaveBeenCalledTimes(2)
  })
})
