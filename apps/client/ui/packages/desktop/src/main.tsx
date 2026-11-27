import { StrictMode, Suspense, lazy } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { ErrorBoundary } from "@shiro/ui-shared/components/error-boundary"
import "./index.css"

const LazyDesktopApp = lazy(() => import("./DesktopApp.tsx"))

const LazyDesktopReaderApp = lazy(() =>
  import("./reader/DesktopReaderApp.tsx").then((mod) => ({
    default: mod.DesktopReaderApp,
  }))
)

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: false,
    },
  },
})

function resolveReaderWindowId(): string | null {
  if (typeof window === "undefined") return null
  const params = new URLSearchParams(window.location.search)
  if (params.get("window") !== "reader") return null
  const windowId = params.get("windowId")?.trim()
  return windowId && windowId.length > 0 ? windowId : null
}

function render(preview = false) {
  const readerWindowId = resolveReaderWindowId()
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <Suspense fallback={null}>
            {readerWindowId ? (
              <LazyDesktopReaderApp windowId={readerWindowId} />
            ) : (
              <LazyDesktopApp preview={preview} />
            )}
          </Suspense>
        </QueryClientProvider>
      </ErrorBoundary>
    </StrictMode>
  )
}

if (import.meta.env.DEV && import.meta.env.MODE === "mock" && !isTauriRuntime()) {
  void Promise.all([
    import("@shiro/ui-shared/api/library-mock"),
    import("@shiro/ui-shared/api/reader-mock"),
  ]).then(([{ enableLibraryPreview }, { enableReaderPreview }]) => {
    enableLibraryPreview()
    enableReaderPreview()
    render(true)
  })
} else {
  render()
}
