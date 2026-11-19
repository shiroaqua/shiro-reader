import { useRef, useState, type ChangeEvent } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle, Upload } from "lucide-react"
import { ApiError, library } from "@/api/library"
import { Button } from "@/components/ui/button"
import { notifyError } from "@/lib/error-notifications"
import { hashFile } from "@/lib/hash"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey } from "@/lib/query-keys"

// `accept` only filters the picker, so the extension is checked again on selection.
const BOOK_FILE_ACCEPT = ".pdf,.epub,application/pdf,application/epub+zip"
const BOOK_FILE_PATTERN = /\.(pdf|epub)$/i

export function UploadBookButton() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const inputRef = useRef<HTMLInputElement>(null)
  const [progress, setProgress] = useState<{
    current: number
    total: number
  } | null>(null)
  const uploadMutation = useMutation({
    mutationFn: async (files: File[]) => {
      let uploadedAny = false
      // Upload one at a time so a failure only affects that file.
      try {
        for (let index = 0; index < files.length; index += 1) {
          const file = files[index]
          setProgress({ current: index + 1, total: files.length })
          const title = file.name.replace(BOOK_FILE_PATTERN, "").trim()
          if (title === "") {
            notifyError({
              source: "server",
              key: "library.book.missing_title",
            })
            continue
          }
          try {
            const hash = await hashFile(file)
            if (!(await library.files.book.exists(hash)))
              await library.files.book.upload(hash, file)

            await library.bookshelf.book.create(title, hash)
            uploadedAny = true
            void queryClient.invalidateQueries({ queryKey: booksQueryKey })
          } catch (error) {
            if (!(error instanceof ApiError)) {
              notifyError({
                source: "local",
                key: "local.common.error.unknown",
              })
            } else if (error.httpStatus === 401 || error.httpStatus === 0) {
              // Session expired or network unavailable: abort remaining uploads.
              break
            }
          }
        }
      } finally {
        setProgress(null)
        if (uploadedAny) {
          await queryClient.invalidateQueries({ queryKey: booksQueryKey })
        }
      }
    },
  })

  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? [])
    // Reset so choosing the same file again still fires a change event.
    event.target.value = ""
    const bookFiles = files.filter((file) => BOOK_FILE_PATTERN.test(file.name))
    if (bookFiles.length < files.length)
      notifyError({
        source: "local",
        key: "local.library.book.file.unsupported",
      })
    if (bookFiles.length > 0) uploadMutation.mutate(bookFiles)
  }

  const label = !uploadMutation.isPending
    ? t("local.book.upload")
    : progress && progress.total > 1
      ? t("local.book.uploading_progress")
          .replace("{current}", String(progress.current))
          .replace("{total}", String(progress.total))
      : t("local.book.uploading")

  return (
    <>
      <input
        ref={inputRef}
        type="file"
        accept={BOOK_FILE_ACCEPT}
        multiple
        hidden
        onChange={handleChange}
      />
      <Button
        type="button"
        size="sm"
        disabled={uploadMutation.isPending}
        aria-busy={uploadMutation.isPending}
        onClick={() => inputRef.current?.click()}
      >
        {uploadMutation.isPending ? (
          <LoaderCircle aria-hidden="true" className="animate-spin" />
        ) : (
          <Upload aria-hidden="true" />
        )}
        {label}
      </Button>
    </>
  )
}
