import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import {
  Check,
  Pencil,
  Tag,
  Trash2,
} from "lucide-react"
import { library } from "@/api/library"
import { CommandError } from "@/api/tauri-bridge"
import { ActionSheet } from "@/components/action-sheet"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { Input } from "@/components/ui/input"
import { BookCover } from "@/features/books/book-cover"
import { useLongPress } from "@/hooks/use-long-press"
import { isMobilePlatform } from "@/hooks/use-mobile"
import { useIsTouch } from "@/hooks/use-touch"
import { BookTagDialog } from "@/features/tags/book-tag-dialog"
import { formatBookDate } from "@/lib/format"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey, trashQueryKey } from "@/lib/query-keys"
import type { Book } from "@/lib/types"
import { cn } from "cn"

export type SelectionPreview = "select" | "deselect" | null

export function BookCard({
  book,
  selected,
  isSelectionMode,
  selectionPreview = null,
  onToggleSelect,
  onHoverChange,
}: {
  book: Book
  selected: boolean
  isSelectionMode: boolean
  selectionPreview?: SelectionPreview
  onToggleSelect: (shiftKey: boolean) => void
  onHoverChange?: ((hovering: boolean) => void) | undefined
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [renaming, setRenaming] = useState(false)
  const [tagsOpen, setTagsOpen] = useState(false)
  const [actionsOpen, setActionsOpen] = useState(false)
  const isTouch = useIsTouch()
  const isTouchMenu = isTouch || isMobilePlatform()
  const [prevSelectionMode, setPrevSelectionMode] = useState(isSelectionMode)
  if (isSelectionMode !== prevSelectionMode) {
    setPrevSelectionMode(isSelectionMode)
    if (isSelectionMode) {
      setRenaming(false)
      setTagsOpen(false)
      setActionsOpen(false)
    }
  }
  const isRenaming = renaming && !isSelectionMode
  // Touch / mobile: long press opens the book action menu when not in selection mode.
  const longPress = useLongPress(() => {
    if (!isSelectionMode && !isRenaming) setActionsOpen(true)
  }, isTouchMenu)
  const willSelect = !selected && selectionPreview === "select"
  const willDeselect = selected && selectionPreview === "deselect"

  const updateBooks = (update: (books: Book[]) => Book[]) =>
    queryClient.setQueryData<Book[]>(
      booksQueryKey,
      (books) => books && update(books)
    )

  const handleMutationError = (error: Error) => {
    if (error instanceof CommandError && error.payload.key === "library.book.not_found") {
      setRenaming(false)
      updateBooks((books) => books.filter((item) => item.id !== book.id))
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
      updateBooks((books) => books.filter((item) => item.id !== book.id))
    },
    onError: handleMutationError,
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
        queryClient.invalidateQueries({ queryKey: trashQueryKey }),
      ]),
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
    <>
      <ContextMenu disabled={isRenaming || isSelectionMode || isTouchMenu}>
      <ContextMenuTrigger
        render={<li />}
        data-selected={selected || undefined}
        onMouseEnter={() => onHoverChange?.(true)}
        onMouseLeave={() => onHoverChange?.(false)}
        onMouseDown={(event) => {
          if (isSelectionMode && event.shiftKey) event.preventDefault()
        }}
        onContextMenu={(event) => {
          if (isTouchMenu) event.preventDefault()
        }}
        {...longPress.handlers}
        onClick={(event) => {
          if (longPress.consumeClick()) return
          if (!isSelectionMode || isRenaming) return
          onToggleSelect(event.shiftKey)
        }}
        className={cn(
          "group/card flex min-w-0 flex-col gap-1",
          isSelectionMode && "cursor-pointer select-none",
          isTouchMenu && "[-webkit-touch-callout:none] select-none"
        )}
      >
        <div
          className={cn(
            "relative rounded-xl",
            selected && !willDeselect
              ? "ring-2 ring-primary ring-offset-2 ring-offset-background"
              : willDeselect
                ? "ring-2 ring-primary/30 ring-offset-2 ring-offset-background"
                : willSelect
                  ? "ring-2 ring-primary/45 ring-offset-2 ring-offset-background"
                  : undefined
          )}
        >
          <BookCover
            bookId={book.id}
            className={cn(
              "aspect-[5/7] w-full rounded-xl shadow-sm transition-shadow duration-200",
              !selected &&
                !willSelect &&
                !willDeselect &&
                "group-hover/card:shadow-lg group-hover/card:shadow-black/[0.08]"
            )}
          />
          {!isRenaming && (
            <>
              <button
                type="button"
                aria-pressed={selected}
                aria-label={t("local.book.select_item").replace(
                  "{title}",
                  book.title
                )}
                onPointerDown={(event) => {
                  event.stopPropagation()
                }}
                onMouseDown={(event) => {
                  if (event.shiftKey) event.preventDefault()
                }}
                onClick={(event) => {
                  event.stopPropagation()
                  if (longPress.consumeClick()) return
                  if (event.detail > 0) event.currentTarget.blur()
                  onToggleSelect(event.shiftKey)
                }}
                className={cn(
                  "group/check peer/check absolute top-2 left-2 z-10 flex size-6 pointer-coarse:size-7 cursor-pointer items-center justify-center rounded-full transition-all duration-150 outline-none before:absolute before:-inset-1.5 before:content-[''] focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1",
                  selected && !willDeselect
                    ? "bg-primary text-primary-foreground opacity-100 shadow-sm ring-2 ring-background/80"
                    : willDeselect
                      ? "bg-primary/50 text-primary-foreground opacity-100 shadow-sm ring-2 ring-background/60"
                      : willSelect
                        ? "border-2 border-primary bg-primary/20 text-primary opacity-100 shadow-xs backdrop-blur-[2px]"
                        : isSelectionMode
                          ? "border-2 border-white/90 bg-black/25 text-white opacity-85 shadow-xs backdrop-blur-[2px] hover:border-white hover:bg-black/45 hover:opacity-100"
                          : "border-2 border-white/85 bg-black/25 text-white opacity-0 shadow-xs backdrop-blur-[2px] group-hover/card:opacity-90 hover:border-white hover:bg-black/45 hover:opacity-100"
                )}
              >
                <Check
                  aria-hidden="true"
                  className={cn(
                    "size-3.5 stroke-[2.75] transition-all duration-150",
                    selected && !willDeselect
                      ? "scale-100 opacity-100"
                      : willDeselect
                        ? "scale-90 opacity-80"
                        : willSelect
                          ? "scale-95 opacity-90"
                          : "scale-90 opacity-0 group-hover/check:scale-100 group-hover/check:opacity-100"
                  )}
                />
              </button>
              <div
                aria-hidden="true"
                className={cn(
                  "pointer-events-none absolute inset-x-0 top-0 h-14 rounded-t-xl bg-gradient-to-b from-black/45 via-black/15 to-transparent transition-opacity duration-150",
                  selected
                    ? "opacity-0"
                    : isSelectionMode
                      ? "opacity-70 group-hover/card:opacity-100"
                      : "opacity-0 peer-focus-visible/check:opacity-100 group-hover/card:opacity-100"
                )}
              />
            </>
          )}
        </div>
        {isRenaming ? (
          <Input
            autoFocus
            defaultValue={book.title}
            aria-label={t("local.book.title")}
            disabled={renameMutation.isPending}
            enterKeyHint="done"
            className="mt-1.5 h-7 text-center"
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
            className="mt-1.5 line-clamp-2 text-center text-sm leading-snug font-medium wrap-break-word"
          >
            {book.title}
          </h2>
        )}
        <time
          dateTime={book.updated_at}
          className="text-center text-xs text-muted-foreground tabular-nums"
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
          disabled={deleteMutation.isPending}
          onClick={() => deleteMutation.mutate()}
        >
          <Trash2 aria-hidden="true" />
          {t("local.book.delete")}
        </ContextMenuItem>
      </ContextMenuContent>
      </ContextMenu>
      <ActionSheet
        open={actionsOpen && isTouchMenu && !isSelectionMode}
        onOpenChange={setActionsOpen}
        title={book.title}
        items={[
          {
            key: "rename",
            label: t("local.book.rename"),
            icon: <Pencil aria-hidden="true" />,
            onSelect: () => setRenaming(true),
          },
          {
            key: "tags",
            label: t("local.book.tags"),
            icon: <Tag aria-hidden="true" />,
            onSelect: () => setTagsOpen(true),
          },
          {
            key: "delete",
            label: t("local.book.delete"),
            icon: <Trash2 aria-hidden="true" />,
            destructive: true,
            disabled: deleteMutation.isPending,
            onSelect: () => deleteMutation.mutate(),
          },
        ]}
      />
      <BookTagDialog
        book={book}
        open={tagsOpen && !isSelectionMode}
        onOpenChange={setTagsOpen}
      />
    </>
  )
}
