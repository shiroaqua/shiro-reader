import { useSyncExternalStore } from "react"
import { readerBridge } from "@shiro/ui-shared/api/reader-bridge"
import type {
  EphemeralReaderViewState,
  NavigationLocator,
  RelocationSnapshot,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import { assertBookSupportedInReader } from "@shiro/ui-shared/features/reader/workspace/book-launch"
import {
  InWindowTabStore,
  type NavigationHistoryEntry,
  type ReaderTabSession,
} from "@shiro/ui-shared/features/reader/workspace/tab-session-store"
import type { Book } from "@shiro/ui-shared/lib/types"

export interface MobileReaderState {
  readonly isOpen: boolean
  readonly tabs: readonly ReaderTabSession[]
  readonly activeTabId: string | null
}

function closeTabSession(tab: ReaderTabSession): void {
  if (!tab.sessionId) return
  void readerBridge.epubClose(tab.sessionId).catch(() => {})
}

class MobileReaderStoreController {
  private readonly tabStore: InWindowTabStore
  private isOpen = false
  private snapshot: MobileReaderState
  private readonly listeners = new Set<() => void>()

  constructor() {
    this.tabStore = new InWindowTabStore({
      onTabClosed: closeTabSession,
    })
    const initial = this.tabStore.getSnapshot()
    this.snapshot = {
      isOpen: false,
      tabs: initial.tabs,
      activeTabId: initial.activeTabId,
    }
    this.tabStore.subscribe(() => {
      const next = this.tabStore.getSnapshot()
      if (next.tabs.length === 0) {
        this.isOpen = false
      }
      this.commit(next.tabs, next.activeTabId)
    })
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  getSnapshot = (): MobileReaderState => {
    return this.snapshot
  }

  openBook(book: Book): boolean {
    if (!assertBookSupportedInReader(book)) return false
    const result = this.tabStore.openOrActivate(book)
    if (!result.tab) return false
    if (!this.isOpen) {
      this.isOpen = true
      const current = this.tabStore.getSnapshot()
      this.commit(current.tabs, current.activeTabId)
    }
    return true
  }

  openBookInBackground(book: Book): boolean {
    if (!assertBookSupportedInReader(book)) return false
    const result = this.tabStore.openInBackground(book)
    return result.tab !== null
  }

  reopenOverlay(): void {
    const current = this.tabStore.getSnapshot()
    if (current.tabs.length === 0 || this.isOpen) return
    this.isOpen = true
    this.commit(current.tabs, current.activeTabId)
  }

  closeOverlay(): void {
    if (!this.isOpen) return
    this.isOpen = false
    const current = this.tabStore.getSnapshot()
    this.commit(current.tabs, current.activeTabId)
  }

  activateTab(tabId: string): void {
    this.tabStore.activate(tabId)
  }

  closeTab(tabId: string): void {
    this.tabStore.close(tabId)
  }

  closeAllTabs(): void {
    this.isOpen = false
    const current = this.tabStore.getSnapshot()
    if (current.tabs.length === 0) {
      this.commit(current.tabs, current.activeTabId)
      return
    }
    this.tabStore.closeAll()
  }

  evictBookIds(bookIds: readonly string[]): void {
    this.tabStore.evictBookIds(bookIds)
  }

  updateSessionId(tabId: string, sessionId: string | null): void {
    this.tabStore.updateSessionId(tabId, sessionId)
  }

  updateRelocation(tabId: string, relocation: RelocationSnapshot): void {
    this.tabStore.updateRelocation(tabId, relocation)
  }

  updateViewState(tabId: string, viewState: EphemeralReaderViewState): void {
    this.tabStore.updateViewState(tabId, viewState)
  }

  pushHistory(tabId: string, entry: NavigationHistoryEntry): void {
    this.tabStore.pushHistory(tabId, entry)
  }

  popHistory(tabId: string): NavigationHistoryEntry | null {
    return this.tabStore.popHistory(tabId)
  }

  getActiveLocator(tabId: string): NavigationLocator | null {
    const tab = this.snapshot.tabs.find((item) => item.tabId === tabId)
    return tab?.lastRelocation?.locator ?? null
  }

  resetForTest(): void {
    this.isOpen = false
    const current = this.tabStore.getSnapshot()
    if (current.tabs.length > 0) {
      this.tabStore.closeAll()
    }
    const after = this.tabStore.getSnapshot()
    this.snapshot = {
      isOpen: false,
      tabs: after.tabs,
      activeTabId: after.activeTabId,
    }
    this.listeners.clear()
  }

  private commit(
    tabs: readonly ReaderTabSession[],
    activeTabId: string | null
  ): void {
    this.snapshot = {
      isOpen: this.isOpen,
      tabs,
      activeTabId,
    }
    for (const listener of this.listeners) {
      listener()
    }
  }
}

export const mobileReaderStore = new MobileReaderStoreController()

export function useMobileReaderStore(): MobileReaderState {
  return useSyncExternalStore(
    mobileReaderStore.subscribe,
    mobileReaderStore.getSnapshot,
    mobileReaderStore.getSnapshot
  )
}

export function resetMobileReaderStoreForTest(): void {
  mobileReaderStore.resetForTest()
}
