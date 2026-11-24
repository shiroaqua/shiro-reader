import { useEffect } from "react"
import { BottomRightOverlayDock } from "./components/bottom-right-overlay-dock"
import { CloudGateScreen } from "./screens/CloudGate"
import { DashboardScreen } from "./screens/Dashboard"
import { SetupScreen } from "./screens/Setup"
import { StartupErrorScreen } from "./screens/StartupError"
import {
  AppRuntimeProvider,
  useAppRuntime,
} from "./context/app-runtime-context"
import { translateLocal, useTranslation } from "./lib/i18n"
import { useAndroidBackButton } from "./hooks/use-android-back-button"

function LoadingScreen() {
  const { t } = useTranslation()
  return (
    <div className="flex min-h-dvh items-center justify-center bg-background">
      <div className="flex flex-col items-center gap-3">
        <div className="size-8 animate-spin rounded-full border-2 border-primary/20 border-t-primary" />
        <span className="text-sm text-muted-foreground">
          {t("local.common.loading")}
        </span>
      </div>
    </div>
  )
}

function AppContent({ preview }: { preview: boolean }) {
  const { isTauri, isLoading, bootstrap } = useAppRuntime()
  const { locale, t } = useTranslation()
  useAndroidBackButton(isTauri)

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en"
    document.title = translateLocal("local.app.name", locale)
  }, [locale])

  if (!isTauri) {
    if (preview) return <DashboardScreen />
    return (
      <main className="grid min-h-dvh place-items-center p-4 text-center">
        <p>{t("local.runtime.native_required")}</p>
      </main>
    )
  }

  if (isLoading) {
    return <LoadingScreen />
  }
  if (!bootstrap) return <StartupErrorScreen />

  switch (bootstrap.phase) {
    case "setup_required":
      return <SetupScreen />
    case "startup_failed":
      return <StartupErrorScreen />
    case "cloud_protocol_incompatible":
    case "cloud_unreachable":
    case "cloud_auth_required":
      return <CloudGateScreen />
    case "ready":
      return <DashboardScreen />
  }
}

export function App({ preview = false }: { preview?: boolean }) {
  return (
    <AppRuntimeProvider>
      <AppContent preview={preview} />
      <BottomRightOverlayDock />
    </AppRuntimeProvider>
  )
}

export default App
