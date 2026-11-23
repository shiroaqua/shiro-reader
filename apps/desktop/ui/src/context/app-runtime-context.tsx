import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  isTauriRuntime,
  listenRuntimeState,
  listenRustErrors,
  setCoverUrlBase,
  tauriBridge,
  type BootstrapState,
  type CloudOriginCheckResult,
  type InitialSetupRequest,
  type UpdateSettingsRequest,
} from "@/api/tauri-bridge"
import { resetBookImportState } from "@/features/books/book-import-store"
import {
  markRustErrorHandled,
  notifyLocalError,
  notifyRustError,
} from "@/lib/error-notifications"
import { setAppLocaleOverride, type Locale } from "@/lib/i18n"

interface AppRuntimeContextValue {
  isTauri: boolean
  isLoading: boolean
  bootstrap: BootstrapState | null
  completeSetup: (request: InitialSetupRequest) => Promise<BootstrapState>
  updateSettings: (request: UpdateSettingsRequest) => Promise<BootstrapState>
  changeLocale: (locale: Locale) => Promise<BootstrapState>
  pickLibraryDir: () => Promise<string | null>
  repairLibraryRoot: (path: string) => Promise<BootstrapState>
  retryStartup: () => Promise<BootstrapState | void>
  cloudCheckOrigin: (origin: string) => Promise<CloudOriginCheckResult>
  cloudProbe: () => Promise<BootstrapState>
  cloudUpdateOrigin: (origin: string) => Promise<BootstrapState>
  cloudLogin: (username: string, password: string) => Promise<BootstrapState>
  cloudLogout: () => Promise<BootstrapState>
  exitApp: () => Promise<void>
}

const AppRuntimeContext = createContext<AppRuntimeContextValue | undefined>(
  undefined
)

export function AppRuntimeProvider({ children }: { children: ReactNode }) {
  const isTauri = isTauriRuntime()
  const queryClient = useQueryClient()
  const [bootstrap, setBootstrap] = useState<BootstrapState | null>(null)
  const [isLoading, setIsLoading] = useState(isTauri)
  const [bootAttempt, setBootAttempt] = useState(0)
  const lastUserIdRef = useRef<string | null>(null)
  const lastPhaseRef = useRef<BootstrapState["phase"] | null>(null)

  const applyBootstrap = useCallback(
    (next: BootstrapState, replayBuffered = false) => {
      setAppLocaleOverride(next.locale)
      setCoverUrlBase(next.coverUrlBase)

      // Clear cached React Query library data whenever leaving ready state or switching cloud user.
      const nextUserId = next.cloud?.user?.userId ?? null
      const leavingReady =
        lastPhaseRef.current === "ready" && next.phase !== "ready"
      const userChanged =
        lastUserIdRef.current !== null && lastUserIdRef.current !== nextUserId
      if (leavingReady || userChanged) {
        queryClient.clear()
        resetBookImportState()
      }
      lastPhaseRef.current = next.phase
      lastUserIdRef.current = nextUserId

      if (replayBuffered) {
        if (next.phase === "startup_failed" && next.startupError) {
          markRustErrorHandled(next.startupError.errorId)
        }
        if (next.phase !== "ready" && next.cloud?.diagnostic) {
          markRustErrorHandled(next.cloud.diagnostic.errorId)
        }
        for (const buffered of next.bufferedErrors) {
          notifyRustError(buffered)
        }
      }
      setBootstrap(next)
    },
    [queryClient]
  )

  useEffect(() => {
    if (!isTauri) return
    let active = true
    let unlistenError: (() => void) | undefined
    let unlistenState: (() => void) | undefined

    const boot = async () => {
      try {
        // Register event listeners BEFORE requesting bootstrap state so no early events are lost.
        unlistenError = await listenRustErrors((event) => {
          if (active) notifyRustError(event)
        })
        if (!active) {
          unlistenError()
          return
        }
        unlistenState = await listenRuntimeState((state) => {
          if (!active) return
          applyBootstrap(state, false)
        })
        if (!active) {
          unlistenState()
          return
        }
        const initial = await tauriBridge.getBootstrapState()
        if (!active) return
        applyBootstrap(initial, true)
      } catch {
        if (active) notifyLocalError("local.common.error.unknown")
      } finally {
        if (active) setIsLoading(false)
      }
    }

    void boot()
    return () => {
      active = false
      unlistenError?.()
      unlistenState?.()
    }
  }, [isTauri, applyBootstrap, bootAttempt])

  const completeSetup = useCallback(
    async (request: InitialSetupRequest) => {
      const next = await tauriBridge.completeInitialSetup(request)
      queryClient.clear()
      applyBootstrap(next)
      return next
    },
    [applyBootstrap, queryClient]
  )

  const updateSettings = useCallback(
    async (request: UpdateSettingsRequest) => {
      const next = await tauriBridge.updateSettings(request)
      applyBootstrap(next)
      return next
    },
    [applyBootstrap]
  )

  const changeLocale = useCallback(
    async (locale: Locale) => {
      const next = await tauriBridge.setAppLocale(locale)
      applyBootstrap(next)
      return next
    },
    [applyBootstrap]
  )

  const pickLibraryDir = useCallback(() => tauriBridge.pickLibraryDir(), [])

  const repairLibraryRoot = useCallback(
    async (path: string) => {
      const next = await tauriBridge.repairLocalLibraryRoot(path)
      queryClient.clear()
      applyBootstrap(next)
      return next
    },
    [applyBootstrap, queryClient]
  )

  const retryStartup = useCallback(async () => {
    if (!bootstrap) {
      setIsLoading(true)
      setBootAttempt((attempt) => attempt + 1)
      return
    }
    const next = await tauriBridge.retryStartup()
    queryClient.clear()
    applyBootstrap(next)
    return next
  }, [applyBootstrap, bootstrap, queryClient])

  const cloudCheckOrigin = useCallback(
    (origin: string) => tauriBridge.cloudCheckOrigin(origin),
    []
  )

  const cloudProbe = useCallback(async () => {
    const next = await tauriBridge.cloudProbe()
    applyBootstrap(next)
    return next
  }, [applyBootstrap])

  const cloudUpdateOrigin = useCallback(
    async (origin: string) => {
      const next = await tauriBridge.cloudUpdateOrigin(origin)
      queryClient.clear()
      applyBootstrap(next)
      return next
    },
    [applyBootstrap, queryClient]
  )

  const cloudLogin = useCallback(
    async (username: string, password: string) => {
      const next = await tauriBridge.cloudLogin(username, password)
      queryClient.clear()
      applyBootstrap(next)
      return next
    },
    [applyBootstrap, queryClient]
  )

  const cloudLogout = useCallback(async () => {
    const next = await tauriBridge.cloudLogout()
    queryClient.clear()
    applyBootstrap(next)
    return next
  }, [applyBootstrap, queryClient])

  const exitApp = useCallback(() => tauriBridge.exitApp(), [])

  return (
    <AppRuntimeContext.Provider
      value={{
        isTauri,
        isLoading,
        bootstrap,
        completeSetup,
        updateSettings,
        changeLocale,
        pickLibraryDir,
        repairLibraryRoot,
        retryStartup,
        cloudCheckOrigin,
        cloudProbe,
        cloudUpdateOrigin,
        cloudLogin,
        cloudLogout,
        exitApp,
      }}
    >
      {children}
    </AppRuntimeContext.Provider>
  )
}

export function useAppRuntime() {
  const context = useContext(AppRuntimeContext)
  if (!context) {
    throw new Error("useAppRuntime must be used within AppRuntimeProvider")
  }
  return context
}
