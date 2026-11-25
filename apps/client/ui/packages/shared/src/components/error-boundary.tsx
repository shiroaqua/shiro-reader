import { Component, type ErrorInfo, type ReactNode } from "react"
import { CircleAlert, RotateCcw } from "lucide-react"
import { EmptyState } from "@shiro/ui-shared/components/empty-state"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

interface ErrorBoundaryState {
  hasError: boolean
}

function ErrorFallback({ onReload }: { onReload: () => void }) {
  const { t } = useTranslation()

  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-4">
      <EmptyState
        tone="destructive"
        icon={<CircleAlert aria-hidden="true" />}
        title={t("local.common.unexpected_error")}
        description={t("local.common.unexpected_error_hint")}
        action={
          <Button type="button" variant="outline" size="sm" onClick={onReload}>
            <RotateCcw aria-hidden="true" />
            {t("local.common.reload")}
          </Button>
        }
      />
    </div>
  )
}

export class ErrorBoundary extends Component<
  { children: ReactNode },
  ErrorBoundaryState
> {
  override state: ErrorBoundaryState = { hasError: false }

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true }
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Uncaught render error:", error, info.componentStack)
  }

  override render() {
    if (this.state.hasError) {
      return <ErrorFallback onReload={() => window.location.reload()} />
    }
    return this.props.children
  }
}
