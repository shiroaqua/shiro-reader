import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { CommandError, isCommandErrorPayload } from "./tauri-bridge"
import type {
  BookFormat,
  BookNavigationData,
  NavigationLocator,
  TranslationDisplayMode,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import type {
  ReaderProfile,
  ReaderProfilePatch,
  ReaderSettingsSnapshot,
  SystemFontFamily,
} from "@shiro/ui-shared/features/reader/contracts/settings"

export const RUST_READER_SETTINGS_EVENT = "shiro://reader-settings"
export const RUST_READER_WINDOW_STATE_EVENT = "shiro://reader-window-state"
export const RUST_READER_TAB_DROP_PREVIEW_EVENT =
  "shiro://reader-tab-drop-preview"

export const READER_ERROR_CODES = {
  SESSION_NOT_FOUND: 3101,
  ENTRY_NOT_FOUND: 3102,
  UNSUPPORTED_FORMAT: 3103,
  CORRUPTED_EPUB: 3104,
  ENTRY_TOO_LARGE: 3105,
  INVALID_FONT_FILE: 3106,
  FONT_TOO_LARGE: 3107,
  FONT_NOT_FOUND: 3108,
} as const

export interface OpenEpubResponse {
  readonly sessionId: string
  readonly bookId: string
  readonly fileSize: number
  readonly entries: Readonly<Record<string, number>>
}

export interface ReaderWindowTabDto {
  readonly tabId: string
  readonly bookId: string
  readonly bookHash: string
  readonly title: string
  readonly format: BookFormat
  readonly sessionId: string | null
  readonly currentLocator: NavigationLocator | null
  readonly fraction: number
  readonly translationMode: TranslationDisplayMode
  readonly targetLanguage: string | null
}

export interface OtherReaderWindowSummary {
  readonly windowId: string
  readonly activeTitle: string
  readonly tabCount: number
}

export interface ReaderWindowSnapshot {
  readonly windowId: string
  readonly windowLabel: string
  readonly tabs: readonly ReaderWindowTabDto[]
  readonly activeTabId: string | null
  readonly otherWindows: readonly OtherReaderWindowSummary[]
  readonly canCrossWindowPointerHitTest: boolean
}

export interface TabDropPreviewEvent {
  readonly windowId: string
  readonly insertIndex: number | null
  readonly draggingTabTitle: string | null
}

export interface OpenBookInReaderRequest {
  readonly bookId: string
  readonly bookHash?: string | undefined
  readonly title?: string | undefined
  readonly format?: string | undefined
  readonly newWindow: boolean
}

export interface UpdateTabRuntimeStateRequest {
  readonly windowId: string
  readonly tabId: string
  readonly sessionId?: string | undefined
  readonly currentLocator?: NavigationLocator | undefined
  readonly fraction?: number | undefined
  readonly translationMode?: TranslationDisplayMode | undefined
  readonly targetLanguage?: string | null | undefined
}

export function toArrayBuffer(
  raw: ArrayBuffer | Uint8Array | readonly number[]
): ArrayBuffer {
  if (raw instanceof ArrayBuffer) {
    return raw
  }
  if (raw instanceof Uint8Array) {
    if (raw.buffer instanceof ArrayBuffer) {
      if (raw.byteOffset === 0 && raw.byteLength === raw.buffer.byteLength) {
        return raw.buffer
      }
      return raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength)
    }
    const copy = new Uint8Array(raw.byteLength)
    copy.set(raw)
    return copy.buffer
  }
  return new Uint8Array(raw).buffer
}

export function getCustomFontProtocolUrl(
  fontUrlBase: string,
  fontId: string
): string {
  const normalizedBase = fontUrlBase.endsWith("/")
    ? fontUrlBase
    : `${fontUrlBase}/`
  return `${normalizedBase}${encodeURIComponent(fontId)}`
}

async function invokeReader<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    if (isCommandErrorPayload(error)) {
      throw new CommandError(error)
    }
    throw error
  }
}

export interface ReaderBridge {
  epubOpen(bookId: string): Promise<OpenEpubResponse>
  epubAttachOrOpen(
    bookId: string,
    sessionId: string | null
  ): Promise<OpenEpubResponse>
  epubReadEntry(sessionId: string, path: string): Promise<ArrayBuffer>
  epubClose(sessionId: string): Promise<void>
  bookNavigation(bookId: string): Promise<BookNavigationData>
  readerGetSettings(): Promise<ReaderSettingsSnapshot>
  readerUpdateGlobalSettings(
    settings: ReaderProfile
  ): Promise<ReaderSettingsSnapshot>
  readerSetBookOverride(
    bookHash: string,
    patch: ReaderProfilePatch
  ): Promise<ReaderSettingsSnapshot>
  readerResetBookOverride(bookHash: string): Promise<ReaderSettingsSnapshot>
  readerListSystemFonts(): Promise<SystemFontFamily[]>
  readerPickAndImportFont(): Promise<ReaderSettingsSnapshot>
  readerDeleteCustomFont(fontId: string): Promise<ReaderSettingsSnapshot>
  readerReadCustomFont(fontId: string): Promise<ArrayBuffer>
  listenReaderSettings(
    handler: (snapshot: ReaderSettingsSnapshot) => void
  ): Promise<UnlistenFn>
  readerOpenBook(req: OpenBookInReaderRequest): Promise<ReaderWindowSnapshot>
  readerGetWindowSnapshot(windowId: string): Promise<ReaderWindowSnapshot>
  readerActivateTab(
    windowId: string,
    tabId: string
  ): Promise<ReaderWindowSnapshot>
  readerReorderTabs(
    windowId: string,
    tabIds: readonly string[]
  ): Promise<ReaderWindowSnapshot>
  readerCloseTab(
    windowId: string,
    tabId: string
  ): Promise<ReaderWindowSnapshot>
  readerUpdateTabState(req: UpdateTabRuntimeStateRequest): Promise<void>
  readerReportTabStripMetrics(
    windowId: string,
    heightPx: number,
    dragWidthPx: number
  ): Promise<void>
  readerMoveTabToWindow(
    sourceWindowId: string,
    tabId: string,
    targetWindowId: string,
    targetIndex?: number | undefined
  ): Promise<ReaderWindowSnapshot>
  readerTearOffTab(
    sourceWindowId: string,
    tabId: string,
    screenX?: number | undefined,
    screenY?: number | undefined
  ): Promise<ReaderWindowSnapshot>
  readerDragStart(windowId: string, tabId: string): Promise<void>
  readerDragOver(
    windowId: string,
    clientX: number,
    clientY: number
  ): Promise<void>
  readerDragCommit(
    windowId: string,
    clientX: number,
    clientY: number
  ): Promise<void>
  readerDragCancel(windowId: string): Promise<void>
  readerOpenExternalUrl(url: string): Promise<void>
  listenReaderWindowState(
    handler: (snapshot: ReaderWindowSnapshot) => void
  ): Promise<UnlistenFn>
  listenReaderTabDropPreview(
    handler: (preview: TabDropPreviewEvent) => void
  ): Promise<UnlistenFn>
}

export const tauriReaderBridge: ReaderBridge = {
  epubOpen: (bookId) => invokeReader<OpenEpubResponse>("epub_open", { bookId }),
  epubAttachOrOpen: (bookId, sessionId) =>
    invokeReader<OpenEpubResponse>("epub_attach_or_open", {
      bookId,
      sessionId,
    }),
  epubReadEntry: async (sessionId, path) => {
    const raw = await invokeReader<ArrayBuffer | Uint8Array | number[]>(
      "epub_read_entry",
      { sessionId, path }
    )
    return toArrayBuffer(raw)
  },
  epubClose: (sessionId) => invokeReader<void>("epub_close", { sessionId }),
  bookNavigation: (bookId) =>
    invokeReader<BookNavigationData>("book_navigation", { id: bookId }),
  readerGetSettings: () =>
    invokeReader<ReaderSettingsSnapshot>("reader_get_settings"),
  readerUpdateGlobalSettings: (settings) =>
    invokeReader<ReaderSettingsSnapshot>("reader_update_global_settings", {
      settings,
    }),
  readerSetBookOverride: (bookHash, patch) =>
    invokeReader<ReaderSettingsSnapshot>("reader_set_book_override", {
      bookHash,
      patch,
    }),
  readerResetBookOverride: (bookHash) =>
    invokeReader<ReaderSettingsSnapshot>("reader_reset_book_override", {
      bookHash,
    }),
  readerListSystemFonts: () =>
    invokeReader<SystemFontFamily[]>("reader_list_system_fonts"),
  readerPickAndImportFont: () =>
    invokeReader<ReaderSettingsSnapshot>("reader_pick_and_import_font"),
  readerDeleteCustomFont: (fontId) =>
    invokeReader<ReaderSettingsSnapshot>("reader_delete_custom_font", {
      fontId,
    }),
  readerReadCustomFont: async (fontId) => {
    const raw = await invokeReader<ArrayBuffer | Uint8Array | number[]>(
      "reader_read_custom_font",
      { fontId }
    )
    return toArrayBuffer(raw)
  },
  listenReaderSettings: (handler) =>
    listen<ReaderSettingsSnapshot>(RUST_READER_SETTINGS_EVENT, (event) => {
      handler(event.payload)
    }),
  readerOpenBook: (req) =>
    invokeReader<ReaderWindowSnapshot>("reader_open_book", {
      bookId: req.bookId,
      bookHash: req.bookHash ?? null,
      title: req.title ?? null,
      format: req.format ?? null,
      newWindow: req.newWindow,
    }),
  readerGetWindowSnapshot: (windowId) =>
    invokeReader<ReaderWindowSnapshot>("reader_get_window_snapshot", {
      windowId,
    }),
  readerActivateTab: (windowId, tabId) =>
    invokeReader<ReaderWindowSnapshot>("reader_activate_tab", {
      windowId,
      tabId,
    }),
  readerReorderTabs: (windowId, tabIds) =>
    invokeReader<ReaderWindowSnapshot>("reader_reorder_tabs", {
      windowId,
      tabIds,
    }),
  readerCloseTab: (windowId, tabId) =>
    invokeReader<ReaderWindowSnapshot>("reader_close_tab", {
      windowId,
      tabId,
    }),
  readerUpdateTabState: (req) =>
    invokeReader<void>("reader_update_tab_state", {
      windowId: req.windowId,
      tabId: req.tabId,
      sessionId: req.sessionId ?? null,
      currentLocator: req.currentLocator ?? null,
      fraction: req.fraction ?? null,
      translationMode: req.translationMode ?? null,
      targetLanguage:
        req.targetLanguage === undefined ? null : req.targetLanguage ?? "",
    }),
  readerReportTabStripMetrics: (windowId, heightPx, dragWidthPx) =>
    invokeReader<void>("reader_report_tab_strip_metrics", {
      windowId,
      heightPx,
      dragWidthPx,
    }),
  readerMoveTabToWindow: (
    sourceWindowId,
    tabId,
    targetWindowId,
    targetIndex
  ) =>
    invokeReader<ReaderWindowSnapshot>("reader_move_tab_to_window", {
      sourceWindowId,
      tabId,
      targetWindowId,
      targetIndex: targetIndex ?? null,
    }),
  readerTearOffTab: (sourceWindowId, tabId, screenX, screenY) =>
    invokeReader<ReaderWindowSnapshot>("reader_tear_off_tab", {
      sourceWindowId,
      tabId,
      screenX: screenX ?? null,
      screenY: screenY ?? null,
    }),
  readerDragStart: (windowId, tabId) =>
    invokeReader<void>("reader_drag_start", { windowId, tabId }),
  readerDragOver: (windowId, clientX, clientY) =>
    invokeReader<void>("reader_drag_over", { windowId, clientX, clientY }),
  readerDragCommit: (windowId, clientX, clientY) =>
    invokeReader<void>("reader_drag_commit", { windowId, clientX, clientY }),
  readerDragCancel: (windowId) =>
    invokeReader<void>("reader_drag_cancel", { windowId }),
  readerOpenExternalUrl: async (url) => {
    const androidBridge =
      typeof window !== "undefined" ? window.__SHIRO_ANDROID_READER__ : undefined
    if (androidBridge?.openExternalUrl) {
      const accepted = androidBridge.openExternalUrl(url)
      if (!accepted) {
        throw new Error("Unsupported or invalid external URL")
      }
      return
    }
    await invokeReader<void>("reader_open_external_url", { url })
  },
  listenReaderWindowState: (handler) =>
    listen<ReaderWindowSnapshot>(RUST_READER_WINDOW_STATE_EVENT, (event) => {
      handler(event.payload)
    }),
  listenReaderTabDropPreview: (handler) =>
    listen<TabDropPreviewEvent>(
      RUST_READER_TAB_DROP_PREVIEW_EVENT,
      (event) => {
        handler(event.payload)
      }
    ),
}

export let readerBridge: ReaderBridge = tauriReaderBridge

export function setPreviewReaderBridge(bridge: ReaderBridge): void {
  if (import.meta.env.DEV) {
    readerBridge = bridge
  }
}

export function setReaderBridgeForTest(bridge: ReaderBridge): void {
  readerBridge = bridge
}

export function resetReaderBridge(): void {
  readerBridge = tauriReaderBridge
}
