import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import {
  closeMobileUploadPanel,
  useBookImportState,
} from "@shiro/ui-shared/features/books/book-import-store"
import { UploadPanelBody } from "@shiro/ui-shared/features/books/upload-panel-body"
import { useBackHandler } from "@shiro/ui-shared/hooks/use-back-handler"
import { BACK_PRIORITY } from "@shiro/ui-shared/lib/back-navigation"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

function isWithinErrorAlerts(target: EventTarget | null): boolean {
  return (
    target instanceof Element && target.closest("[data-error-alerts]") !== null
  )
}

export function MobileUploadScreen() {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null
  const { upload, mobileOpen, optionsOpen } = useBookImportState()

  const shouldRender =
    (!isTauri ||
      (bootstrap?.canImportBooks === true && bootstrap.phase === "ready")) &&
    mobileOpen &&
    upload.visible

  useBackHandler(shouldRender, BACK_PRIORITY.uploadOverlay, () => {
    closeMobileUploadPanel()
  })

  return (
    <DialogPrimitive.Root
      open={shouldRender}
      onOpenChange={(open, eventDetails) => {
        if (open) return
        if (
          eventDetails.reason === "outside-press" ||
          eventDetails.reason === "focus-out"
        ) {
          const event = eventDetails.event
          const relatedTarget =
            "relatedTarget" in event
              ? (event.relatedTarget as EventTarget | null)
              : null
          if (
            isWithinErrorAlerts(event.target) ||
            isWithinErrorAlerts(relatedTarget)
          ) {
            eventDetails.cancel()
            return
          }
        }
        closeMobileUploadPanel()
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Popup
          data-upload-panel=""
          aria-label={t("local.upload.panel.title")}
          className="fixed inset-0 z-[90] flex flex-col bg-background text-foreground outline-none"
        >
          <UploadPanelBody
            upload={upload}
            optionsOpen={optionsOpen}
            variant="fullscreen"
            dismissLabel={t("local.upload.panel.close")}
            onDismiss={closeMobileUploadPanel}
          />
        </DialogPrimitive.Popup>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
