import type { Book } from "@shiro/ui-shared/lib/types"
import {
  normalizeBookFormat,
  type BookFormat,
  type BookLocator,
  type EphemeralReaderViewState,
  type RelocationSnapshot,
} from "../contracts/engine"

export const MAX_NAVIGATION_HISTORY = 16

export interface NavigationHistoryEntry {
  readonly locator: BookLocator
  readonly fraction: number
  readonly label?: string | undefined
}

export class NavigationHistoryStack {
  private readonly items: NavigationHistoryEntry[] = []
  private readonly maxItems: number

  constructor(maxItems = MAX_NAVIGATION_HISTORY) {
    this.maxItems = Math.max(1, maxItems)
  }

  get size(): number {
    return this.items.length
  }

  push(entry: NavigationHistoryEntry): void {
    this.items.push(entry)
    if (this.items.length > this.maxItems) {
      this.items.splice(0, this.items.length - this.maxItems)
    }
  }

  pop(): NavigationHistoryEntry | null {
    return this.items.pop() ?? null
  }

  peek(): NavigationHistoryEntry | null {
    return this.items[this.items.length - 1] ?? null
  }

  clear(): void {
    this.items.length = 0
  }

  toArray(): readonly NavigationHistoryEntry[] {
    return [...this.items]
  }
}

export interface ReaderTabSession {
  readonly tabId: string
  readonly bookId: string
  readonly bookHash: string
  readonly title: string
  readonly format: BookFormat
  readonly sessionId: string | null
  readonly lastRelocation: RelocationSnapshot | null
  readonly viewState: EphemeralReaderViewState
  readonly history: readonly NavigationHistoryEntry[]
}

export interface InWindowTabStoreSnapshot {
  readonly tabs: readonly ReaderTabSession[]
  readonly activeTabId: string | null
}

export interface InWindowTabStoreOptions {
  readonly onTabClosed?: ((tab: ReaderTabSession) => void) | undefined
}

let nextGeneratedTabSeq = 1

function createTabId(bookId: string): string {
  const seq = nextGeneratedTabSeq++
  return `tab-${bookId}-${seq}`
}

export class InWindowTabStore {
  private tabs: ReaderTabSession[] = []
  private activeTabId: string | null = null
  private snapshot: InWindowTabStoreSnapshot = {
    tabs: [],
    activeTabId: null,
  }
  private readonly listeners = new Set<() => void>()
  private readonly onTabClosed: ((tab: ReaderTabSession) => void) | undefined

  constructor(options: InWindowTabStoreOptions = {}) {
    this.onTabClosed = options.onTabClosed
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  getSnapshot = (): InWindowTabStoreSnapshot => {
    return this.snapshot
  }

  openOrActivate(book: Book): {
    readonly tab: ReaderTabSession | null
    readonly created: boolean
  } {
    const existing = this.tabs.find((tab) => tab.bookId === book.id)
    if (existing) {
      if (this.activeTabId !== existing.tabId) {
        this.activeTabId = existing.tabId
        this.commit()
      }
      return { tab: existing, created: false }
    }

    const createdTab = this.createTabRecord(book)
    if (!createdTab) return { tab: null, created: false }

    this.tabs = [...this.tabs, createdTab]
    this.activeTabId = createdTab.tabId
    this.commit()
    return { tab: createdTab, created: true }
  }

  openInBackground(book: Book): {
    readonly tab: ReaderTabSession | null
    readonly created: boolean
  } {
    const existing = this.tabs.find((tab) => tab.bookId === book.id)
    if (existing) {
      return { tab: existing, created: false }
    }

    const createdTab = this.createTabRecord(book)
    if (!createdTab) return { tab: null, created: false }

    this.tabs = [...this.tabs, createdTab]
    if (this.activeTabId === null) {
      this.activeTabId = createdTab.tabId
    }
    this.commit()
    return { tab: createdTab, created: true }
  }

  activate(tabId: string): void {
    if (this.activeTabId === tabId) return
    if (!this.tabs.some((tab) => tab.tabId === tabId)) return
    this.activeTabId = tabId
    this.commit()
  }

  close(tabId: string): ReaderTabSession | null {
    const index = this.tabs.findIndex((tab) => tab.tabId === tabId)
    if (index < 0) return null
    const removed = this.tabs[index]!
    const nextTabs = this.tabs.filter((tab) => tab.tabId !== tabId)

    let nextActive = this.activeTabId
    if (this.activeTabId === tabId) {
      const replacement = nextTabs[index] ?? nextTabs[index - 1] ?? null
      nextActive = replacement?.tabId ?? null
    }

    this.tabs = nextTabs
    this.activeTabId = nextActive
    this.commit()
    this.onTabClosed?.(removed)
    return removed
  }

  closeOthers(keepTabId: string): void {
    const kept = this.tabs.find((tab) => tab.tabId === keepTabId)
    if (!kept) return
    const removed = this.tabs.filter((tab) => tab.tabId !== keepTabId)
    if (removed.length === 0) return

    this.tabs = [kept]
    this.activeTabId = kept.tabId
    this.commit()
    for (const tab of removed) {
      this.onTabClosed?.(tab)
    }
  }

  closeRight(tabId: string): void {
    const index = this.tabs.findIndex((tab) => tab.tabId === tabId)
    if (index < 0 || index === this.tabs.length - 1) return

    const kept = this.tabs.slice(0, index + 1)
    const removed = this.tabs.slice(index + 1)
    const activeStillExists =
      this.activeTabId !== null &&
      kept.some((tab) => tab.tabId === this.activeTabId)

    this.tabs = kept
    if (!activeStillExists) {
      this.activeTabId = kept[kept.length - 1]?.tabId ?? null
    }
    this.commit()
    for (const tab of removed) {
      this.onTabClosed?.(tab)
    }
  }

  closeAll(): void {
    if (this.tabs.length === 0) return
    const removed = this.tabs
    this.tabs = []
    this.activeTabId = null
    this.commit()
    for (const tab of removed) {
      this.onTabClosed?.(tab)
    }
  }

  reorder(orderedTabIds: readonly string[]): void {
    const byId = new Map(this.tabs.map((tab) => [tab.tabId, tab]))
    const next: ReaderTabSession[] = []
    for (const id of orderedTabIds) {
      const found = byId.get(id)
      if (found) {
        next.push(found)
        byId.delete(id)
      }
    }
    for (const remaining of byId.values()) {
      next.push(remaining)
    }
    this.tabs = next
    this.commit()
  }

  evictBookIds(bookIds: readonly string[]): void {
    if (bookIds.length === 0) return
    const idSet = new Set(bookIds)
    const removed = this.tabs.filter((tab) => idSet.has(tab.bookId))
    if (removed.length === 0) return

    const nextTabs = this.tabs.filter((tab) => !idSet.has(tab.bookId))
    const activeStillExists =
      this.activeTabId !== null &&
      nextTabs.some((tab) => tab.tabId === this.activeTabId)

    this.tabs = nextTabs
    if (!activeStillExists) {
      this.activeTabId = nextTabs[0]?.tabId ?? null
    }
    this.commit()
    for (const tab of removed) {
      this.onTabClosed?.(tab)
    }
  }

  updateSessionId(tabId: string, sessionId: string | null): void {
    this.patchTab(tabId, (tab) =>
      tab.sessionId === sessionId ? tab : { ...tab, sessionId }
    )
  }

  updateRelocation(tabId: string, relocation: RelocationSnapshot): void {
    this.patchTab(tabId, (tab) => ({
      ...tab,
      lastRelocation: relocation,
    }))
  }

  updateViewState(tabId: string, viewState: EphemeralReaderViewState): void {
    this.patchTab(tabId, (tab) => ({
      ...tab,
      viewState,
    }))
  }

  pushHistory(tabId: string, entry: NavigationHistoryEntry): void {
    this.patchTab(tabId, (tab) => {
      const next = [...tab.history, entry]
      if (next.length > MAX_NAVIGATION_HISTORY) {
        next.splice(0, next.length - MAX_NAVIGATION_HISTORY)
      }
      return { ...tab, history: next }
    })
  }

  popHistory(tabId: string): NavigationHistoryEntry | null {
    const tab = this.tabs.find((item) => item.tabId === tabId)
    if (!tab || tab.history.length === 0) return null
    const popped = tab.history[tab.history.length - 1] ?? null
    this.patchTab(tabId, (current) => ({
      ...current,
      history: current.history.slice(0, -1),
    }))
    return popped
  }

  clearHistory(tabId: string): void {
    this.patchTab(tabId, (tab) =>
      tab.history.length === 0 ? tab : { ...tab, history: [] }
    )
  }

  private createTabRecord(book: Book): ReaderTabSession | null {
    const format = normalizeBookFormat(book.type)
    if (format === "UNKNOWN") return null
    return {
      tabId: createTabId(book.id),
      bookId: book.id,
      bookHash: book.hash,
      title: book.title,
      format,
      sessionId: null,
      lastRelocation: null,
      viewState: {
        translationMode: "original",
        targetLanguage: null,
      },
      history: [],
    }
  }

  private patchTab(
    tabId: string,
    updater: (tab: ReaderTabSession) => ReaderTabSession
  ): void {
    let changed = false
    const nextTabs = this.tabs.map((tab) => {
      if (tab.tabId !== tabId) return tab
      const updated = updater(tab)
      if (updated !== tab) changed = true
      return updated
    })
    if (!changed) return
    this.tabs = nextTabs
    this.commit()
  }

  private commit(): void {
    this.snapshot = {
      tabs: this.tabs,
      activeTabId: this.activeTabId,
    }
    for (const listener of this.listeners) {
      listener()
    }
  }
}
