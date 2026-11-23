import { useId, useState } from "react"
import { Cloud, FolderOpen, HardDrive, LoaderCircle } from "lucide-react"
import type { LaunchMode } from "@/api/tauri-bridge"
import { BrandMark } from "@/components/brand-mark"
import { CloudOriginInput } from "@/components/cloud-origin-input"
import { Button } from "@/components/ui/button"
import { useAppRuntime } from "@/context/app-runtime-context"
import { useTranslation, type Locale } from "@/lib/i18n"
import { cn } from "cn"

export function SetupScreen() {
  const { t, locale } = useTranslation()
  const { bootstrap, changeLocale, completeSetup, pickLibraryDir } =
    useAppRuntime()
  const cloudInputId = useId()

  const [mode, setMode] = useState<LaunchMode>("local")
  const [customRoot, setCustomRoot] = useState<string>(
    bootstrap?.configuredLibraryRoot &&
      bootstrap.configuredLibraryRoot !== bootstrap.defaultLibraryRoot
      ? bootstrap.configuredLibraryRoot
      : ""
  )
  const [cloudOrigin, setCloudOrigin] = useState(bootstrap?.cloudOrigin ?? "")
  const [cloudOriginValid, setCloudOriginValid] = useState(false)
  const [submitting, setSubmitting] = useState(false)

  const canPickDir = bootstrap?.canPickLibraryDir ?? false
  const effectiveLocalPath =
    customRoot.trim() || bootstrap?.defaultLibraryRoot || ""

  const handleLocaleChange = async (nextLocale: Locale) => {
    try {
      await changeLocale(nextLocale)
    } catch {
      // Rust host already emits localized error notification.
    }
  }

  const handlePickDir = async () => {
    try {
      const picked = await pickLibraryDir()
      if (picked) setCustomRoot(picked)
    } catch {
      // Rust host already emits localized error notification.
    }
  }

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (submitting) return
    setSubmitting(true)
    try {
      await completeSetup({
        mode,
        locale,
        localLibraryRoot:
          canPickDir && customRoot.trim().length > 0 ? customRoot.trim() : null,
        cloudOrigin:
          mode === "cloud" && cloudOrigin.trim().length > 0
            ? cloudOrigin.trim()
            : null,
      })
    } catch {
      // Rust host emits localized shiro://error; keep user input intact.
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="relative flex min-h-dvh items-center justify-center overflow-hidden bg-background p-4">
      <div className="pointer-events-none absolute inset-0" aria-hidden="true">
        <div className="absolute -top-1/4 -right-1/4 size-[600px] rounded-full bg-primary/5 blur-3xl" />
        <div className="absolute -bottom-1/4 -left-1/4 size-[500px] rounded-full bg-primary/5 blur-3xl" />
      </div>

      <div className="relative z-10 w-full max-w-lg">
        <div className="mb-6 flex flex-col items-center text-center">
          <BrandMark className="mb-3 size-14 rounded-2xl [&_svg:not([class*='size-'])]:size-7" />
          <h1 className="text-2xl font-bold tracking-tight">
            {t("local.setup.title")}
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("local.setup.subtitle")}
          </p>
        </div>

        <form
          onSubmit={handleSubmit}
          className="space-y-5 rounded-2xl border bg-card p-6 shadow-xl shadow-black/[0.03] dark:shadow-black/20"
        >
          {/* Language */}
          <div className="space-y-2">
            <span className="block text-sm font-medium">
              {t("local.setup.language")}
            </span>
            <div className="grid grid-cols-2 gap-2">
              <Button
                type="button"
                variant={locale === "zh" ? "default" : "outline"}
                onClick={() => void handleLocaleChange("zh")}
                disabled={submitting}
              >
                简体中文
              </Button>
              <Button
                type="button"
                variant={locale === "en" ? "default" : "outline"}
                onClick={() => void handleLocaleChange("en")}
                disabled={submitting}
              >
                English
              </Button>
            </div>
          </div>

          {/* Launch mode */}
          <div className="space-y-2">
            <span className="block text-sm font-medium">
              {t("local.setup.mode")}
            </span>
            <div className="grid gap-2.5 sm:grid-cols-2">
              <button
                type="button"
                disabled={submitting}
                onClick={() => setMode("local")}
                className={cn(
                  "flex flex-col items-start gap-1.5 rounded-xl border p-3.5 text-left transition-colors",
                  mode === "local"
                    ? "border-primary bg-primary/5 ring-1 ring-primary"
                    : "border-border hover:bg-muted/50"
                )}
              >
                <div className="flex items-center gap-2 font-medium">
                  <HardDrive className="size-4" />
                  <span>{t("local.mode.local")}</span>
                </div>
                <span className="text-xs text-muted-foreground">
                  {t("local.mode.local_description")}
                </span>
              </button>

              <button
                type="button"
                disabled={submitting}
                onClick={() => setMode("cloud")}
                className={cn(
                  "flex flex-col items-start gap-1.5 rounded-xl border p-3.5 text-left transition-colors",
                  mode === "cloud"
                    ? "border-primary bg-primary/5 ring-1 ring-primary"
                    : "border-border hover:bg-muted/50"
                )}
              >
                <div className="flex items-center gap-2 font-medium">
                  <Cloud className="size-4" />
                  <span>{t("local.mode.cloud")}</span>
                </div>
                <span className="text-xs text-muted-foreground">
                  {t("local.mode.cloud_description")}
                </span>
              </button>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("local.setup.mode_fixed_hint")}
            </p>
          </div>

          {/* Mode-specific configuration */}
          {mode === "local" ? (
            <div className="space-y-2">
              <span className="block text-sm font-medium">
                {t("local.setup.library_dir")}
              </span>
              <div className="rounded-lg border bg-muted/40 px-3 py-2 font-mono text-xs break-all text-foreground">
                {effectiveLocalPath}
              </div>
              {canPickDir ? (
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => void handlePickDir()}
                    disabled={submitting}
                  >
                    <FolderOpen />
                    {t("local.setup.library_dir_pick")}
                  </Button>
                  {customRoot.trim().length > 0 && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => setCustomRoot("")}
                      disabled={submitting}
                    >
                      {t("local.setup.library_dir_default")}
                    </Button>
                  )}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">
                  {t("local.setup.android_private_hint")}
                </p>
              )}
            </div>
          ) : (
            <div className="space-y-1.5">
              <label
                htmlFor={cloudInputId}
                className="block text-sm font-medium"
              >
                {t("local.setup.cloud_origin")}
              </label>
              <CloudOriginInput
                id={cloudInputId}
                placeholder={t("local.setup.cloud_origin_placeholder")}
                value={cloudOrigin}
                onChange={setCloudOrigin}
                onValidityChange={setCloudOriginValid}
                disabled={submitting}
                className="h-9"
              />
            </div>
          )}

          <Button
            type="submit"
            disabled={
              submitting ||
              (mode === "cloud" && (!cloudOrigin.trim() || !cloudOriginValid))
            }
            aria-busy={submitting}
            className="h-10 w-full text-sm font-semibold"
          >
            {submitting && <LoaderCircle className="animate-spin" />}
            {submitting ? t("local.setup.submitting") : t("local.setup.submit")}
          </Button>
        </form>
      </div>
    </div>
  )
}
