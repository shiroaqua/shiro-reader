import "@testing-library/jest-dom/vitest"
import { cleanup } from "@testing-library/react"
import { afterEach, vi } from "vitest"
import { resetLibraryBridge } from "@shiro/ui-shared/api/library"
import { resetReaderBridge } from "@shiro/ui-shared/api/reader-bridge"
import { setCoverUrlBase } from "@shiro/ui-shared/api/tauri-bridge"
import { resetBookImportState } from "@shiro/ui-shared/features/books/book-import-store"
import { resetReaderSettingsStoreForTest } from "@shiro/ui-shared/features/reader/settings/reader-settings-store"
import { resetErrorNotifications } from "@shiro/ui-shared/lib/error-notifications"
import { setAppLocaleOverride } from "@shiro/ui-shared/lib/i18n"
import { resetBuilderIds } from "./support/builders"
import { installMediaMocks, resetMediaMocks } from "./support/media"
import { tauriTestHost } from "./support/mock-tauri"

vi.mock("@tauri-apps/api/core", async () => {
  const { FakeChannel, tauriTestHost: host } = await import(
    "./support/mock-tauri"
  )
  return {
    Channel: FakeChannel,
    invoke: (command: string, args?: Record<string, unknown>) =>
      host.invoke(command, args),
    isTauri: () => host.isTauriRuntime(),
  }
})

vi.mock("@tauri-apps/api/event", async () => {
  const { tauriTestHost: host } = await import("./support/mock-tauri")
  return {
    listen: (
      eventName: string,
      handler: (event: { payload: unknown }) => void
    ) => host.listen(eventName, handler),
  }
})

vi.mock("@tauri-apps/api/app", async () => {
  const { tauriTestHost: host } = await import("./support/mock-tauri")
  return {
    exit: (code?: number) => host.exit(code),
    onBackButtonPress: (handler: () => void) => host.registerBackButton(handler),
  }
})

vi.mock("@tauri-apps/plugin-os", async () => {
  const { tauriTestHost: host } = await import("./support/mock-tauri")
  return {
    platform: () => host.getPlatform(),
  }
})

vi.mock("@tauri-apps/api/webview", async () => {
  const { tauriTestHost: host } = await import("./support/mock-tauri")
  return {
    getCurrentWebview: () => ({
      onDragDropEvent: (handler: (event: { payload: unknown }) => void) =>
        host.registerDragDrop(handler),
    }),
  }
})

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    isFullscreen: vi.fn().mockResolvedValue(false),
    setFullscreen: vi.fn().mockResolvedValue(undefined),
    startDragging: vi.fn().mockResolvedValue(undefined),
    onResized: vi.fn().mockResolvedValue(() => {}),
  }),
}))

installMediaMocks()

if (typeof globalThis.ResizeObserver === "undefined") {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  globalThis.ResizeObserver = ResizeObserverStub
}

if (typeof Element !== "undefined" && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {}
}

if (typeof globalThis.PointerEvent === "undefined") {
  class PointerEventPolyfill extends MouseEvent {
    readonly pointerId: number
    readonly pointerType: string
    readonly isPrimary: boolean

    constructor(type: string, params: PointerEventInit = {}) {
      super(type, params)
      this.pointerId = params.pointerId ?? 1
      this.pointerType = params.pointerType ?? "mouse"
      this.isPrimary = params.isPrimary ?? true
    }
  }
  globalThis.PointerEvent = PointerEventPolyfill as typeof PointerEvent
}

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  resetErrorNotifications()
  resetBookImportState()
  setAppLocaleOverride(null)
  setCoverUrlBase("shiro-cover://localhost/book/")
  resetLibraryBridge()
  resetReaderBridge()
  resetReaderSettingsStoreForTest()
  localStorage.clear()
  resetMediaMocks()
  tauriTestHost.reset()
  resetBuilderIds()
})
