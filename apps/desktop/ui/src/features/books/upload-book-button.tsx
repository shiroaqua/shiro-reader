import { useQueryClient } from "@tanstack/react-query"
import { LoaderCircle, Upload, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  cancelActiveBookImport,
  startBookImport,
  useBookImportState,
} from "@/features/books/book-import-store"
import { useTranslation } from "@/lib/i18n"
import { cn } from "cn"

/**
 * Starts a book import. `floating` renders a phone-style floating action
 * button above the bottom navigation; progress and results stack above it.
 */
export function UploadBookButton({ floating = false }: { floating?: boolean }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const { pending, progress, summary, canCancel, cancelling } =
    useBookImportState()

  const label = !pending
    ? t("local.book.upload")
    : progress && progress.total > 1
      ? t("local.book.uploading_progress")
          .replace("{current}", String(progress.index + 1))
          .replace("{total}", String(progress.total))
      : t("local.book.uploading")
  const percent = progress?.totalBytes
    ? Math.min(100, Math.round((progress.bytesRead * 100) / progress.totalBytes))
    : null

  return (
    <div
      className={cn(
        "flex min-w-0 items-center gap-1.5",
        floating
          ? "fixed right-4 bottom-[calc(4.5rem+env(safe-area-inset-bottom))] z-30 max-w-[calc(100vw-2rem)] flex-col-reverse items-end"
          : "flex-wrap"
      )}
    >
      <Button
        type="button"
        size={floating ? "icon-lg" : "sm"}
        disabled={pending}
        aria-busy={pending}
        aria-label={floating ? label : undefined}
        title={floating ? label : undefined}
        className={
          floating
            ? "size-14 rounded-full shadow-lg pointer-coarse:size-14 [&_svg:not([class*='size-'])]:size-6"
            : undefined
        }
        onClick={() => void startBookImport(queryClient)}
      >
        {pending ? (
          <LoaderCircle aria-hidden="true" className="animate-spin" />
        ) : (
          <Upload aria-hidden="true" />
        )}
        {!floating && label}
      </Button>
      {pending && canCancel && (
        <Button
          type="button"
          size="icon-sm"
          variant="outline"
          disabled={cancelling}
          aria-label={t("local.book.import_cancel")}
          title={t("local.book.import_cancel")}
          onClick={() => void cancelActiveBookImport()}
        >
          <X />
        </Button>
      )}
      {pending && progress && (
        <span
          className={cn(
            "max-w-32 truncate text-xs text-muted-foreground",
            floating &&
              "max-w-48 rounded-md bg-popover px-2 py-1 shadow-md ring-1 ring-foreground/10"
          )}
          title={progress.fileName}
        >
          {progress.fileName}
          {percent !== null && ` · ${percent}%`}
        </span>
      )}
      {!pending && summary?.summaryMessage && (
        <details
          className={cn(
            "max-w-full rounded-md border bg-muted/60 px-2 py-1 text-xs text-muted-foreground",
            floating && "bg-popover shadow-md"
          )}
        >
          <summary className="cursor-pointer" role="status">
            {summary.summaryMessage}
          </summary>
          <ul className="max-h-28 list-disc overflow-y-auto pl-4">
            {summary.failed.map(({ fileName, error }) => (
              <li key={error.errorId} className="wrap-anywhere">
                {t("local.book.import_failed_item")} {fileName}
              </li>
            ))}
            {summary.skipped.map((name, index) => (
              <li key={index} className="wrap-anywhere">
                {t("local.book.import_skipped_item")} {name}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )
}
