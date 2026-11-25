import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import {
  setUploadPanelExpanded,
  useBookImportState,
} from "@shiro/ui-shared/features/books/book-import-store"
import { UploadPanelBody } from "@shiro/ui-shared/features/books/upload-panel-body"
import { UploadPanelMinimizedButton } from "./upload-panel-fab"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

export function UploadPanel() {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null
  const { upload, expanded, optionsOpen } = useBookImportState()

  const shouldRender =
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
