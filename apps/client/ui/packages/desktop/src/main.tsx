import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { ErrorBoundary } from "@shiro/ui-shared/components/error-boundary"
import "./index.css"
import DesktopApp from "./DesktopApp.tsx"

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: false,
    },
  },
})

function render(preview = false) {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <DesktopApp preview={preview} />
        </QueryClientProvider>
      </ErrorBoundary>
    </StrictMode>
  )
}

if (import.meta.env.DEV && import.meta.env.MODE === "mock" && !isTauriRuntime()) {
  void import("@shiro/ui-shared/api/library-mock").then(
    ({ enableLibraryPreview }) => {
      enableLibraryPreview()
      render(true)
    }
  )
} else {
  render()
}
