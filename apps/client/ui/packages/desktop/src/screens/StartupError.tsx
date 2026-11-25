import { useState } from "react"
import {
  AlertTriangle,
  FolderOpen,
  LoaderCircle,
  Power,
  RotateCcw,
} from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@shiro/ui-shared/components/ui/alert"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { useTranslation, type Locale } from "@shiro/ui-shared/lib/i18n"

export function DesktopStartupErrorScreen() {
  const { t, locale } = useTranslation()
  const {
    bootstrap,
    changeLocale,
    completeSetup,
    exitApp,
    pickLibraryDir,
    repairLibraryRoot,
    retryStartup,
  } = useAppRuntime()
  const [busy, setBusy] = useState(false)

  const diagnostic = bootstrap?.startupError
  const canPickDir = bootstrap?.canPickLibraryDir ?? false
  const canReconfigureLocal =
    diagnostic?.key === "client.config.invalid" ||
    diagnostic?.key === "client.cloud.invalid_origin"

  const handleLocale = async (nextLocale: Locale) => {
    try {
      await changeLocale(nextLocale)
    } catch {
      // Host emits localized error event.
    }
  }

  const handleRepairDir = async () => {
    if (busy) return
    setBusy(true)
    try {
      const picked = await pickLibraryDir()
      if (picked) {
        await repairLibraryRoot(picked)
      }
    } catch {
      // Host emits localized error event; remain on recovery screen.
    } finally {
      setBusy(false)
    }
  }

  const handleRetry = async () => {
    if (busy) return
    setBusy(true)
    try {
      await retryStartup()
    } catch {
      // Host emits localized error event; updated diagnostic is shown.
    } finally {
      setBusy(false)
    }
  }

  const handleReconfigureLocal = async () => {
    if (busy) return
    setBusy(true)
    try {
      await completeSetup({
        mode: "local",
        locale,
        localLibraryRoot: null,
        cloudOrigin: null,
      })
    } catch {
      // Host emits localized error event.
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex min-h-dvh items-center justify-center bg-background p-6">
      <div className="w-full max-w-lg space-y-5 rounded-2xl border bg-card p-6 shadow-lg">
        <div className="flex items-start justify-between gap-3">
          <div className="space-y-1">
            <h1 className="text-xl font-semibold tracking-tight text-destructive">
              {t("local.startup_error.title")}
            </h1>
            <p className="text-sm text-muted-foreground">
              {t("local.startup_error.subtitle")}
            </p>
          </div>
          <div className="flex shrink-0 gap-1">
            <Button
              type="button"
              size="xs"
              variant={locale === "zh" ? "secondary" : "ghost"}
              onClick={() => void handleLocale("zh")}
              disabled={busy}
            >
              中文
            </Button>
            <Button
              type="button"
              size="xs"
              variant={locale === "en" ? "secondary" : "ghost"}
              onClick={() => void handleLocale("en")}
              disabled={busy}
            >
              EN
            </Button>
          </div>
        </div>

        <Alert variant="destructive">
          <AlertTriangle />
          <AlertTitle>
            {diagnostic
              ? `${t("local.startup_error.code")}: ${diagnostic.code}`
              : t("local.common.error")}
          </AlertTitle>
          <AlertDescription className="space-y-2">
            <p>{diagnostic?.message ?? t("local.common.error.unknown")}</p>
            {diagnostic?.path && (
              <div className="text-xs">
                <span className="font-medium">
                  {t("local.startup_error.path")}:{" "}
                </span>
                <span className="font-mono break-all">{diagnostic.path}</span>
              </div>
            )}
            {diagnostic?.detail && (
              <details className="text-xs opacity-85">
                <summary className="cursor-pointer font-medium">
                  {t("local.startup_error.detail")}
                </summary>
                <pre className="mt-1 overflow-x-auto whitespace-pre-wrap font-mono">
                  {diagnostic.detail}
                </pre>
              </details>
            )}
          </AlertDescription>
        </Alert>

        <div className="flex flex-wrap gap-2">
          {canPickDir && (
            <Button
              type="button"
              onClick={() => void handleRepairDir()}
              disabled={busy}
            >
              {busy ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <FolderOpen />
              )}
              {t("local.startup_error.choose_dir")}
            </Button>
          )}

          {canReconfigureLocal && (
            <Button
              type="button"
              variant="secondary"
              onClick={() => void handleReconfigureLocal()}
              disabled={busy}
            >
              {t("local.startup_error.reconfigure")}
            </Button>
          )}

          <Button
            type="button"
            variant="outline"
            onClick={() => void handleRetry()}
            disabled={busy}
          >
            <RotateCcw />
            {t("local.startup_error.retry")}
          </Button>

          <Button
            type="button"
            variant="destructive"
            className="ml-auto"
            onClick={() => void exitApp()}
          >
            <Power />
            {t("local.startup_error.exit")}
          </Button>
        </div>
      </div>
    </div>
  )
}
