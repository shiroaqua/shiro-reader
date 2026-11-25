import { createPortal } from "react-dom"
import { useQueryClient } from "@tanstack/react-query"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { ApiErrorAlerts } from "@shiro/ui-shared/components/api-error-alerts"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { useUploadQueueSync } from "@shiro/ui-shared/features/books/book-import-store"
import { UploadPanel } from "./components/upload-panel"

export function DesktopOverlayDock() {
  const queryClient = useQueryClient()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null

  const canSyncUploads =
    !isTauri ||
    (bootstrap?.canImportBooks === true && bootstrap.phase === "ready")

  useUploadQueueSync(queryClient, canSyncUploads)

  return createPortal(
    <div
      data-floating-dock=""
      className="pointer-events-none fixed right-4 bottom-4 z-[100] flex max-h-[calc(100dvh-2rem)] w-max max-w-[calc(100vw-2rem)] flex-col items-end gap-3"
    >
      <ApiErrorAlerts />
      <UploadPanel />
    </div>,
    document.body
  )
}
