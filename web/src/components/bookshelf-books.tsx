import { useRef, useState, type ChangeEvent } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { BookOpen, LibraryBig, LoaderCircle, Upload } from "lucide-react"
import { ApiError, library } from "@/api/library"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { notifyError } from "@/lib/error-notifications"
import { formatBookDate } from "@/lib/format"
import { booksQueryKey } from "@/lib/query-keys"
import { useTranslation } from "@/lib/i18n"
import type { Book } from "@/lib/types"
import { hashFile } from "@/lib/hash"

export function BookshelfBooks() {
  const { t } = useTranslation()
  const { data: books, isPending, isError } = useQuery<Book[]>({
    queryKey: booksQueryKey,
    queryFn: () => library.bookshelf.getBooks(),
  })

  return (
    <section aria-labelledby="bookshelf-books-title" className="flex flex-1 flex-col gap-4 p-4">
      <div className="flex items-center justify-end gap-2">
        <UploadBookButton />
      </div>
      {isPending ? (
        <div aria-busy="true" aria-label={t("book.loading")} className={GRID_CLASS_NAME}>
          {Array.from({ length: 6 }, (_, index) => (
            <div key={index} className="flex flex-col gap-2">
              <Skeleton className="aspect-[3/4] w-full" />
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-3 w-1/2" />
            </div>
          ))}
        </div>
      ) : isError ? (
        <p role="status" className="text-sm text-muted-foreground">
          {t("book.load_failed")}
        </p>
      ) : books.length === 0 ? (
        <div
          role="status"
          className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground"
        >
          <LibraryBig aria-hidden="true" className="size-12" />
          <p className="text-sm">{t("book.empty")}</p>
        </div>
      ) : (
        <ul aria-label={t("book.list")} className={GRID_CLASS_NAME}>
          {books.map((book) => (
            <BookCard key={book.id} book={book} />
          ))}
        </ul>
      )}
    </section>
  )
}

// `accept` only filters the picker, so the extension is checked again on selection.
const BOOK_FILE_ACCEPT = ".pdf,.epub,application/pdf,application/epub+zip"
const BOOK_FILE_PATTERN = /\.(pdf|epub)$/i

function UploadBookButton() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const inputRef = useRef<HTMLInputElement>(null)
  const uploadMutation = useMutation({
    mutationFn: async (files: File[]) => {
      // Upload one at a time so a failure only affects that file.
      for (const file of files) {
        try {
          const hash = await hashFile(file);
          if (!(await library.files.book.exists(hash)))
            await library.files.book.upload(hash, file);

          await library.bookshelf.createBook(
            file.name.replace(BOOK_FILE_PATTERN, "").trim(),
            hash
          )
         } catch (error) {
          if (!(error instanceof ApiError)) notifyError("common.error.unknown")
        }
      }
    },
    onSettled: () =>
      queryClient.invalidateQueries({ queryKey: booksQueryKey }),
  })

  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? [])
    // Reset so choosing the same file again still fires a change event.
    event.target.value = ""
    const bookFiles = files.filter((file) => BOOK_FILE_PATTERN.test(file.name))
    if (bookFiles.length < files.length) notifyError("library.book.file.unsupported")
    if (bookFiles.length > 0) uploadMutation.mutate(bookFiles)
  }

  return (
    <>
      <input
        ref={inputRef}
        type="file"
        accept={BOOK_FILE_ACCEPT}
        multiple
        hidden
        data-testid="book-upload-input"
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
        {uploadMutation.isPending ? t("book.uploading") : t("book.upload")}
      </Button>
    </>
  )
}

const GRID_CLASS_NAME =
  "grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-x-4 gap-y-6"

function BookCard({ book }: { book: Book }) {
  const [coverFailed, setCoverFailed] = useState(false)

  return (
    <li className="flex min-w-0 flex-col gap-1">
      <div className="aspect-[3/4] w-full overflow-hidden rounded-md border bg-muted">
        {coverFailed ? (
          <div className="flex size-full items-center justify-center text-muted-foreground">
            <BookOpen aria-hidden="true" className="size-8" />
          </div>
        ) : (
          <img
            src={library.bookshelf.coverUrl(book.id)}
            alt=""
            loading="lazy"
            className="size-full object-cover"
            onError={() => setCoverFailed(true)}
          />
        )}
      </div>
      <h2 title={book.title} className="mt-1 line-clamp-2 text-center text-sm font-medium break-all">
        {book.title}
      </h2>
      <time dateTime={book.updated_at} className="text-center text-xs text-muted-foreground">
        {formatBookDate(book.updated_at)}
      </time>
    </li>
  )
}
