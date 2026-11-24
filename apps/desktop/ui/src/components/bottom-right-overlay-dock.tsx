import { createPortal } from "react-dom"
import { useQueryClient } from "@tanstack/react-query"
import { isTauriRuntime } from "@/api/tauri-bridge"
import { ApiErrorAlerts } from "@/components/api-error-alerts"
import { useOptionalAppRuntime } from "@/context/app-runtime-context"
import { useUploadQueueSync } from "@/features/books/book-import-store"
import { MobileUploadScreen } from "@/features/books/mobile-upload-screen"
import { UploadPanel } from "@/features/books/upload-panel"

export function BottomRightOverlayDock() {
  const queryClient = useQueryClient()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null

  const canSyncUploads =
    !isTauri ||
    (bootstrap?.canImportBooks === true && bootstrap.phase === "ready")

  useUploadQueueSync(queryClient, canSyncUploads)

  return (
    <>
      <MobileUploadScreen />
      {createPortal(
        <div
          data-floating-dock=""
          className="pointer-events-none fixed right-4 bottom-4 z-[100] flex max-h-[calc(100dvh-2rem)] w-max max-w-[calc(100vw-2rem)] flex-col items-end gap-3"
        >
          <ApiErrorAlerts />
          <UploadPanel />
        </div>,
        document.body
      )}
    </>
  )
}
