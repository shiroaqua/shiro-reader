import { act, fireEvent, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  mobileReaderStore,
  resetMobileReaderStoreForTest,
} from "@shiro/mobile-ui/reader/mobile-reader-store"
import {
  readerBridge,
  type ReaderWindowSnapshot,
} from "@shiro/ui-shared/api/reader-bridge"
import type { ReaderDocumentInstance } from "@shiro/ui-shared/features/reader/contracts/engine"
import { readerEngineRegistry } from "@shiro/ui-shared/features/reader/engine/engine-registry"
import { getErrorNotifications } from "@shiro/ui-shared/lib/error-notifications"
import { makeBook } from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { tauriTestHost } from "../support/mock-tauri"
import { renderDesktopApp, renderMobileApp } from "../support/render"

function mockWindowSnapshot(bookId: string, title: string): ReaderWindowSnapshot {
  return {
    windowId: "reader-1",
    windowLabel: "reader-1",
    tabs: [
      {
        tabId: "tab-1",
        bookId,
        bookHash: `hash-${bookId}`,
        title,
        format: "EPUB",
        sessionId: "sess-1",
        currentLocator: null,
        fraction: 0,
        translationMode: "original",
        targetLanguage: null,
      },
    ],
    activeTabId: "tab-1",
    otherWindows: [],
    canCrossWindowPointerHitTest: true,
  }
}

function mockLightReaderEngine() {
  return vi
    .spyOn(readerEngineRegistry, "open")
    .mockImplementation(async (_format, params) => {
      const instance: ReaderDocumentInstance = {
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
          title: params.fallbackTitle,
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
      return instance
    })
}

describe("BookCard reader launch entrypoints and eviction", () => {
  afterEach(() => {
    resetMobileReaderStoreForTest()
  })

  it("opens EPUB on cover click, skips open in selection mode, and supports desktop Open in New Window", async () => {
    const epub1 = makeBook({ id: "epub-1", title: "Tea Book", type: "epub" })
    const epub2 = makeBook({ id: "epub-2", title: "Stone Story", type: "epub" })
    const library = new FakeLibrary().seedBooks([epub1, epub2])
    const openBookSpy = vi
      .spyOn(readerBridge, "readerOpenBook")
      .mockResolvedValue(mockWindowSnapshot("epub-1", "Tea Book"))

    const { user } = renderDesktopApp({ library })

    const heading = await screen.findByRole("heading", { name: "Tea Book" })
    const card1 = heading.closest("li")!

    // Single click on non-selection card opens in active window (newWindow: false)
    await user.click(card1)
    expect(openBookSpy).toHaveBeenCalledTimes(1)
    expect(openBookSpy).toHaveBeenLastCalledWith({
      bookId: "epub-1",
      bookHash: epub1.hash,
      title: "Tea Book",
      format: "EPUB",
      newWindow: false,
    })

    // Enter selection mode -> clicking card2 selects it and does NOT open reader
    await user.click(screen.getByRole("button", { name: 'Select "Tea Book"' }))
    const card2 = screen.getByRole("heading", { name: "Stone Story" }).closest("li")!
    await user.click(card2)
    expect(openBookSpy).toHaveBeenCalledTimes(1)
    expect(screen.getByText("2 selected")).toBeInTheDocument()

    // Exit selection mode -> context menu "Open in New Window" passes newWindow: true (and "Open" is omitted)
    await user.click(screen.getByRole("button", { name: "Clear selection" }))
    fireEvent.contextMenu(card1)
    const openNewWindowItem = await screen.findByText("Open in New Window")
    expect(screen.queryByText("Open")).not.toBeInTheDocument()
    await user.click(openNewWindowItem)

    expect(openBookSpy).toHaveBeenCalledTimes(2)
    expect(openBookSpy).toHaveBeenLastCalledWith({
      bookId: "epub-1",
      bookHash: epub1.hash,
      title: "Tea Book",
      format: "EPUB",
      newWindow: true,
    })
  })

  it("blocks PDF launch with unsupported_format notification without opening a window or mobile tab", async () => {
    const pdfBook = makeBook({ id: "pdf-1", title: "Reference PDF", type: "pdf" })
    const library = new FakeLibrary().seedBooks([pdfBook])
    const openBookSpy = vi.spyOn(readerBridge, "readerOpenBook")

    const { user } = renderDesktopApp({ library })

    const pdfHeading = await screen.findByRole("heading", {
      name: "Reference PDF",
    })
    await user.click(pdfHeading.closest("li")!)

    expect(openBookSpy).not.toHaveBeenCalled()
    expect(
      getErrorNotifications().some(
        (n) =>
          n.translation.source === "local" &&
          n.translation.key === "local.reader.error.unsupported_format"
      )
    ).toBe(true)
    expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
  })

  it("omits Open and Open in Background Tab from mobile ActionSheet and evicts deleted books", async () => {
    mockLightReaderEngine()
    const epubCloseSpy = vi
      .spyOn(readerBridge, "epubClose")
      .mockResolvedValue(undefined)
    tauriTestHost.setPlatform("android")
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => undefined)

    const b1 = makeBook({ id: "mob-1", title: "Moonlight", type: "epub" })
    const b2 = makeBook({ id: "mob-2", title: "Starlight", type: "epub" })
    const library = new FakeLibrary().seedBooks([b1, b2])

    const { user } = renderMobileApp({ library })
    const h1 = await screen.findByRole("heading", { name: "Moonlight" })
    const card1 = h1.closest("li")!

    // Long press on touch card -> ActionSheet omits Open, Open in Background Tab, and Open in New Window
    vi.useFakeTimers()
    act(() => {
      fireEvent.pointerDown(card1, {
        pointerType: "touch",
        clientX: 40,
        clientY: 40,
      })
      vi.advanceTimersByTime(420)
      fireEvent.pointerUp(card1, {
        pointerType: "touch",
        clientX: 40,
        clientY: 40,
      })
      fireEvent.click(card1)
    })
    vi.useRealTimers()

    await screen.findByRole("button", { name: "Rename" })
    expect(
      screen.queryByRole("button", { name: "Open" })
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: "Open in Background Tab" })
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: "Open in New Window" })
    ).not.toBeInTheDocument()

    await user.keyboard("{Escape}")
    await waitFor(() => {
      expect(
        screen.queryByRole("button", { name: "Rename" })
      ).not.toBeInTheDocument()
    })

    // Seed two open mobile reader tabs in the background and give them live sessionIds to verify delete eviction calls epubClose
    act(() => {
      mobileReaderStore.openBook(b1)
      mobileReaderStore.openBook(b2)
      mobileReaderStore.closeOverlay()
    })
    const tab1 = mobileReaderStore.getSnapshot().tabs[0]!
    const tab2 = mobileReaderStore.getSnapshot().tabs[1]!
    act(() => {
      mobileReaderStore.updateSessionId(tab1.tabId, "sess-mob-1")
      mobileReaderStore.updateSessionId(tab2.tabId, "sess-mob-2")
    })
    expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(2)

    // Delete b1 via selection bar -> evicts tab1 and closes sess-mob-1
    await user.click(screen.getByRole("button", { name: 'Select "Moonlight"' }))
    await user.click(screen.getByRole("button", { name: "Delete" }))

    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs.map((t) => t.bookId)).toEqual([
        "mob-2",
      ])
    })
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-mob-1")

    // Batch-style delete b2 -> clears remaining tab and hides Continue Reading pill
    await user.click(screen.getByRole("button", { name: 'Select "Starlight"' }))
    await user.click(screen.getByRole("button", { name: "Delete" }))

    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
    })
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-mob-2")
    expect(
      screen.queryByTestId("mobile-continue-reading")
    ).not.toBeInTheDocument()
  })

  it("evicts mobile reader tabs on TrashList permanent delete and exposes Open in New Window in touch ActionSheet", async () => {
    mockLightReaderEngine()
    const epubCloseSpy = vi
      .spyOn(readerBridge, "epubClose")
      .mockResolvedValue(undefined)
    tauriTestHost.setPlatform("android")
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => undefined)

    const trashedBook = makeBook({
      id: "trash-evict-1",
      title: "Trashed Novel",
      type: "epub",
    })
    const library = new FakeLibrary().seedTrash([
      {
        ...trashedBook,
        deleted_at: "2026-01-02T00:00:00Z",
      },
    ])

    // Seed an active mobile reader tab for the trashed book
    act(() => {
      mobileReaderStore.openBook(trashedBook)
      mobileReaderStore.closeOverlay()
      const tab = mobileReaderStore.getSnapshot().tabs[0]!
      mobileReaderStore.updateSessionId(tab.tabId, "sess-trash-1")
    })
    expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(1)

    const { user, unmount } = renderMobileApp({ library })

    // Navigate to Trash via bottom MobileNav and permanently delete the book
    await user.click(
      await screen.findByRole("button", { name: /^(Trash|回收站)$/i })
    )
    await screen.findByRole("heading", { name: "Trashed Novel" })
    await user.click(
      screen.getByRole("button", {
        name: /^(Permanently delete|彻底删除)$/i,
      })
    )
    const confirmBtns = await screen.findAllByRole("button", {
      name: /^(Permanently delete|彻底删除)$/i,
    })
    await user.click(confirmBtns[confirmBtns.length - 1]!)

    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
    })
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-trash-1")
    unmount()

    // Desktop touch-primary tablet (coarse pointer) long press exposes Open in New Window
    tauriTestHost.setPlatform("windows")
    const prevMatchMedia = window.matchMedia
    window.matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: query.includes("coarse"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }))
    try {
      const desktopBook = makeBook({
        id: "tablet-1",
        title: "Tablet EPUB",
        type: "epub",
      })
      const desktopLib = new FakeLibrary().seedBooks([desktopBook])
      const openBookSpy = vi
        .spyOn(readerBridge, "readerOpenBook")
        .mockResolvedValue(mockWindowSnapshot("tablet-1", "Tablet EPUB"))

      const { user: desktopUser } = renderDesktopApp({ library: desktopLib })
      const heading = await screen.findByRole("heading", { name: "Tablet EPUB" })
      const card = heading.closest("li")!

      vi.useFakeTimers()
      act(() => {
        fireEvent.pointerDown(card, {
          pointerType: "touch",
          clientX: 30,
          clientY: 30,
        })
        vi.advanceTimersByTime(420)
        fireEvent.pointerUp(card, {
          pointerType: "touch",
          clientX: 30,
          clientY: 30,
        })
        fireEvent.click(card)
      })
      vi.useRealTimers()

      const openNewWinBtn = await screen.findByRole("button", {
        name: "Open in New Window",
      })
      expect(
        screen.queryByRole("button", { name: "Open" })
      ).not.toBeInTheDocument()
      expect(
        screen.queryByRole("button", { name: "Open in Background Tab" })
      ).not.toBeInTheDocument()
      await desktopUser.click(openNewWinBtn)
      expect(openBookSpy).toHaveBeenCalledWith({
        bookId: "tablet-1",
        bookHash: desktopBook.hash,
        title: "Tablet EPUB",
        format: "EPUB",
        newWindow: true,
      })
    } finally {
      window.matchMedia = prevMatchMedia
    }
  })
})
