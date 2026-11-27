import { Suspense, lazy, useEffect, useRef } from "react"
import { createPortal } from "react-dom"
import { useQueryClient } from "@tanstack/react-query"
import { isTauriRuntime, type BootstrapState } from "@shiro/ui-shared/api/tauri-bridge"
import { ApiErrorAlerts } from "@shiro/ui-shared/components/api-error-alerts"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { useUploadQueueSync } from "@shiro/ui-shared/features/books/book-import-store"
import {
  mobileReaderStore,
  useMobileReaderStore,
} from "./reader/mobile-reader-store"
import { MobileUploadScreen } from "./screens/UploadScreen"

const LazyMobileReaderOverlay = lazy(() =>
  import("./reader/MobileReaderOverlay").then((mod) => ({
    default: mod.MobileReaderOverlay,
  }))
)

export function MobileOverlayHost() {
  const queryClient = useQueryClient()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null
  const readerState = useMobileReaderStore()

  const prevPhaseRef = useRef<BootstrapState["phase"] | null>(null)
  const prevModeRef = useRef<BootstrapState["activeMode"] | null>(null)
  const prevUserIdRef = useRef<string | null>(null)

  useEffect(() => {
    const nextPhase = bootstrap?.phase ?? null
    const nextMode = bootstrap?.activeMode ?? null
    const nextUserId = bootstrap?.cloud?.user?.userId ?? null

    const leavingReady =
      prevPhaseRef.current === "ready" && nextPhase !== "ready"
    const modeChanged =
      prevModeRef.current !== null &&
      nextMode !== null &&
      prevModeRef.current !== nextMode
    const userChanged =
      prevUserIdRef.current !== null && prevUserIdRef.current !== nextUserId

    if (leavingReady || modeChanged || userChanged) {
      mobileReaderStore.closeAllTabs()
    }

    prevPhaseRef.current = nextPhase
    prevModeRef.current = nextMode
    prevUserIdRef.current = nextUserId
  }, [bootstrap?.phase, bootstrap?.activeMode, bootstrap?.cloud?.user?.userId])

  const canSyncUploads =
    !isTauri ||
    (bootstrap?.canImportBooks === true && bootstrap.phase === "ready")

  useUploadQueueSync(queryClient, canSyncUploads, { enableMobileOpenUi: true })

  const shouldMountReader = readerState.isOpen || readerState.tabs.length > 0

  return (
    <>
      {shouldMountReader && (
        <Suspense fallback={null}>
          <LazyMobileReaderOverlay />
        </Suspense>
      )}
      <MobileUploadScreen />
      {createPortal(
        <div
          data-floating-dock=""
          style={{
            bottom:
              "calc(1rem + var(--shiro-safe-bottom, env(safe-area-inset-bottom, 0px)))",
          }}
          className="pointer-events-none fixed right-4 z-[100] flex max-h-[calc(100dvh-2rem)] w-max max-w-[calc(100vw-2rem)] flex-col items-end gap-3"
        >
          <ApiErrorAlerts />
        </div>,
        document.body
      )}
    </>
  )
}
