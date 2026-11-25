import { useEffect, useState } from "react"
import { getCurrentWebview } from "@tauri-apps/api/webview"
import { Upload } from "lucide-react"
import { enqueueUploadPaths } from "@shiro/ui-shared/features/books/book-import-store"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

export function DesktopDropUploadOverlay({ enabled }: { enabled: boolean }) {
  const { t } = useTranslation()
  const [dragging, setDragging] = useState(false)

  useEffect(() => {
    if (!enabled) return

    let active = true
    let unlisten: (() => void) | undefined

    const clearDragging = () => {
      if (active) setDragging(false)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        clearDragging()
      }
    }

    window.addEventListener("blur", clearDragging)
    window.addEventListener("keydown", handleKeyDown)

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!active) return
        const payload = event.payload
        if (payload.type === "enter" || payload.type === "over") {
          setDragging(true)
        } else if (payload.type === "leave") {
          setDragging(false)
        } else if (payload.type === "drop") {
          setDragging(false)
          if (payload.paths.length > 0) {
            void enqueueUploadPaths(payload.paths)
          }
        }
      })
      .then((stop) => {
        if (active) {
          unlisten = stop
        } else {
          stop()
        }
      })
      .catch(() => {
        // Ignore in non-Tauri environments.
      })

    return () => {
      active = false
      setDragging(false)
      window.removeEventListener("blur", clearDragging)
      window.removeEventListener("keydown", handleKeyDown)
      unlisten?.()
    }
  }, [enabled])

  if (!enabled || !dragging) return null

  return (
    <div
      data-drop-upload-overlay=""
      className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-6 backdrop-blur-xs"
    >
      <div className="flex max-w-sm flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-primary bg-popover/95 px-8 py-6 text-center shadow-2xl">
        <div className="flex size-14 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-sm">
          <Upload aria-hidden="true" className="size-7 animate-bounce" />
        </div>
        <p className="text-base font-semibold text-foreground">
          {t("local.upload.drop.title")}
        </p>
        <p className="text-xs text-muted-foreground">
          {t("local.upload.drop.hint")}
        </p>
      </div>
    </div>
  )
}
