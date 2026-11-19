import { useEffect, useMemo, useState } from "react"
import { createPortal } from "react-dom"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  CheckCheck,
  CircleAlert,
  LibraryBig,
  LoaderCircle,
  RotateCcw,
  Tag,
  Trash2,
  X,
} from "lucide-react"
import { library } from "@/api/library"
import { EmptyState } from "@/components/empty-state"
import { PageSection, PageToolbar } from "@/components/page-layout"
import { SortMenu } from "@/components/sort-menu"
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
import { Skeleton } from "@/components/ui/skeleton"
import { BookCard } from "@/features/books/book-card"
import { UploadBookButton } from "@/features/books/upload-book-button"
import { BatchBookTagDialog } from "@/features/tags/book-tag-dialog"
import { booksQueryKey, trashQueryKey } from "@/lib/query-keys"
import {
  loadBookSort,
  saveBookSort,
  sortBooks,
  type BookSort,
} from "@/lib/book-sort"
import { formatCountMessage, useTranslation } from "@/lib/i18n"
import type { Book } from "@/lib/types"

type SelectionAnchor = {
  id: string
  action: "select" | "deselect"
}

export function BookshelfBooks({
  headerOverlay = null,
}: {
  headerOverlay?: HTMLElement | null
}) {
  const { t, locale } = useTranslation()
  const queryClient = useQueryClient()
  const {
    data: books,
    isPending,
    isError,
    isFetching,
    refetch,
  } = useQuery<Book[]>({
    queryKey: booksQueryKey,
    queryFn: () => library.bookshelf.book.list(),
  })
  const [sort, setSort] = useState<BookSort>(loadBookSort)
  const sortedBooks = useMemo(
    () => (books ? sortBooks(books, sort, locale) : []),
    [books, sort, locale]
  )

  const [selectedDraft, setSelectedDraft] = useState<ReadonlySet<string>>(
    () => new Set()
  )
  const [anchor, setAnchor] = useState<SelectionAnchor | null>(null)
  const [hoveredBookId, setHoveredBookId] = useState<string | null>(null)
  const [shiftPressed, setShiftPressed] = useState(false)
  const [batchTagOpen, setBatchTagOpen] = useState(false)
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false)

  const validBookIds = useMemo(
    () => new Set(sortedBooks.map((book) => book.id)),
    [sortedBooks]
  )
  const selectedIds = useMemo(
    () => new Set([...selectedDraft].filter((id) => validBookIds.has(id))),
    [selectedDraft, validBookIds]
  )
  const selectedBooks = useMemo(
    () => sortedBooks.filter((book) => selectedIds.has(book.id)),
    [sortedBooks, selectedIds]
  )
  const selectedBookIds = useMemo(
    () => selectedBooks.map((book) => book.id),
    [selectedBooks]
  )
  const isSelectionMode = selectedIds.size > 0
  const isAllSelected =
    sortedBooks.length > 0 && selectedIds.size === sortedBooks.length

  const clearSelection = () => {
    setSelectedDraft(new Set())
    setAnchor(null)
    setHoveredBookId(null)
    setShiftPressed(false)
  }

  const toggleSelectAll = () => {
    if (isAllSelected) {
      clearSelection()
    } else {
      setSelectedDraft(new Set(sortedBooks.map((book) => book.id)))
      setAnchor(null)
    }
  }

  const toggleSelectBook = (bookId: string, shiftKey: boolean) => {
    if (shiftKey && isSelectionMode && anchor) {
      const anchorIndex = sortedBooks.findIndex((book) => book.id === anchor.id)
      const targetIndex = sortedBooks.findIndex((book) => book.id === bookId)
      if (anchorIndex !== -1 && targetIndex !== -1) {
        const start = Math.min(anchorIndex, targetIndex)
        const end = Math.max(anchorIndex, targetIndex)
        const next = new Set(selectedIds)
        for (let index = start; index <= end; index += 1) {
          const id = sortedBooks[index].id
          if (anchor.action === "select") next.add(id)
          else next.delete(id)
        }
        setSelectedDraft(next)
        setAnchor(
          next.size > 0 ? { id: bookId, action: anchor.action } : null
        )
        if (next.size === 0) {
          setHoveredBookId(null)
          setShiftPressed(false)
        }
        return
      }
    }

    const willSelect = !selectedIds.has(bookId)
    const next = new Set(selectedIds)
    if (willSelect) next.add(bookId)
    else next.delete(bookId)
    setSelectedDraft(next)
    setAnchor(
      next.size > 0
        ? { id: bookId, action: willSelect ? "select" : "deselect" }
        : null
    )
    if (next.size === 0) {
      setHoveredBookId(null)
      setShiftPressed(false)
    }
  }

  const previewRange = useMemo(() => {
    if (!isSelectionMode || !shiftPressed || !anchor || !hoveredBookId) {
      return null
    }
    const anchorIndex = sortedBooks.findIndex((book) => book.id === anchor.id)
    const hoverIndex = sortedBooks.findIndex(
      (book) => book.id === hoveredBookId
    )
    if (anchorIndex === -1 || hoverIndex === -1) return null
    const start = Math.min(anchorIndex, hoverIndex)
    const end = Math.max(anchorIndex, hoverIndex)
    const ids = new Set<string>()
    for (let index = start; index <= end; index += 1) {
      ids.add(sortedBooks[index].id)
    }
    return { ids, action: anchor.action }
  }, [isSelectionMode, shiftPressed, anchor, hoveredBookId, sortedBooks])

  useEffect(() => {
    if (!isSelectionMode) return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Shift") setShiftPressed(true)
      if (batchTagOpen || batchDeleteOpen || event.defaultPrevented) return
      if (isEditableTarget(event.target) || hasOpenOverlay()) return
      if (event.key === "Escape") {
        event.preventDefault()
        setSelectedDraft(new Set())
        setAnchor(null)
        setHoveredBookId(null)
        setShiftPressed(false)
      } else if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === "a"
      ) {
        event.preventDefault()
        setSelectedDraft(new Set(sortedBooks.map((book) => book.id)))
        setAnchor(null)
      }
    }
    const handleKeyUp = (event: KeyboardEvent) => {
      if (event.key === "Shift") setShiftPressed(false)
    }
    const handleBlur = () => setShiftPressed(false)
    window.addEventListener("keydown", handleKeyDown)
    window.addEventListener("keyup", handleKeyUp)
    window.addEventListener("blur", handleBlur)
    return () => {
      window.removeEventListener("keydown", handleKeyDown)
      window.removeEventListener("keyup", handleKeyUp)
      window.removeEventListener("blur", handleBlur)
    }
  }, [isSelectionMode, batchTagOpen, batchDeleteOpen, sortedBooks])

  const batchDeleteMutation = useMutation({
    mutationFn: (ids: string[]) =>
      ids.length === 1
        ? library.bookshelf.book.delete(ids[0])
        : library.bookshelf.book.deleteBatch(ids),
    onSuccess: (_, ids) => {
      const deleted = new Set(ids)
      queryClient.setQueryData<Book[]>(booksQueryKey, (current) =>
        current?.filter((item) => !deleted.has(item.id))
      )
      clearSelection()
      setBatchDeleteOpen(false)
    },
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
        queryClient.invalidateQueries({ queryKey: trashQueryKey }),
      ]),
  })

  const changeSort = (next: BookSort) => {
    setSort(next)
    saveBookSort(next)
  }

  return (
    <PageSection aria-label={t("local.dashboard.bookshelf")}>
      {isSelectionMode &&
        headerOverlay &&
        createPortal(
          <BookshelfSelectionBar
            selectedCount={selectedIds.size}
            isAllSelected={isAllSelected}
            disabled={batchDeleteMutation.isPending}
            onClear={clearSelection}
            onToggleSelectAll={toggleSelectAll}
            onTag={() => setBatchTagOpen(true)}
            onDelete={() => setBatchDeleteOpen(true)}
          />,
          headerOverlay
        )}
      <PageToolbar
        start={
          books &&
          books.length > 0 && (
            <span className="tabular-nums">
              {formatCountMessage(
                books.length,
                "local.book.count_one",
                "local.book.count",
                t
              )}
            </span>
          )
        }
        end={
          <>
            <SortMenu
              fields={[
                { value: "title", label: t("local.book.sort.title") },
                { value: "date", label: t("local.book.sort.date") },
              ]}
              sort={sort}
              onChange={changeSort}
            />
            <UploadBookButton />
          </>
        }
      />
      {isPending ? (
        <div
          aria-busy="true"
          aria-label={t("local.book.loading")}
          className={GRID_CLASS_NAME}
        >
          {Array.from({ length: 8 }, (_, index) => (
            <div key={index} className="flex flex-col items-center gap-2">
              <Skeleton className="aspect-[5/7] w-full rounded-xl" />
              <Skeleton className="mt-1 h-4 w-3/4" />
              <Skeleton className="h-3 w-1/2" />
            </div>
          ))}
        </div>
      ) : isError ? (
        <EmptyState
          tone="destructive"
          icon={<CircleAlert aria-hidden="true" />}
          title={t("local.book.load_failed")}
          action={
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={isFetching}
              onClick={() => void refetch()}
            >
              <RotateCcw aria-hidden="true" />
              {t("local.common.retry")}
            </Button>
          }
        />
      ) : books.length === 0 ? (
        <EmptyState
          icon={<LibraryBig aria-hidden="true" />}
          title={t("local.book.empty")}
          description={t("local.book.empty_hint")}
        />
      ) : (
        <ul aria-label={t("local.book.list")} className={GRID_CLASS_NAME}>
          {sortedBooks.map((book) => (
            <BookCard
              key={book.id}
              book={book}
              selected={selectedIds.has(book.id)}
              isSelectionMode={isSelectionMode}
              selectionPreview={
                previewRange?.ids.has(book.id) ? previewRange.action : null
              }
              onToggleSelect={(shiftKey) => toggleSelectBook(book.id, shiftKey)}
              onHoverChange={
                isSelectionMode
                  ? (hovering) => {
                      if (hovering) setHoveredBookId(book.id)
                      else
                        setHoveredBookId((current) =>
                          current === book.id ? null : current
                        )
                    }
                  : undefined
              }
            />
          ))}
        </ul>
      )}

      <BatchBookTagDialog
        bookIds={selectedBookIds}
        open={batchTagOpen && selectedBookIds.length > 0}
        onOpenChange={setBatchTagOpen}
        onSuccess={clearSelection}
      />

      <AlertDialog
        open={batchDeleteOpen && selectedBookIds.length > 0}
        onOpenChange={(open) => {
          if (!batchDeleteMutation.isPending) setBatchDeleteOpen(open)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(
                selectedBooks.length > 1
                  ? "local.book.delete_batch_confirm_title"
                  : "local.book.delete_confirm_title"
              )}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {selectedBooks.length > 1
                ? t("local.book.delete_batch_confirm_description").replace(
                    "{count}",
                    String(selectedBooks.length)
                  )
                : t("local.book.delete_confirm_description").replace(
                    "{title}",
                    selectedBooks[0]?.title ?? ""
                  )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose
              disabled={batchDeleteMutation.isPending}
              render={<Button variant="outline" />}
            >
              {t("local.common.cancel")}
            </AlertDialogClose>
            <Button
              variant="destructive"
              disabled={
                batchDeleteMutation.isPending || selectedBookIds.length === 0
              }
              aria-busy={batchDeleteMutation.isPending}
              onClick={() => batchDeleteMutation.mutate(selectedBookIds)}
            >
              {batchDeleteMutation.isPending && (
                <LoaderCircle aria-hidden="true" className="animate-spin" />
              )}
              {t("local.book.delete")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageSection>
  )
}

function BookshelfSelectionBar({
  selectedCount,
  isAllSelected,
  disabled,
  onClear,
  onToggleSelectAll,
  onTag,
  onDelete,
}: {
  selectedCount: number
  isAllSelected: boolean
  disabled: boolean
  onClear: () => void
  onToggleSelectAll: () => void
  onTag: () => void
  onDelete: () => void
}) {
  const { t } = useTranslation()
  const selectAllLabel = t(
    isAllSelected ? "local.book.deselect_all" : "local.book.select_all"
  )

  return (
    <div
      data-slot="selection-bar"
      role="toolbar"
      aria-label={t("local.book.selection_actions")}
      className="animate-in fade-in-0 slide-in-from-top-1 flex min-w-0 flex-1 items-center justify-between gap-2 duration-150"
    >
      <div className="flex min-w-0 items-center gap-2">
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          disabled={disabled}
          aria-label={t("local.book.clear_selection")}
          title={t("local.book.clear_selection")}
          onClick={onClear}
        >
          <X aria-hidden="true" />
        </Button>
        <span className="truncate text-sm font-semibold tracking-tight tabular-nums sm:text-base">
          {t("local.book.selected_count").replace(
            "{count}",
            String(selectedCount)
          )}
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-1.5 sm:gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={selectAllLabel}
          title={selectAllLabel}
          onClick={onToggleSelectAll}
        >
          <CheckCheck aria-hidden="true" />
          <span className="hidden sm:inline">{selectAllLabel}</span>
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled}
          aria-label={t("local.book.tags")}
          title={t("local.book.tags")}
          onClick={onTag}
        >
          <Tag aria-hidden="true" />
          <span className="hidden sm:inline">{t("local.book.tags")}</span>
        </Button>
        <Button
          type="button"
          variant="destructive"
          size="sm"
          disabled={disabled}
          aria-label={t("local.book.delete")}
          title={t("local.book.delete")}
          onClick={onDelete}
        >
          <Trash2 aria-hidden="true" />
          <span className="hidden sm:inline">{t("local.book.delete")}</span>
        </Button>
      </div>
    </div>
  )
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  )
}

function hasOpenOverlay(): boolean {
  return (
    document.querySelector(
      '[role="dialog"], [role="alertdialog"], [role="menu"]'
    ) !== null
  )
}

const GRID_CLASS_NAME =
  "grid grid-cols-[repeat(auto-fill,minmax(9.5rem,1fr))] gap-x-5 gap-y-8 sm:grid-cols-[repeat(auto-fill,minmax(11rem,1fr))]"
