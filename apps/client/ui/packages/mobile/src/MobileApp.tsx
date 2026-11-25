import { useEffect } from "react"
import {
  AppRuntimeProvider,
  useAppRuntime,
} from "@shiro/ui-shared/context/app-runtime-context"
import { translateLocal, useTranslation } from "@shiro/ui-shared/lib/i18n"
import { MobileOverlayHost } from "./MobileOverlayHost"
import { useAndroidBackButton } from "./hooks/use-android-back-button"
import { MobileCloudGateScreen } from "./screens/CloudGate"
import { MobileDashboardScreen } from "./screens/Dashboard"
import { MobileSetupScreen } from "./screens/Setup"
import { MobileStartupErrorScreen } from "./screens/StartupError"

function MobileLoadingScreen() {
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

function MobileContent({ preview }: { preview: boolean }) {
  const { isTauri, isLoading, bootstrap } = useAppRuntime()
  const { t } = useTranslation()

  if (!isTauri) {
    if (preview) return <MobileDashboardScreen />
    return (
      <main className="grid min-h-dvh place-items-center p-4 text-center">
        <p>{t("local.runtime.native_required")}</p>
      </main>
    )
  }

  if (isLoading) {
    return <MobileLoadingScreen />
  }
  if (!bootstrap) return <MobileStartupErrorScreen />

  switch (bootstrap.phase) {
    case "setup_required":
      return <MobileSetupScreen />
    case "startup_failed":
      return <MobileStartupErrorScreen />
    case "cloud_protocol_incompatible":
    case "cloud_unreachable":
    case "cloud_auth_required":
      return <MobileCloudGateScreen />
    case "ready":
      return <MobileDashboardScreen />
  }
}

function MobileAppShell({ preview }: { preview: boolean }) {
  const { isTauri } = useAppRuntime()
  const { locale } = useTranslation()
  useAndroidBackButton(isTauri)

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en"
    document.title = translateLocal("local.app.name", locale)
  }, [locale])

  return (
    <>
      <MobileContent preview={preview} />
      <MobileOverlayHost />
    </>
  )
}

export function MobileApp({ preview = false }: { preview?: boolean }) {
  return (
    <AppRuntimeProvider>
      <MobileAppShell preview={preview} />
    </AppRuntimeProvider>
  )
}

export default MobileApp
