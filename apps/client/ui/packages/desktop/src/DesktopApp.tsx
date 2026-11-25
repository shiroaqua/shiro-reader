import { useEffect } from "react"
import {
  AppRuntimeProvider,
  useAppRuntime,
} from "@shiro/ui-shared/context/app-runtime-context"
import { translateLocal, useTranslation } from "@shiro/ui-shared/lib/i18n"
import { DesktopOverlayDock } from "./DesktopOverlayDock"
import { DesktopCloudGateScreen } from "./screens/CloudGate"
import { DesktopDashboardScreen } from "./screens/Dashboard"
import { DesktopSetupScreen } from "./screens/Setup"
import { DesktopStartupErrorScreen } from "./screens/StartupError"

function DesktopLoadingScreen() {
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

function DesktopContent({ preview }: { preview: boolean }) {
  const { isTauri, isLoading, bootstrap } = useAppRuntime()
  const { t } = useTranslation()

  if (!isTauri) {
    if (preview) return <DesktopDashboardScreen />
    return (
      <main className="grid min-h-dvh place-items-center p-4 text-center">
        <p>{t("local.runtime.native_required")}</p>
      </main>
    )
  }

  if (isLoading) {
    return <DesktopLoadingScreen />
  }
  if (!bootstrap) return <DesktopStartupErrorScreen />

  switch (bootstrap.phase) {
    case "setup_required":
      return <DesktopSetupScreen />
    case "startup_failed":
      return <DesktopStartupErrorScreen />
    case "cloud_protocol_incompatible":
    case "cloud_unreachable":
    case "cloud_auth_required":
      return <DesktopCloudGateScreen />
    case "ready":
      return <DesktopDashboardScreen />
  }
}

function DesktopAppShell({ preview }: { preview: boolean }) {
  const { locale } = useTranslation()

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en"
    document.title = translateLocal("local.app.name", locale)
  }, [locale])

  return (
    <>
      <DesktopContent preview={preview} />
      <DesktopOverlayDock />
    </>
  )
}

export function DesktopApp({ preview = false }: { preview?: boolean }) {
  return (
    <AppRuntimeProvider>
      <DesktopAppShell preview={preview} />
    </AppRuntimeProvider>
  )
}

export default DesktopApp
