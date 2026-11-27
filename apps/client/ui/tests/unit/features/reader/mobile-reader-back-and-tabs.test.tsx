import { act, screen, waitFor, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { MobileOverlayHost } from "@shiro/mobile-ui/MobileOverlayHost"
import { MobileReaderOverlay } from "@shiro/mobile-ui/reader/MobileReaderOverlay"
import {
  mobileReaderStore,
  resetMobileReaderStoreForTest,
} from "@shiro/mobile-ui/reader/mobile-reader-store"
import {
  readerBridge,
  setReaderBridgeForTest,
} from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import type {
  BookLocator,
  OpenDocumentParams,
  ReaderDocumentInstance,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import type { SelectionContext } from "@shiro/ui-shared/features/reader/contracts/selection"
import { readerEngineRegistry } from "@shiro/ui-shared/features/reader/engine/engine-registry"
import {
  BACK_PRIORITY,
  dispatchBack,
  registerBackHandler,
} from "@shiro/ui-shared/lib/back-navigation"
import {
  cloudAuthRequiredState,
  makeBook,
  makeCloudStatus,
  readyCloudState,
  readyLocalState,
} from "../../../support/builders"
import { tauriTestHost } from "../../../support/mock-tauri"
import { renderWithProviders } from "../../../support/render"

interface OpenedEngineRecord {
  readonly params: OpenDocumentParams
  readonly instance: ReaderDocumentInstance
}

function setupMockReaderEngine() {
  const opened: OpenedEngineRecord[] = []

  const openSpy = vi
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
          author: "Test Author",
          language: "en",
          direction: "ltr",
          isFixedLayout: false,
        },
        navigation: {
          toc: [],
          linear_units: [],
          total_weight: 100,
        },
        getSessionId: () => params.reattachSessionId ?? null,
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
      opened.push({ params, instance })
      return instance
    })

  return { opened, openSpy }
}

function makeSelectionContext(clearSelection: () => void): SelectionContext {
  return {
    tabId: "tab-1",
    bookId: "book-1",
    bookHash: "hash-1",
    format: "EPUB",
    sectionIndex: 0,
    locator: null,
    textOrigin: "original",
    text: "Sample quote",
    anchorBlockOriginalText: null,
    anchor: {
      preferredPlacement: "top",
      anchorX: 160,
      topY: 120,
      bottomY: 140,
    },
    clearSelection,
  }
}

describe("MobileReaderOverlay back stack and multi-tab lifecycle", () => {
  beforeEach(() => {
    setReaderBridgeForTest(createMockReaderBridge())
  })

  afterEach(() => {
    resetMobileReaderStoreForTest()
    delete window.__SHIRO_ANDROID_READER__
  })

  it("consumes Android back strictly in order: open Sheet -> active selection -> reader overlay -> bookshelf handlers", async () => {
    const { opened } = setupMockReaderEngine()
    const setSystemUi = vi.fn()
    const setSuppressMenu = vi.fn()
    window.__SHIRO_ANDROID_READER__ = {
      setSuppressSelectionMenu: setSuppressMenu,
      setReaderSystemUi: setSystemUi,
    }

    const searchHandler = vi.fn(() => true)
    const selectionHandler = vi.fn(() => true)
    const navigationHandler = vi.fn(() => true)
    const unsubSearch = registerBackHandler(BACK_PRIORITY.search, searchHandler)
    const unsubSelection = registerBackHandler(
      BACK_PRIORITY.selection,
      selectionHandler
    )
    const unsubNavigation = registerBackHandler(
      BACK_PRIORITY.navigation,
      navigationHandler
    )

    try {
      const book = makeBook({ id: "m-1", title: "Mobile Book One", type: "epub" })
      const { user } = renderWithProviders(<MobileReaderOverlay />)

      act(() => {
        mobileReaderStore.openBook(book)
      })

      await waitFor(() => {
        expect(opened).toHaveLength(1)
      })
      expect(setSystemUi).toHaveBeenLastCalledWith(true, true, false, expect.any(String))

      // Tap center to reveal chrome & system bars
      act(() => {
        opened[0]!.params.events.onViewportTap(0.5, 0.5)
      })
      expect(setSystemUi).toHaveBeenLastCalledWith(
        true,
        false,
        false,
        expect.any(String)
      )

      // Bottom chrome keeps TOC & Reader Settings without Paginated/Scrolled flow toggle button
      expect(
        screen.getByRole("button", { name: /Contents|目录/i })
      ).toBeInTheDocument()
      expect(
        screen.getByRole("button", { name: /Reader Settings|阅读设置/i })
      ).toBeInTheDocument()
      expect(
        screen.queryByRole("button", { name: /^(Paginated|Scrolled|翻页|滚动)$/i })
      ).not.toBeInTheDocument()

      // Open Settings Sheet -> [role="dialog"]
      await user.click(screen.getByRole("button", { name: "Reader Settings" }))
      expect(await screen.findByRole("dialog")).toBeInTheDocument()

      // Emit an active text selection inside the viewport
      const clearSelectionSpy = vi.fn()
      act(() => {
        opened[0]!.params.events.onSelectionChange(
          makeSelectionContext(clearSelectionSpy)
        )
      })

      // 1st Back: closes the open Sheet dialog first; selection and reader remain
      act(() => {
        expect(dispatchBack()).toBe(true)
      })
      await waitFor(() => {
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
      })
      expect(clearSelectionSpy).not.toHaveBeenCalled()
      expect(mobileReaderStore.getSnapshot().isOpen).toBe(true)

      // 2nd Back: clears active selection at BACK_PRIORITY.readerSelection (46)
      act(() => {
        expect(dispatchBack()).toBe(true)
      })
      expect(clearSelectionSpy).toHaveBeenCalledTimes(1)
      expect(mobileReaderStore.getSnapshot().isOpen).toBe(true)

      // 3rd Back: collapses reader overlay at BACK_PRIORITY.readerWorkspace (42) and restores Android bars
      act(() => {
        expect(dispatchBack()).toBe(true)
      })
      expect(mobileReaderStore.getSnapshot().isOpen).toBe(false)
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(1)
      expect(setSystemUi).toHaveBeenLastCalledWith(
        false,
        false,
        false,
        "#ffffff"
      )
      expect(searchHandler).not.toHaveBeenCalled()

      // Subsequent Back presses cascade down bookshelf priorities: search -> selection -> navigation
      expect(dispatchBack()).toBe(true)
      expect(searchHandler).toHaveBeenCalledTimes(1)
      unsubSearch()

      expect(dispatchBack()).toBe(true)
      expect(selectionHandler).toHaveBeenCalledTimes(1)
      unsubSelection()

      expect(dispatchBack()).toBe(true)
      expect(navigationHandler).toHaveBeenCalledTimes(1)
    } finally {
      unsubSearch()
      unsubSelection()
      unsubNavigation()
    }
  })

  it("mounts only one active ReaderViewport across tabs, restores saved locators, and closes all sessions", async () => {
    const { opened } = setupMockReaderEngine()
    const epubCloseSpy = vi
      .spyOn(readerBridge, "epubClose")
      .mockResolvedValue(undefined)

    const bookA = makeBook({ id: "book-a", title: "Book A", type: "epub" })
    const bookB = makeBook({ id: "book-b", title: "Book B", type: "epub" })
    const bookC = makeBook({ id: "book-c", title: "Book C", type: "epub" })

    const locA: BookLocator = {
      kind: "epub",
      spine_index: 2,
      href: "ch2.xhtml",
      fragment: null,
      cfi: "epubcfi(/6/6!/4/10)",
    }
    const locB: BookLocator = {
      kind: "epub",
      spine_index: 4,
      href: "ch4.xhtml",
      fragment: null,
      cfi: "epubcfi(/6/10!/4/4)",
    }

    const { user, container } = renderWithProviders(<MobileReaderOverlay />)

    act(() => {
      mobileReaderStore.openBook(bookA)
      mobileReaderStore.openBookInBackground(bookB)
      mobileReaderStore.openBookInBackground(bookC)
    })

    await waitFor(() => {
      expect(opened).toHaveLength(1)
    })
    expect(opened[0]!.params.bookId).toBe("book-a")
    expect(container.querySelectorAll("[data-reader-root]")).toHaveLength(1)

    // Record Book A's live session ID and reading position
    act(() => {
      opened[0]!.params.events.onSessionChange?.("sess-a")
      opened[0]!.params.events.onRelocate({
        locator: locA,
        fraction: 0.35,
        sectionIndex: 2,
        sectionTotal: 5,
        activeTocIndex: null,
        pageStats: null,
      })
    })

    // Show chrome & switch to Book B via desktop-style horizontal tab strip in top bar
    act(() => {
      opened[0]!.params.events.onViewportTap(0.5, 0.5)
    })
    const tablist = screen.getByRole("tablist", { name: "Reader tabs" })
    const tabItems = within(tablist).getAllByRole("tab")
    expect(tabItems).toHaveLength(3)
    expect(within(tablist).getByRole("tab", { name: /Book A/ })).toHaveAttribute(
      "aria-selected",
      "true"
    )
    await user.click(within(tablist).getByRole("tab", { name: /Book B/ }))

    await waitFor(() => {
      expect(opened).toHaveLength(2)
    })
    expect(opened[1]!.params.bookId).toBe("book-b")
    expect(container.querySelectorAll("[data-reader-root]")).toHaveLength(1)

    act(() => {
      opened[1]!.params.events.onSessionChange?.("sess-b")
      opened[1]!.params.events.onRelocate({
        locator: locB,
        fraction: 0.72,
        sectionIndex: 4,
        sectionTotal: 5,
        activeTocIndex: null,
        pageStats: null,
      })
    })

    // Switch back to Book A directly from the horizontal tab strip -> verifies saved locator & reattachSessionId
    await user.click(
      within(screen.getByRole("tablist", { name: "Reader tabs" })).getByRole(
        "tab",
        { name: /Book A/ }
      )
    )
    await waitFor(() => {
      expect(opened).toHaveLength(3)
    })
    expect(opened[2]!.params.bookId).toBe("book-a")
    expect(opened[2]!.params.initialLocator).toEqual(locA)
    expect(opened[2]!.params.reattachSessionId).toBe("sess-a")
    expect(container.querySelectorAll("[data-reader-root]")).toHaveLength(1)

    // Back to library hides overlay while keeping open tabs alive in memory
    await user.click(screen.getByRole("button", { name: "Back to Library" }))
    expect(mobileReaderStore.getSnapshot().isOpen).toBe(false)
    expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(3)

    // Reopen overlay, show chrome, and close tabs individually via each tab's close button
    act(() => {
      mobileReaderStore.reopenOverlay()
    })
    await waitFor(() => {
      expect(opened).toHaveLength(4)
    })
    act(() => {
      opened[3]!.params.events.onViewportTap(0.5, 0.5)
    })
    await user.click(screen.getByRole("button", { name: "Close tab: Book A" }))
    await user.click(screen.getByRole("button", { name: "Close tab: Book B" }))
    await user.click(screen.getByRole("button", { name: "Close tab: Book C" }))

    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
      expect(mobileReaderStore.getSnapshot().isOpen).toBe(false)
    })
    expect(epubCloseSpy).toHaveBeenCalledTimes(2)
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-a")
    expect(epubCloseSpy).toHaveBeenCalledWith("sess-b")
  })

  it("clears mobile reader tabs when runtime leaves ready phase or switches cloud user", async () => {
    setupMockReaderEngine()
    const cloudAlice = readyCloudState({
      cloud: makeCloudStatus({
        origin: "https://books.example.com",
        user: { userId: "alice-id", username: "alice" },
        hasSavedSession: true,
      }),
    })
    const cloudBob = readyCloudState({
      cloud: makeCloudStatus({
        origin: "https://books.example.com",
        user: { userId: "bob-id", username: "bob" },
        hasSavedSession: true,
      }),
    })

    tauriTestHost.setTauriRuntime(true)
    tauriTestHost.onInvoke("get_bootstrap_state", () => cloudAlice)

    renderWithProviders(<MobileOverlayHost />, { withRuntime: true })

    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
    })

    // Open a book while ready as Alice
    act(() => {
      mobileReaderStore.openBook(
        makeBook({ id: "cloud-1", title: "Cloud Book 1", type: "epub" })
      )
    })
    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(1)
    })

    // Switching cloud user clears all open tabs
    act(() => {
      tauriTestHost.emitRuntimeState(cloudBob)
    })
    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
      expect(mobileReaderStore.getSnapshot().isOpen).toBe(false)
    })

    // Re-open in cloud mode, then switch runtime phase away from ready -> clears tabs
    act(() => {
      mobileReaderStore.openBook(
        makeBook({ id: "cloud-2", title: "Cloud Book 2", type: "epub" })
      )
    })
    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(1)
    })

    act(() => {
      tauriTestHost.emitRuntimeState(cloudAuthRequiredState())
    })
    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
      expect(mobileReaderStore.getSnapshot().isOpen).toBe(false)
    })

    // Switching back from local ready should remain safe
    act(() => {
      tauriTestHost.emitRuntimeState(readyLocalState())
    })
    await waitFor(() => {
      expect(mobileReaderStore.getSnapshot().tabs).toHaveLength(0)
    })
  })

  it("dismisses nested FontPickerModal on Android back before dismissing the parent Settings sheet", async () => {
    const { opened } = setupMockReaderEngine()
    const book = makeBook({ id: "m-font-back", title: "Font Stack Book", type: "epub" })
    const { user } = renderWithProviders(<MobileReaderOverlay />)

    act(() => {
      mobileReaderStore.openBook(book)
    })
    await waitFor(() => {
      expect(opened).toHaveLength(1)
    })

    // Reveal chrome & open Settings Sheet
    act(() => {
      opened[0]!.params.events.onViewportTap(0.5, 0.5)
    })
    await user.click(screen.getByRole("button", { name: /Reader Settings|阅读设置/i }))
    await screen.findByText(/Global Defaults|全局默认/i)

    // Open nested FontPickerModal from Settings Sheet
    await user.click(
      screen.getByRole("button", { name: /Publisher Default|书籍默认/i })
    )
    await screen.findByRole("button", { name: /Import Font|导入字体/i })

    // 1st Android back: closes FontPickerModal while Settings Sheet remains open
    act(() => {
      expect(dispatchBack()).toBe(true)
    })
    await waitFor(() => {
      expect(
        screen.queryByRole("button", { name: /Import Font|导入字体/i })
      ).not.toBeInTheDocument()
    })
    expect(screen.getByText(/Global Defaults|全局默认/i)).toBeInTheDocument()

    // 2nd Android back: closes Settings Sheet while reader overlay stays open
    act(() => {
      expect(dispatchBack()).toBe(true)
    })
    await waitFor(() => {
      expect(
        screen.queryByText(/Global Defaults|全局默认/i)
      ).not.toBeInTheDocument()
    })
    expect(mobileReaderStore.getSnapshot().isOpen).toBe(true)
  })
})
