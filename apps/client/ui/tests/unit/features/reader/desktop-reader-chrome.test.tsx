import { StrictMode } from "react"
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { DesktopReaderApp } from "@shiro/desktop-ui/reader/DesktopReaderApp"
import { DesktopReaderChrome } from "@shiro/desktop-ui/reader/DesktopReaderChrome"
import { setReaderBridgeForTest } from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import type { RelocationSnapshot } from "@shiro/ui-shared/features/reader/contracts/engine"
import { resetReaderSettingsStoreForTest } from "@shiro/ui-shared/features/reader/settings/reader-settings-store"
import { resetErrorNotifications } from "@shiro/ui-shared/lib/error-notifications"
import { readyLocalState } from "../../../support/builders"
import { tauriTestHost } from "../../../support/mock-tauri"
import { renderWithProviders } from "../../../support/render"

let latestViewportProps: {
  readonly title: string
  readonly reattachSessionId?: string | null | undefined
  readonly onRelocate?: ((reloc: RelocationSnapshot) => void) | undefined
  readonly onExternalLink?: ((url: string) => void) | undefined
} | null = null

vi.mock("@shiro/ui-shared/features/reader/components/reader-viewport", () => ({
  ReaderViewport: (props: {
    readonly title: string
    readonly reattachSessionId?: string | null | undefined
    readonly onRelocate?: ((reloc: RelocationSnapshot) => void) | undefined
    readonly onExternalLink?: ((url: string) => void) | undefined
  }) => {
    latestViewportProps = props
    return (
      <div
        data-testid="mock-desktop-viewport"
        data-reattach-session={props.reattachSessionId ?? ""}
      >
        {props.title}
      </div>
    )
  },
}))

describe("DesktopReaderChrome Zen mode & mock preview lifecycle", () => {
  beforeEach(() => {
    latestViewportProps = null
    resetErrorNotifications()
    resetReaderSettingsStoreForTest()
  })

  it("auto-hides Zen top bar after tab context menu closes while cursor is outside and cleans mock window on X close", async () => {
    tauriTestHost.setTauriRuntime(false)
    const bridge = createMockReaderBridge()
    setReaderBridgeForTest(bridge)

    const win = await bridge.readerOpenBook({
      bookId: "b-1",
      bookHash: "a".repeat(64),
      title: "Zen Book",
      format: "EPUB",
      newWindow: true,
    })
    const closeTabSpy = vi.spyOn(bridge, "readerCloseTab")
    const onCloseFallback = vi.fn()

    render(
      <StrictMode>
        <DesktopReaderChrome
          windowId={win.windowId}
          embedded={true}
          onCloseWindowFallback={onCloseFallback}
        />
      </StrictMode>
    )

    await screen.findByTestId("mock-desktop-viewport")
    // Embedded preview must not render a duplicate floating error dock
    expect(document.querySelector("[data-floating-dock]")).toBeNull()

    // Enter Zen Mode via F11 -> header hides
    fireEvent.keyDown(window, { key: "F11" })
    await waitFor(() => {
      expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
    })

    // Hover top 24px reveal zone -> header shows
    const topZone = document.querySelector(".fixed.inset-x-0.top-0.z-40.h-6")!
    fireEvent.mouseEnter(topZone)
    const tab = await screen.findByRole("tab", { name: /Zen Book/i })

    // Open tab context menu, leave header while menu is open, then close menu -> top bar must auto-hide
    fireEvent.contextMenu(tab)
    await screen.findByText(/Move to new window|移至新窗口/i)
    const headerWrap = tab.closest(".fixed.inset-x-0.top-0.z-50")!
    fireEvent.mouseLeave(headerWrap)
    expect(document.querySelector('[role="tablist"]')).not.toBeNull()

    // Escape closes context menu; top bar should now hide instead of staying stuck visible
    fireEvent.keyDown(document, { key: "Escape" })
    await waitFor(() => {
      expect(document.querySelector('[role="tablist"]')).toBeNull()
    })

    // Exit Zen Mode via F11 and click Close Window (X) -> closes mock tabs before fallback
    fireEvent.keyDown(window, { key: "F11" })
    const closeWinBtn = await screen.findByRole("button", {
      name: /Close window|关闭窗口/i,
    })
    fireEvent.click(closeWinBtn)
    await waitFor(() => {
      expect(closeTabSpy).toHaveBeenCalledWith(win.windowId, win.tabs[0]!.tabId)
      expect(onCloseFallback).toHaveBeenCalledTimes(1)
    })
  })

  it("filters DesktopReaderApp alerts to 31xx reader errors with replayBuffered=false and flushes pending tab sync before cross-window move", async () => {
    tauriTestHost.setTauriRuntime(true)
    tauriTestHost.onInvoke("get_bootstrap_state", () =>
      readyLocalState({
        bufferedErrors: [
          {
            operationId: "op-old",
            errorId: "err-old-buffered",
            code: 3104,
            key: "client.reader.corrupted_epub",
            locale: "en",
            message: "Pre-existing buffered reader error",
            itemId: null,
          },
        ],
      })
    )

    const bridge = createMockReaderBridge()
    setReaderBridgeForTest(bridge)

    const win1 = await bridge.readerOpenBook({
      bookId: "b-1",
      bookHash: "a".repeat(64),
      title: "Alpha EPUB",
      format: "EPUB",
      newWindow: true,
    })
    const win2 = await bridge.readerOpenBook({
      bookId: "b-2",
      bookHash: "b".repeat(64),
      title: "Beta EPUB",
      format: "EPUB",
      newWindow: true,
    })
    await bridge.readerUpdateTabState({
      windowId: win1.windowId,
      tabId: win1.tabs[0]!.tabId,
      sessionId: "sess-reattach-1",
    })

    const callOrder: string[] = []
    vi.spyOn(bridge, "readerUpdateTabState").mockImplementation(async (req) => {
      callOrder.push(`sync:${req.tabId}:${req.fraction ?? ""}`)
    })
    const origMove = bridge.readerMoveTabToWindow.bind(bridge)
    vi.spyOn(bridge, "readerMoveTabToWindow").mockImplementation(
      async (src, tid, dst, idx) => {
        callOrder.push(`move:${tid}->${dst}`)
        return origMove(src, tid, dst, idx)
      }
    )

    renderWithProviders(<DesktopReaderApp windowId={win1.windowId} />)

    const vp = await screen.findByTestId("mock-desktop-viewport")
    expect(vp).toHaveAttribute("data-reattach-session", "sess-reattach-1")
    expect(
      screen.queryByText("Pre-existing buffered reader error")
    ).not.toBeInTheDocument()

    // Non-reader 2006 error must be filtered out; 3104 reader error must render.
    act(() => {
      tauriTestHost.emitRustError({
        operationId: "op-1",
        errorId: "err-non-reader",
        code: 2006,
        key: "library.book.not_found",
        locale: "en",
        message: "Bookshelf book not found",
        itemId: null,
      })
      tauriTestHost.emitRustError({
        operationId: "op-2",
        errorId: "err-reader-live",
        code: 3104,
        key: "client.reader.corrupted_epub",
        locale: "en",
        message: "Corrupted EPUB archive alert",
        itemId: null,
      })
    })
    expect(
      await screen.findByText("Corrupted EPUB archive alert")
    ).toBeInTheDocument()
    expect(
      screen.queryByText("Bookshelf book not found")
    ).not.toBeInTheDocument()

    // Queue a debounced relocation update, then immediately move the tab to win2 -> sync must flush BEFORE move.
    act(() => {
      latestViewportProps?.onRelocate?.({
        locator: {
          kind: "epub",
          spine_index: 1,
          href: "ch2.xhtml",
          target: {
            kind: "cfi",
            cfi: "epubcfi(/6/4!/4/2/1:0)",
          },
        },
        fraction: 0.55,
        sectionIndex: 1,
        sectionTotal: 3,
        activeTocIndex: 1,
        pageStats: { current: 2, total: 5 },
      })
    })

    const tab = screen.getByRole("tab", { name: /Alpha EPUB/i })
    fireEvent.contextMenu(tab)
    const moveTarget = await screen.findByText(/Beta EPUB \(1\)/)
    fireEvent.click(moveTarget)

    await waitFor(() => {
      expect(callOrder).toEqual([
        `sync:${win1.tabs[0]!.tabId}:0.55`,
        `move:${win1.tabs[0]!.tabId}->${win2.windowId}`,
      ])
    })
  })

  it("resets Zen hover when entering via top-bar button, hides after Portal menu closes without mouseLeave, and blocks Escape while ExternalLinkConfirmDialog is open", async () => {
    tauriTestHost.setTauriRuntime(false)
    const bridge = createMockReaderBridge()
    setReaderBridgeForTest(bridge)

    const win = await bridge.readerOpenBook({
      bookId: "b-zen-btn",
      bookHash: "c".repeat(64),
      title: "Zen Button Book",
      format: "EPUB",
      newWindow: true,
    })

    render(
      <DesktopReaderChrome
        windowId={win.windowId}
        embedded={true}
      />
    )
    await screen.findByTestId("mock-desktop-viewport")

    // Hover header then click top-bar Zen button -> header must hide cleanly
    const tab = await screen.findByRole("tab", { name: /Zen Button Book/i })
    const initialHeader = tab.closest(".shrink-0")!
    fireEvent.mouseEnter(initialHeader)
    fireEvent.click(
      screen.getByRole("button", { name: /Zen mode|禅模式|专注模式/i })
    )
    await waitFor(() => {
      expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
    })

    // Reveal header, open context menu, and close via Escape WITHOUT firing mouseLeave on header
    const topZone = document.querySelector(".fixed.inset-x-0.top-0.z-40.h-6")!
    fireEvent.mouseEnter(topZone)
    const revealedTab = await screen.findByRole("tab", {
      name: /Zen Button Book/i,
    })
    fireEvent.contextMenu(revealedTab)
    await screen.findByText(/Move to new window|移至新窗口/i)
    fireEvent.keyDown(document, { key: "Escape" })
    await waitFor(() => {
      expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
    })

    // Open ExternalLinkConfirmDialog while in Zen mode -> Escape must NOT exit Zen mode
    act(() => {
      latestViewportProps?.onExternalLink?.("https://example.com/safe")
    })
    expect(
      await screen.findByTestId("external-link-target-url")
    ).toBeInTheDocument()

    fireEvent.keyDown(window, { key: "Escape" })
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument()
  })
})
