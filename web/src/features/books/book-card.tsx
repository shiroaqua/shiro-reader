import { useEffect, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { BookOpen, LoaderCircle, Pencil, Tag, Trash2 } from "lucide-react"
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
import { Input } from "@/components/ui/input"
import { BookTagDialog } from "@/features/tags/book-tag-dialog"
import { formatBookDate } from "@/lib/format"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey } from "@/lib/query-keys"
import type { Book } from "@/lib/types"
import { ErrorCode } from "@/lib/error-codes"

export function BookCard({ book }: { book: Book }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [coverFailed, setCoverFailed] = useState(false)
  const [coverUrl, setCoverUrl] = useState<string>()
  useEffect(() => {
    let active = true
    let objectUrl: string | undefined
    void library.bookshelf.book
      .getCover(book.id)
      .then((blob) => {
        if (!active) return
        objectUrl = URL.createObjectURL(blob)
        setCoverUrl(objectUrl)
      })
      .catch(() => {
        if (active) setCoverFailed(true)
      })
    return () => {
      active = false
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [book.id])
  const [renaming, setRenaming] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [tagsOpen, setTagsOpen] = useState(false)

  const updateBooks = (update: (books: Book[]) => Book[]) =>
    queryClient.setQueryData<Book[]>(
      booksQueryKey,
      (books) => books && update(books)
    )

  const handleMutationError = (error: Error) => {
    if (!(error instanceof ApiError)) return
    if (error.code === ErrorCode.bookNotFound) {
      setRenaming(false)
      setDeleteOpen(false)
      updateBooks((books) => books.filter((item) => item.id !== book.id))
    } else if (error.code === ErrorCode.bookVersionConflict) {
      // Leave edit mode so the refetched title is visible before the user retries.
      setRenaming(false)
    }
  }

  const renameMutation = useMutation({
    mutationFn: (title: string) =>
      library.bookshelf.book.rename(book.id, title),
    onSuccess: (_, title) => {
      updateBooks((books) =>
        books.map((item) => (item.id === book.id ? { ...item, title } : item))
      )
      setRenaming(false)
    },
    onError: handleMutationError,
    onSettled: () => queryClient.invalidateQueries({ queryKey: booksQueryKey }),
  })

  const deleteMutation = useMutation({
    mutationFn: () => library.bookshelf.book.delete(book.id),
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
        className="group/card flex min-w-0 flex-col gap-1.5"
      >
        <div className="relative aspect-[3/4] w-full overflow-hidden rounded-xl border border-border/50 bg-muted shadow-sm transition-all duration-200 group-hover/card:-translate-y-0.5 group-hover/card:shadow-lg group-hover/card:shadow-black/[0.08] dark:group-hover/card:shadow-black/25">
          {coverFailed ? (
            <div className="flex size-full items-center justify-center bg-gradient-to-br from-muted to-muted-foreground/5 text-muted-foreground">
              <BookOpen aria-hidden="true" className="size-8 opacity-40" />
            </div>
          ) : (
            <img
              src={coverUrl}
              alt=""
              loading="lazy"
              className="size-full object-cover transition-transform duration-200 group-hover/card:scale-[1.02]"
              onError={() => setCoverFailed(true)}
            />
          )}
        </div>
        {renaming ? (
          <Input
            autoFocus
            defaultValue={book.title}
            aria-label={t("local.book.title")}
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
          <h2
            title={book.title}
            className="mt-1.5 line-clamp-2 text-center text-sm leading-snug font-medium break-all"
          >
            {book.title}
          </h2>
        )}
        <time
          dateTime={book.updated_at}
          className="text-center text-[11px] text-muted-foreground"
        >
          {formatBookDate(book.updated_at)}
        </time>
      </ContextMenuTrigger>
      <ContextMenuContent finalFocus={false}>
        <ContextMenuItem onClick={() => setRenaming(true)}>
          <Pencil aria-hidden="true" />
          {t("local.book.rename")}
        </ContextMenuItem>
        <ContextMenuItem onClick={() => setTagsOpen(true)}>
          <Tag aria-hidden="true" />
          {t("local.book.tags")}
        </ContextMenuItem>
        <ContextMenuItem
          variant="destructive"
          onClick={() => setDeleteOpen(true)}
        >
          <Trash2 aria-hidden="true" />
          {t("local.book.delete")}
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
            <AlertDialogTitle>
              {t("local.book.delete_confirm_title")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("local.book.delete_confirm_description").replace(
                "{title}",
                book.title
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose
              disabled={deleteMutation.isPending}
              render={<Button variant="outline" />}
            >
              {t("local.common.cancel")}
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
              {t("local.book.delete")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <BookTagDialog book={book} open={tagsOpen} onOpenChange={setTagsOpen} />
    </ContextMenu>
  )
}
