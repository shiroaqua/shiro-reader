import { useId, useState } from "react"
import {
  AlertCircle,
  Cloud,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  Power,
} from "lucide-react"
import type { LaunchMode } from "@/api/tauri-bridge"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { CloudOriginInput } from "@/components/cloud-origin-input"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useAppRuntime } from "@/context/app-runtime-context"
import { useTranslation, type Locale } from "@/lib/i18n"

export function SettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { bootstrap } = useAppRuntime()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-lg">
        {open && bootstrap && (
          <SettingsForm
            key={`${bootstrap.configuredMode ?? ""}-${bootstrap.configuredLibraryRoot}-${bootstrap.cloudOrigin ?? ""}`}
            onClose={() => onOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function SettingsForm({ onClose }: { onClose: () => void }) {
  const { t, locale } = useTranslation()
  const {
    bootstrap,
    changeLocale,
    updateSettings,
    pickLibraryDir,
    exitApp,
  } = useAppRuntime()
  const cloudInputId = useId()

  const [nextMode, setNextMode] = useState<LaunchMode>(
    bootstrap?.configuredMode ?? "local"
  )
  const [libraryRootDraft, setLibraryRootDraft] = useState(
    bootstrap?.configuredLibraryRoot ?? ""
  )
  const [cloudOriginDraft, setCloudOriginDraft] = useState(
    bootstrap?.cloudOrigin ?? ""
  )
  const [cloudOriginValid, setCloudOriginValid] = useState(false)
  const [saving, setSaving] = useState(false)

  if (!bootstrap) return null

  const canPickDir = bootstrap.canPickLibraryDir

  const isDirty =
    nextMode !== (bootstrap.configuredMode ?? "local") ||
    (nextMode === "local" &&
      canPickDir &&
      libraryRootDraft.trim() !== bootstrap.configuredLibraryRoot) ||
    (nextMode === "cloud" &&
      cloudOriginDraft.trim() !== (bootstrap.cloudOrigin ?? ""))

  const handleLocale = async (nextLocale: Locale) => {
    try {
      await changeLocale(nextLocale)
    } catch {
      // Host emits localized error event.
    }
  }

  const handlePickDir = async () => {
    try {
      const picked = await pickLibraryDir()
      if (picked) setLibraryRootDraft(picked)
    } catch {
      // Host emits localized error event.
    }
  }

  const handleSave = async () => {
    if (saving) return
    setSaving(true)
    try {
      await updateSettings({
        mode: nextMode,
        locale: null,
        localLibraryRoot:
          nextMode === "local" && canPickDir
            ? libraryRootDraft.trim() === bootstrap.defaultLibraryRoot
              ? ""
              : libraryRootDraft.trim()
            : null,
        cloudOrigin: nextMode === "cloud" ? cloudOriginDraft.trim() : null,
      })
    } catch {
      // Host emits localized error event; preserve user drafts.
    } finally {
      setSaving(false)
    }
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("local.settings.title")}</DialogTitle>
      </DialogHeader>

      <div className="space-y-4 py-1">
        {bootstrap.pendingRestart && (
          <Alert>
            <AlertCircle />
            <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
              <span>{t("local.settings.pending_restart")}</span>
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() => void exitApp()}
              >
                <Power />
                {t("local.settings.exit_to_restart")}
              </Button>
            </AlertDescription>
          </Alert>
        )}

        {/* Language */}
        <div className="space-y-1.5">
          <span className="block text-sm font-medium">
            {t("local.settings.language")}
          </span>
          <div className="grid grid-cols-2 gap-2">
            <Button
              type="button"
              size="sm"
              variant={locale === "zh" ? "default" : "outline"}
              onClick={() => void handleLocale("zh")}
            >
              简体中文
            </Button>
            <Button
              type="button"
              size="sm"
              variant={locale === "en" ? "default" : "outline"}
              onClick={() => void handleLocale("en")}
            >
              English
            </Button>
          </div>
        </div>

        {/* Mode */}
        <div className="space-y-1.5 border-t pt-3">
          <span className="block text-sm font-medium">
            {t("local.settings.mode")}
          </span>
          <div className="grid grid-cols-2 gap-2">
            <Button
              type="button"
              size="sm"
              variant={nextMode === "local" ? "default" : "outline"}
              onClick={() => setNextMode("local")}
              disabled={saving}
            >
              <HardDrive />
              {t("local.mode.local")}
            </Button>
            <Button
              type="button"
              size="sm"
              variant={nextMode === "cloud" ? "default" : "outline"}
              onClick={() => setNextMode("cloud")}
              disabled={saving}
            >
              <Cloud />
              {t("local.mode.cloud")}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("local.settings.mode_restart_hint")}
          </p>
        </div>

        {/* Mode-specific settings: desktop local directory OR cloud URL, never both */}
        {nextMode === "local" ? (
          canPickDir && (
            <div className="space-y-2 border-t pt-3">
              <span className="block text-sm font-medium">
                {t("local.settings.library_dir")}
              </span>
              <div className="rounded-md border bg-muted/40 px-2.5 py-1.5 text-xs break-all">
                {libraryRootDraft || bootstrap.defaultLibraryRoot}
              </div>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  onClick={() => void handlePickDir()}
                  disabled={saving}
                >
                  <FolderOpen />
                  {t("local.settings.pick_dir")}
                </Button>
                {libraryRootDraft !== bootstrap.defaultLibraryRoot && (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={() =>
                      setLibraryRootDraft(bootstrap.defaultLibraryRoot)
                    }
                    disabled={saving}
                  >
                    {t("local.settings.reset_dir")}
                  </Button>
                )}
              </div>
            </div>
          )
        ) : (
          <div className="space-y-1.5 border-t pt-3">
            <label htmlFor={cloudInputId} className="block text-sm font-medium">
              {t("local.settings.cloud_origin")}
            </label>
            <CloudOriginInput
              id={cloudInputId}
              placeholder={t("local.setup.cloud_origin_placeholder")}
              value={cloudOriginDraft}
              onChange={setCloudOriginDraft}
              onValidityChange={setCloudOriginValid}
              disabled={saving}
            />
          </div>
        )}
      </div>

      <DialogFooter>
        <DialogClose
          render={
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={saving}
            />
          }
        >
          {t("local.common.close")}
        </DialogClose>
        <Button
          type="button"
          onClick={() => void handleSave()}
          disabled={
            saving ||
            !isDirty ||
            (nextMode === "cloud" &&
              (!cloudOriginDraft.trim() || !cloudOriginValid))
          }
        >
          {saving && <LoaderCircle className="animate-spin" />}
          {saving ? t("local.settings.saving") : t("local.settings.save")}
        </Button>
      </DialogFooter>
    </>
  )
}
