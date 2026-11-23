import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import "./index.css"
import App from "./App.tsx"
import { ApiErrorAlerts } from "@/components/api-error-alerts"
import { ErrorBoundary } from "@/components/error-boundary"
import { isTauriRuntime } from "@/api/tauri-bridge"

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
          <App preview={preview} />
          <ApiErrorAlerts />
        </QueryClientProvider>
      </ErrorBoundary>
    </StrictMode>
  )
}

if (import.meta.env.DEV && import.meta.env.MODE === "mock" && !isTauriRuntime()) {
  void import("@/api/library-mock").then(({ enableLibraryPreview }) => {
    enableLibraryPreview()
    render(true)
  })
} else {
  render()
}
