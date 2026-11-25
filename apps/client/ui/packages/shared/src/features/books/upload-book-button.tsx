import { LoaderCircle, Upload } from "lucide-react"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import {
  pickAndEnqueueBooks,
  useBookImportState,
} from "@shiro/ui-shared/features/books/book-import-store"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

export function UploadBookButton() {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const isTauri = runtime ? runtime.isTauri : isTauriRuntime()
  const bootstrap = runtime?.bootstrap ?? null
  const { pickerOpening } = useBookImportState()

  if (isTauri && bootstrap?.canImportBooks === false) {
    return null
  }

  return (
    <Button
      type="button"
      size="sm"
      disabled={pickerOpening}
      aria-busy={pickerOpening}
      onClick={() => void pickAndEnqueueBooks()}
    >
      {pickerOpening ? (
        <LoaderCircle aria-hidden="true" className="animate-spin" />
      ) : (
        <Upload aria-hidden="true" />
      )}
      {t("local.book.upload")}
    </Button>
  )
}
