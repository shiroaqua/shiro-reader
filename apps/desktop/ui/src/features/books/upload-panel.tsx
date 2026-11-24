import { isTauriRuntime } from "@/api/tauri-bridge"
import { useOptionalAppRuntime } from "@/context/app-runtime-context"
import {
  setUploadPanelExpanded,
  useBookImportState,
} from "@/features/books/book-import-store"
import { UploadPanelBody } from "@/features/books/upload-panel-body"
import { UploadPanelMinimizedButton } from "@/features/books/upload-panel-fab"
import { isMobilePlatform } from "@/hooks/use-mobile"
import { useTranslation } from "@/lib/i18n"

export function UploadPanel() {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null
  const { upload, expanded, optionsOpen } = useBookImportState()

  const shouldRender =
    !isMobilePlatform() &&
    (!isTauri ||
      (bootstrap?.canImportBooks === true && bootstrap.phase === "ready")) &&
    upload.visible

  if (!shouldRender) return null

  return (
    <div data-upload-panel="" className="pointer-events-auto shrink-0">
      {!expanded ? (
        <UploadPanelMinimizedButton
          remaining={upload.stats.remaining}
          failed={upload.stats.failed}
          label={t("local.upload.panel.expand")}
          onExpand={() => setUploadPanelExpanded(true)}
        />
      ) : (
        <UploadPanelBody
          upload={upload}
          optionsOpen={optionsOpen}
          variant="floating"
          dismissLabel={t("local.upload.panel.minimize")}
          onDismiss={() => setUploadPanelExpanded(false)}
        />
      )}
    </div>
  )
}
