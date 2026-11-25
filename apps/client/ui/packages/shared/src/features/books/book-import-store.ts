import { useEffect, useRef, useSyncExternalStore } from "react"
import type { QueryClient } from "@tanstack/react-query"
import { libraryBridge } from "@shiro/ui-shared/api/library"
import type { UploadPanelSnapshot } from "@shiro/ui-shared/api/tauri-bridge"
import { booksQueryKey } from "@shiro/ui-shared/lib/query-keys"

const INVALIDATE_THROTTLE_MS = 150

export const INITIAL_UPLOAD_SNAPSHOT: UploadPanelSnapshot = {
  revision: 0,
  visible: false,
  concurrency: 4,
  completedCounter: 0,
  stats: {
    total: 0,
    remaining: 0,
    active: 0,
    succeeded: 0,
    failed: 0,
  },
  hasDismissibleErrors: false,
  tasks: [],
}

export interface BookImportViewState {
  upload: UploadPanelSnapshot
  expanded: boolean
  mobileOpen: boolean
  optionsOpen: boolean
  pickerOpening: boolean
}

const INITIAL_VIEW_STATE: BookImportViewState = {
  upload: INITIAL_UPLOAD_SNAPSHOT,
  expanded: true,
  mobileOpen: false,
  optionsOpen: false,
  pickerOpening: false,
}

let state: BookImportViewState = INITIAL_VIEW_STATE
let hydrated = false
const listeners = new Set<() => void>()

function emitChange() {
  listeners.forEach((listener) => listener())
}

function updateView(patch: Partial<BookImportViewState>) {
  state = { ...state, ...patch }
  emitChange()
}

export function applyUploadSnapshot(next: UploadPanelSnapshot) {
  if (next.revision < state.upload.revision) return
  if (next.revision === state.upload.revision && hydrated) return
  hydrated = true
  const wasVisible = state.upload.visible
  const shouldCloseMobile = !next.visible && state.mobileOpen
  updateView({
    upload: next,
    expanded: !wasVisible && next.visible ? true : state.expanded,
    mobileOpen: next.visible ? state.mobileOpen : false,
    optionsOpen: next.visible ? state.optionsOpen : false,
  })
  if (shouldCloseMobile) {
    void libraryBridge.uploadSetMobileUiOpen(false).catch(() => {})
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): BookImportViewState {
  return state
}

export function useBookImportState(): BookImportViewState {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

export function resetBookImportState() {
  hydrated = false
  if (state === INITIAL_VIEW_STATE) return
  state = INITIAL_VIEW_STATE
  emitChange()
}

export function setUploadPanelExpanded(expanded: boolean) {
  if (state.expanded === expanded) return
  updateView({
    expanded,
    optionsOpen: expanded ? state.optionsOpen : false,
  })
}

export function openMobileUploadPanel() {
  if (!state.mobileOpen) {
    updateView({ mobileOpen: true })
  }
  void libraryBridge.uploadSetMobileUiOpen(true).catch(() => {})
}

export function closeMobileUploadPanel() {
  if (!state.mobileOpen && !state.optionsOpen) {
    void libraryBridge.uploadSetMobileUiOpen(false).catch(() => {})
    return
  }
  updateView({
    mobileOpen: false,
    optionsOpen: false,
  })
  void libraryBridge.uploadSetMobileUiOpen(false).catch(() => {})
}

export function toggleUploadOptions() {
  updateView({ optionsOpen: !state.optionsOpen })
}

export async function pickAndEnqueueBooks(): Promise<void> {
  if (state.pickerOpening) return
  updateView({ pickerOpening: true })
  try {
    const next = await libraryBridge.uploadPickAndEnqueue()
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  } finally {
    updateView({ pickerOpening: false })
  }
}

export async function enqueueUploadPaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return
  try {
    const next = await libraryBridge.uploadEnqueuePaths(paths)
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

export async function retryUploadTask(taskId: number): Promise<void> {
  try {
    const next = await libraryBridge.uploadRetryTask(taskId)
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

export async function cancelUploadTask(taskId: number): Promise<void> {
  try {
    const next = await libraryBridge.uploadCancelTask(taskId)
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

export async function cancelAllUploads(): Promise<void> {
  try {
    const next = await libraryBridge.uploadCancelAll()
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

export async function dismissUploadTask(taskId: number): Promise<void> {
  try {
    const next = await libraryBridge.uploadDismissTask(taskId)
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

export async function dismissUploadErrors(): Promise<void> {
  try {
    const next = await libraryBridge.uploadDismissErrors()
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

export async function setUploadConcurrency(concurrency: number): Promise<void> {
  const clamped = Math.min(16, Math.max(1, Math.floor(concurrency)))
  try {
    const next = await libraryBridge.uploadSetConcurrency(clamped)
    applyUploadSnapshot(next)
  } catch {
    // Rust host emits localized shiro://error notification.
  }
}

/**
 * Subscribes to `libraryBridge.listenUploadState` (covering both Tauri and mock preview),
 * hydrates initial state, and invalidates bookshelf queries with 150ms throttling whenever
 * the monotonic `completedCounter` advances (flushing immediately once `remaining === 0`).
 */
export function useUploadQueueSync(
  queryClient: QueryClient,
  enabled: boolean,
  options?: { enableMobileOpenUi?: boolean }
) {
  const enableMobileOpenUi = options?.enableMobileOpenUi ?? false
  const wasEnabledRef = useRef(enabled)

  useEffect(() => {
    const wasEnabled = wasEnabledRef.current
    wasEnabledRef.current = enabled
    if (!enabled) {
      if (wasEnabled) {
        if (state.mobileOpen) {
          void libraryBridge.uploadSetMobileUiOpen(false).catch(() => {})
        }
        resetBookImportState()
      }
      return
    }
    let active = true
    let unlisten: (() => void) | undefined
    let unlistenOpenUi: (() => void) | undefined
    let lastCompleted = state.upload.completedCounter
    let pendingTimer: ReturnType<typeof setTimeout> | null = null
    let needsInvalidate = false

    const flushInvalidation = () => {
      if (pendingTimer !== null) {
        clearTimeout(pendingTimer)
        pendingTimer = null
      }
      if (!needsInvalidate) return
      needsInvalidate = false
      void queryClient.invalidateQueries({ queryKey: booksQueryKey })
    }

    const checkCompletedCounter = (next: UploadPanelSnapshot) => {
      if (!active) return
      if (next.completedCounter > lastCompleted) {
        lastCompleted = next.completedCounter
        needsInvalidate = true
        if (next.stats.remaining === 0) {
          flushInvalidation()
        } else if (pendingTimer === null) {
          pendingTimer = setTimeout(flushInvalidation, INVALIDATE_THROTTLE_MS)
        }
      } else if (needsInvalidate && next.stats.remaining === 0) {
        flushInvalidation()
      }
    }

    const unsubscribeStore = subscribe(() => {
      checkCompletedCounter(state.upload)
    })

    const handleSnapshot = (next: UploadPanelSnapshot) => {
      if (!active) return
      applyUploadSnapshot(next)
      checkCompletedCounter(state.upload)
    }

    const init = async () => {
      try {
        const stop = await libraryBridge.listenUploadState(handleSnapshot)
        if (!active) {
          stop()
          return
        }
        unlisten = stop
        if (enableMobileOpenUi) {
          const stopOpenUi = await libraryBridge.listenUploadOpenUi(() => {
            if (!active) return
            openMobileUploadPanel()
          })
          if (!active) {
            stopOpenUi()
            return
          }
          unlistenOpenUi = stopOpenUi
        }
        const initial = await libraryBridge.uploadGetState()
        if (!active) return
        handleSnapshot(initial)
        if (enableMobileOpenUi) {
          const pending = await libraryBridge.uploadConsumePendingOpenUi()
          if (active && pending) {
            openMobileUploadPanel()
          }
        }
      } catch {
        // Ignore bridge initialization failure outside supported runtimes.
      }
    }

    void init()

    return () => {
      active = false
      unsubscribeStore()
      flushInvalidation()
      unlisten?.()
      unlistenOpenUi?.()
    }
  }, [queryClient, enabled, enableMobileOpenUi])
}
