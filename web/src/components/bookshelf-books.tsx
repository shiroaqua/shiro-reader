import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  ArrowDownWideNarrow,
  ArrowUpNarrowWide,
  BookOpen,
  LibraryBig,
  LoaderCircle,
  Pencil,
  Trash2,
  Upload,
} from "lucide-react"
import { ApiError, library } from "@/api/library"
import {
  AlertDialog,
  AlertDialogClose,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { notifyError } from "@/lib/error-notifications"
import { formatBookDate } from "@/lib/format"
import { booksQueryKey } from "@/lib/query-keys"
import {
  loadBookSort,
  saveBookSort,
  sortBooks,
  type BookSort,
  type BookSortField,
  type BookSortOrder,
} from "@/lib/book-sort"
import { useTranslation } from "@/lib/i18n"
import type { Book } from "@/lib/types"
import { hashFile } from "@/lib/hash"

export function BookshelfBooks() {
  const { t, locale } = useTranslation()
  const { data: books, isPending, isError } = useQuery<Book[]>({
    queryKey: booksQueryKey,
    queryFn: () => library.bookshelf.getBooks(),
  })
  const [sort, setSort] = useState<BookSort>(loadBookSort)
  const sortedBooks = useMemo(
    () => (books ? sortBooks(books, sort, locale) : []),
    [books, sort, locale]
  )

  const changeSort = (next: BookSort) => {
    setSort(next)
    saveBookSort(next)
  }

  return (
    <section aria-labelledby="bookshelf-books-title" className="flex flex-1 flex-col gap-4 p-4">
      <div className="flex items-center justify-end gap-2">
        <SortBooksMenu sort={sort} onChange={changeSort} />
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
          {sortedBooks.map((book) => (
            <BookCard key={book.id} book={book} />
          ))}
        </ul>
      )}
    </section>
  )
}

function SortBooksMenu({
  sort,
  onChange,
}: {
  sort: BookSort
  onChange: (sort: BookSort) => void
}) {
  const { t } = useTranslation()
  const Icon = sort.order === "asc" ? ArrowUpNarrowWide : ArrowDownWideNarrow

  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button type="button" variant="outline" size="sm" />}>
        <Icon aria-hidden="true" />
        {t(sort.field === "title" ? "book.sort.title" : "book.sort.date")}
      </DropdownMenuTrigger>
      <DropdownMenuContent aria-label={t("book.sort")}>
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("book.sort.by")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.field}
            onValueChange={(field: BookSortField) => onChange({ ...sort, field })}
          >
            <DropdownMenuRadioItem value="title">{t("book.sort.title")}</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="date">{t("book.sort.date")}</DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("book.sort.order")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.order}
            onValueChange={(order: BookSortOrder) => onChange({ ...sort, order })}
          >
            <DropdownMenuRadioItem value="asc">{t("book.sort.asc")}</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="desc">{t("book.sort.desc")}</DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
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
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [coverFailed, setCoverFailed] = useState(false)
  const [coverUrl, setCoverUrl] = useState<string>()
  useEffect(() => {
    let active = true
    let objectUrl: string | undefined
    void library.bookshelf.getCover(book.id).then((blob) => {
      if (!active) return
      objectUrl = URL.createObjectURL(blob)
      setCoverUrl(objectUrl)
    }).catch(() => {
      if (active) setCoverFailed(true)
    })
    return () => {
      active = false
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [book.id])
  const [renaming, setRenaming] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)

  const updateBooks = (update: (books: Book[]) => Book[]) =>
    queryClient.setQueryData<Book[]>(booksQueryKey, (books) => books && update(books))

  const handleMutationError = (error: Error) => {
    if (!(error instanceof ApiError)) return
    if (error.message === "library.book.not_found") {
      setRenaming(false)
      setDeleteOpen(false)
      updateBooks((books) => books.filter((item) => item.id !== book.id))
    } else if (error.message === "library.book.version_conflict") {
      // Leave edit mode so the refetched title is visible before the user retries.
      setRenaming(false)
    }
  }

  const renameMutation = useMutation({
    mutationFn: (title: string) => library.bookshelf.renameBook(book.id, title),
    onSuccess: (_, title) => {
      updateBooks((books) => books.map((item) => (item.id === book.id ? { ...item, title } : item)))
      setRenaming(false)
    },
    onError: handleMutationError,
    onSettled: () => queryClient.invalidateQueries({ queryKey: booksQueryKey }),
  })

  const deleteMutation = useMutation({
    mutationFn: () => library.bookshelf.deleteBook(book.id),
    onSuccess: () => {
      setDeleteOpen(false)
      updateBooks((books) => books.filter((item) => item.id !== book.id))
    },
    onError: handleMutationError,
    onSettled: () => queryClient.invalidateQueries({ queryKey: booksQueryKey }),
  })

  const submitRename = (value: string) => {
    const title = value.trim()
    // Nothing to send for an empty or unchanged title; just leave edit mode.
    if (title === "" || title === book.title) {
      setRenaming(false)
      return
    }
    renameMutation.mutate(title)
  }

  return (
    <ContextMenu disabled={renaming}>
      <ContextMenuTrigger
        render={<li />}
        className="flex min-w-0 flex-col gap-1"
      >
      <div className="aspect-[3/4] w-full overflow-hidden rounded-md border bg-muted">
        {coverFailed ? (
          <div className="flex size-full items-center justify-center text-muted-foreground">
            <BookOpen aria-hidden="true" className="size-8" />
          </div>
        ) : (
          <img
            src={coverUrl}
            alt=""
            loading="lazy"
            className="size-full object-cover"
            onError={() => setCoverFailed(true)}
          />
        )}
      </div>
      {renaming ? (
        <Input
          autoFocus
          defaultValue={book.title}
          aria-label={t("book.title")}
          disabled={renameMutation.isPending}
          className="mt-1 h-7 text-center"
          onFocus={(event) => event.currentTarget.select()}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return
            if (event.key === "Enter") {
              event.preventDefault()
              submitRename(event.currentTarget.value)
            } else if (event.key === "Escape") {
              event.preventDefault()
              setRenaming(false)
            }
          }}
          onBlur={() => {
            if (!renameMutation.isPending) setRenaming(false)
          }}
        />
      ) : (
        <h2 title={book.title} className="mt-1 line-clamp-2 text-center text-sm font-medium break-all">
          {book.title}
        </h2>
      )}
      <time dateTime={book.updated_at} className="text-center text-xs text-muted-foreground">
        {formatBookDate(book.updated_at)}
      </time>
      </ContextMenuTrigger>
      <ContextMenuContent finalFocus={false}>
        <ContextMenuItem onClick={() => setRenaming(true)}>
          <Pencil aria-hidden="true" />
          {t("book.rename")}
        </ContextMenuItem>
        <ContextMenuItem variant="destructive" onClick={() => setDeleteOpen(true)}>
          <Trash2 aria-hidden="true" />
          {t("book.delete")}
        </ContextMenuItem>
      </ContextMenuContent>
      <AlertDialog
        open={deleteOpen}
        onOpenChange={(open) => {
          if (!deleteMutation.isPending) setDeleteOpen(open)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("book.delete_confirm_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("book.delete_confirm_description").replace("{title}", book.title)}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose
              disabled={deleteMutation.isPending}
              render={<Button variant="outline" />}
            >
              {t("common.cancel")}
            </AlertDialogClose>
            <Button
              variant="destructive"
              disabled={deleteMutation.isPending}
              aria-busy={deleteMutation.isPending}
              onClick={() => deleteMutation.mutate()}
            >
              {deleteMutation.isPending && (
                <LoaderCircle aria-hidden="true" className="animate-spin" />
              )}
              {t("book.delete")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </ContextMenu>
  )
}
