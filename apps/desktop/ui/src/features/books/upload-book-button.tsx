import { LoaderCircle, Upload } from "lucide-react"
import { isTauriRuntime } from "@/api/tauri-bridge"
import { Button } from "@/components/ui/button"
import { useOptionalAppRuntime } from "@/context/app-runtime-context"
import {
  pickAndEnqueueBooks,
  useBookImportState,
} from "@/features/books/book-import-store"
import { useTranslation } from "@/lib/i18n"

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
