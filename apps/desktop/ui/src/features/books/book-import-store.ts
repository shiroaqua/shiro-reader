import { useSyncExternalStore } from "react"
import type { QueryClient } from "@tanstack/react-query"
import { libraryBridge } from "@/api/library"
import type { ImportBooksResult, ImportProgressEvent } from "@/api/tauri-bridge"
import { booksQueryKey } from "@/lib/query-keys"

export type ImportItemProgress = Extract<
  ImportProgressEvent,
  { stage: "itemProgress" }
>

export interface BookImportSnapshot {
  pending: boolean
  progress: ImportItemProgress | null
  summary: ImportBooksResult | null
  canCancel: boolean
  cancelling: boolean
}

const INITIAL_SNAPSHOT: BookImportSnapshot = {
  pending: false,
  progress: null,
  summary: null,
  canCancel: false,
  cancelling: false,
}

let snapshot: BookImportSnapshot = INITIAL_SNAPSHOT
let generation = 0
const listeners = new Set<() => void>()

function updateSnapshot(patch: Partial<BookImportSnapshot>) {
  snapshot = { ...snapshot, ...patch }
  listeners.forEach((listener) => listener())
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): BookImportSnapshot {
  return snapshot
}

export function useBookImportState(): BookImportSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

export function resetBookImportState() {
  generation += 1
  if (snapshot === INITIAL_SNAPSHOT) return
  snapshot = INITIAL_SNAPSHOT
  listeners.forEach((listener) => listener())
}

export async function startBookImport(queryClient: QueryClient): Promise<void> {
  if (snapshot.pending) return

  const runGeneration = generation
  let importedAny = false
  updateSnapshot({
    pending: true,
    progress: null,
    summary: null,
    canCancel: false,
    cancelling: false,
  })

  try {
    const result = await libraryBridge.pickAndImportBooks((event) => {
      if (generation !== runGeneration) return
      switch (event.stage) {
        case "started":
          updateSnapshot({ canCancel: true })
          break
        case "itemProgress":
          updateSnapshot({ progress: event })
          break
        case "itemSucceeded":
          importedAny = true
          void queryClient.invalidateQueries({ queryKey: booksQueryKey })
          break
        case "finished":
          updateSnapshot({ canCancel: false })
          break
      }
    })
    if (generation !== runGeneration) return
    importedAny ||= result.imported.length > 0
    if (result.failed.length || result.skipped.length) {
      updateSnapshot({ summary: result })
    }
  } catch {
    // Rust host emits localized shiro://error notification.
  } finally {
    if (generation === runGeneration) {
      updateSnapshot({
        pending: false,
        progress: null,
        canCancel: false,
        cancelling: false,
      })
      if (importedAny) {
        await queryClient.invalidateQueries({ queryKey: booksQueryKey })
      }
    }
  }
}

export async function cancelActiveBookImport(): Promise<void> {
  if (!snapshot.pending || snapshot.cancelling) return
  updateSnapshot({ cancelling: true })
  try {
    if (!(await libraryBridge.cancelBookImport())) {
      updateSnapshot({ cancelling: false })
    }
  } catch {
    updateSnapshot({ cancelling: false })
  }
}
