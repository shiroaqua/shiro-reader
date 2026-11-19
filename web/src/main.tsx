import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import "./index.css"
import App from "./App.tsx"
import { ApiErrorAlerts } from "@/components/api-error-alerts"
import { ErrorBoundary } from "@/components/error-boundary"

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
})

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <App />
        <ApiErrorAlerts />
      </QueryClientProvider>
    </ErrorBoundary>
  </StrictMode>
)
