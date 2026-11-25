import type { Platform } from "@tauri-apps/plugin-os"
import type {
  BootstrapState,
  UploadPanelSnapshot,
} from "@shiro/ui-shared/api/tauri-bridge"
import type { RustErrorEvent } from "@shiro/ui-shared/lib/error-notifications"
import type { FakeLibrary } from "./fake-library"

const RUST_ERROR_EVENT = "shiro://error"
const RUST_RUNTIME_STATE_EVENT = "shiro://runtime-state"
const RUST_UPLOAD_STATE_EVENT = "shiro://upload-state"
const RUST_UPLOAD_OPEN_UI_EVENT = "shiro://upload-open-ui"

export type DragDropPayload =
  | { type: "enter"; paths: string[]; position: { x: number; y: number } }
  | { type: "over"; position: { x: number; y: number } }
  | { type: "drop"; paths: string[]; position: { x: number; y: number } }
  | { type: "leave" }

export class FakeChannel<T = unknown> {
  id = 1
  onmessage: (response: T) => void

  constructor(onmessage?: (response: T) => void) {
    this.onmessage = onmessage ?? (() => {})
  }
}

export type InvokeCall = {
  command: string
  args?: Record<string, unknown> | undefined
}

type InvokeHandler = (args?: Record<string, unknown>) => unknown
type EventListener = (event: { payload: unknown }) => void
type BackListener = () => void

class TauriTestHost {
  private tauriEnabled = true
  private currentPlatform: Platform = "linux"
  private handlers = new Map<string, InvokeHandler>()
  private recordedCalls: InvokeCall[] = []
  private eventListeners = new Map<string, Set<EventListener>>()
  private backListeners = new Set<BackListener>()
  private deferBackRegistration = false
  private pendingBackRegistrations: (() => void)[] = []
  private unregisteredBackListeners = 0
  private dragDropListeners = new Set<EventListener>()
  private deferDragDropRegistration = false
  private pendingDragDropRegistrations: (() => void)[] = []
  exitCalls: number[] = []

  setTauriRuntime(enabled: boolean) {
    this.tauriEnabled = enabled
  }

  isTauriRuntime(): boolean {
    return this.tauriEnabled
  }

  setPlatform(os: Platform) {
    this.currentPlatform = os
  }

  getPlatform(): Platform {
    return this.currentPlatform
  }

  onInvoke(command: string, handler: InvokeHandler) {
    this.handlers.set(command, handler)
  }

  attachLibrary(fakeLibrary: FakeLibrary) {
    for (const [command, handler] of Object.entries(
      fakeLibrary.createTauriInvokeHandlers()
    )) {
      this.handlers.set(command, handler)
    }
  }

  async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    this.recordedCalls.push({ command, args })
    const handler = this.handlers.get(command)
    if (!handler) {
      throw new Error(`Unhandled Tauri invoke command in test: ${command}`)
    }
    return (await handler(args)) as T
  }

  invokeCalls(command?: string): InvokeCall[] {
    if (!command) return [...this.recordedCalls]
    return this.recordedCalls.filter((call) => call.command === command)
  }

  clearInvokeCalls() {
    this.recordedCalls = []
  }

  async listen(
    eventName: string,
    listener: EventListener
  ): Promise<() => void> {
    const listeners = this.eventListeners.get(eventName) ?? new Set()
    listeners.add(listener)
    this.eventListeners.set(eventName, listeners)
    return () => {
      listeners.delete(listener)
    }
  }

  emitEvent(eventName: string, payload: unknown) {
    for (const listener of this.eventListeners.get(eventName) ?? []) {
      listener({ payload })
    }
  }

  emitRustError(event: RustErrorEvent) {
    this.emitEvent(RUST_ERROR_EVENT, event)
  }

  emitRuntimeState(state: BootstrapState) {
    this.emitEvent(RUST_RUNTIME_STATE_EVENT, state)
  }

  emitUploadState(snapshot: UploadPanelSnapshot) {
    this.emitEvent(RUST_UPLOAD_STATE_EVENT, snapshot)
  }

  emitUploadOpenUi() {
    this.emitEvent(RUST_UPLOAD_OPEN_UI_EVENT, undefined)
  }

  setDeferBackRegistration(deferred: boolean) {
    this.deferBackRegistration = deferred
  }

  flushBackRegistrations() {
    const pending = [...this.pendingBackRegistrations]
    this.pendingBackRegistrations = []
    for (const resolve of pending) {
      resolve()
    }
  }

  async registerBackButton(listener: BackListener) {
    if (this.deferBackRegistration) {
      await new Promise<void>((resolve) => {
        this.pendingBackRegistrations.push(resolve)
      })
    }
    this.backListeners.add(listener)
    return {
      unregister: async () => {
        this.backListeners.delete(listener)
        this.unregisteredBackListeners += 1
      },
    }
  }

  triggerAndroidBack() {
    for (const listener of [...this.backListeners]) {
      listener()
    }
  }

  backListenerCount(): number {
    return this.backListeners.size
  }

  backUnregisterCount(): number {
    return this.unregisteredBackListeners
  }

  setDeferDragDropRegistration(deferred: boolean) {
    this.deferDragDropRegistration = deferred
  }

  flushDragDropRegistrations() {
    const pending = [...this.pendingDragDropRegistrations]
    this.pendingDragDropRegistrations = []
    for (const resolve of pending) {
      resolve()
    }
  }

  async registerDragDrop(listener: EventListener): Promise<() => void> {
    if (this.deferDragDropRegistration) {
      await new Promise<void>((resolve) => {
        this.pendingDragDropRegistrations.push(resolve)
      })
    }
    this.dragDropListeners.add(listener)
    return () => {
      this.dragDropListeners.delete(listener)
    }
  }

  emitDragDrop(payload: DragDropPayload) {
    for (const listener of [...this.dragDropListeners]) {
      listener({ payload })
    }
  }

  dragDropListenerCount(): number {
    return this.dragDropListeners.size
  }

  async exit(code = 0) {
    this.exitCalls.push(code)
  }

  reset() {
    this.tauriEnabled = true
    this.currentPlatform = "linux"
    this.handlers.clear()
    this.recordedCalls = []
    this.eventListeners.clear()
    this.backListeners.clear()
    this.deferBackRegistration = false
    this.pendingBackRegistrations = []
    this.unregisteredBackListeners = 0
    this.dragDropListeners.clear()
    this.deferDragDropRegistration = false
    this.pendingDragDropRegistrations = []
    this.exitCalls = []
  }
}

export const tauriTestHost = new TauriTestHost()
