import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  CircleAlert,
  LoaderCircle,
  RotateCcw,
  Trash,
  Trash2,
} from "lucide-react"
import { library } from "@/api/library"
import { EmptyState } from "@/components/empty-state"
import { PageSection, PageToolbar } from "@/components/page-layout"
import { SortMenu, type SortOrder } from "@/components/sort-menu"
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
import { Checkbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"
import { BookCover } from "@/features/books/book-cover"
import { formatBookDate } from "@/lib/format"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey, trashQueryKey } from "@/lib/query-keys"
import type { TrashedBook } from "@/lib/types"
import { cn } from "cn"

type SortField = "title" | "deleted_at"
type TrashSort = { field: SortField; order: SortOrder }

const DEFAULT_SORT: TrashSort = { field: "deleted_at", order: "desc" }

function sortTrash(
  books: TrashedBook[],
  sort: TrashSort,
  locale: string
): TrashedBook[] {
  const sorted = [...books]
  const collator = new Intl.Collator(locale, { sensitivity: "base" })

  sorted.sort((a, b) => {
    if (sort.field === "title") {
      const comparison = collator.compare(a.title, b.title)
      return sort.order === "asc" ? comparison : -comparison
    } else {
      const comparison = a.deleted_at.localeCompare(b.deleted_at)
      return sort.order === "asc" ? comparison : -comparison
    }
  })

  return sorted
}

export function TrashList() {
  const { t, locale } = useTranslation()
  const queryClient = useQueryClient()
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [sort, setSort] = useState<TrashSort>(DEFAULT_SORT)
  const [permanentDeleteOpen, setPermanentDeleteOpen] = useState(false)
  const [itemToDelete, setItemToDelete] = useState<TrashedBook | null>(null)
  const [idsToDelete, setIdsToDelete] = useState<string[]>([])

  const {
    data: trash,
    isPending,
    isError,
    isFetching,
    refetch,
  } = useQuery<TrashedBook[]>({
    queryKey: trashQueryKey,
    queryFn: () => library.bookshelf.trash.list(),
  })

  const sortedTrash = useMemo(
    () => (trash ? sortTrash(trash, sort, locale) : []),
    [trash, sort, locale]
  )

  const updateTrash = (update: (trash: TrashedBook[]) => TrashedBook[]) =>
    queryClient.setQueryData<TrashedBook[]>(
      trashQueryKey,
      (trash) => trash && update(trash)
    )

  const restoreMutation = useMutation({
    mutationFn: (ids: string[]) =>
      ids.length === 1
        ? library.bookshelf.trash.restore(ids[0])
        : library.bookshelf.trash.restoreBatch(ids),
    onSuccess: (_, ids) => {
      updateTrash((trash) => trash.filter((item) => !ids.includes(item.id)))
      setSelectedIds(new Set())
    },
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: trashQueryKey }),
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
      ]),
  })

  const permanentDeleteMutation = useMutation({
    mutationFn: (ids: string[]) =>
      ids.length === 1
        ? library.bookshelf.trash.permanentlyDelete(ids[0])
        : library.bookshelf.trash.permanentlyDeleteBatch(ids),
    onSuccess: (_, ids) => {
      updateTrash((trash) => trash.filter((item) => !ids.includes(item.id)))
      setSelectedIds((selected) => new Set([...selected].filter((id) => !ids.includes(id))))
      setPermanentDeleteOpen(false)
      setItemToDelete(null)
      setIdsToDelete([])
    },
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: trashQueryKey }),
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
      ]),
  })

  const handleSelectAll = (checked: boolean) => {
    if (checked && trash) {
      setSelectedIds(new Set(trash.map((item) => item.id)))
    } else {
      setSelectedIds(new Set())
    }
  }

  const handleSelectItem = (id: string, checked: boolean) => {
    const newSelected = new Set(selectedIds)
    if (checked) {
      newSelected.add(id)
    } else {
      newSelected.delete(id)
    }
    setSelectedIds(newSelected)
  }

  const handleRestoreSelected = () => {
    if (selectedIds.size > 0) {
      restoreMutation.mutate(Array.from(selectedIds))
    }
  }

  const handlePermanentDelete = (item: TrashedBook) => {
    setItemToDelete(item)
    setIdsToDelete([item.id])
    setPermanentDeleteOpen(true)
  }

  const handlePermanentDeleteSelected = () => {
    setItemToDelete(null)
    setIdsToDelete(Array.from(selectedIds))
    setPermanentDeleteOpen(true)
  }

  const confirmPermanentDelete = () => {
    if (idsToDelete.length > 0) {
      permanentDeleteMutation.mutate(idsToDelete)
    }
  }

  const isAllSelected = !!trash && trash.length > 0 && selectedIds.size === trash.length
  const isSomeSelected = selectedIds.size > 0 && !isAllSelected

  return (
    <PageSection aria-label={t("local.dashboard.trash")}>
      <PageToolbar
        start={
          trash &&
          trash.length > 0 && (
            <label className="flex cursor-pointer items-center gap-2.5 select-none">
              <Checkbox
                checked={isAllSelected}
                indeterminate={isSomeSelected}
                onCheckedChange={handleSelectAll}
                aria-label={t("local.trash.select_all")}
              />
              <span className="tabular-nums">
                {selectedIds.size > 0
                  ? t("local.trash.selected_count").replace(
                      "{count}",
                      String(selectedIds.size)
                    )
                  : t("local.trash.count").replace(
                      "{count}",
                      String(trash.length)
                    )}
              </span>
            </label>
          )
        }
        end={
          <>
            {selectedIds.size > 0 && (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleRestoreSelected}
                  disabled={restoreMutation.isPending}
                  aria-busy={restoreMutation.isPending}
                >
                  {restoreMutation.isPending ? (
                    <LoaderCircle aria-hidden="true" className="animate-spin" />
                  ) : (
                    <RotateCcw aria-hidden="true" />
                  )}
                  {t("local.trash.restore_selected")}
                </Button>
                <Button
                  type="button"
                  variant="destructive"
                  size="sm"
                  onClick={handlePermanentDeleteSelected}
                  disabled={permanentDeleteMutation.isPending || restoreMutation.isPending}
                >
                  <Trash2 aria-hidden="true" />
                  {t("local.trash.permanently_delete_selected")}
                </Button>
              </>
            )}
            <SortMenu
              fields={[
                { value: "title", label: t("local.book.sort.title") },
                { value: "deleted_at", label: t("local.trash.sort.deleted_date") },
              ]}
              sort={sort}
              onChange={setSort}
            />
          </>
        }
      />

      {isPending ? (
        <div
          aria-busy="true"
          aria-label={t("local.trash.loading")}
          className={LIST_CLASS_NAME}
        >
          {Array.from({ length: 5 }, (_, index) => (
            <div key={index} className={cn(ITEM_CLASS_NAME, "bg-card")}>
              <Skeleton className="size-4" />
              <Skeleton className="h-14 w-[42px] rounded-md" />
              <div className="flex-1 space-y-2">
                <Skeleton className="h-4 w-3/4" />
                <Skeleton className="h-3 w-1/4" />
              </div>
              <Skeleton className="h-7 w-16" />
            </div>
          ))}
        </div>
      ) : isError ? (
        <EmptyState
          tone="destructive"
          icon={<CircleAlert aria-hidden="true" />}
          title={t("local.trash.load_failed")}
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
      ) : trash.length === 0 ? (
        <EmptyState
          icon={<Trash aria-hidden="true" />}
          title={t("local.trash.empty")}
          description={t("local.trash.empty_hint")}
        />
      ) : (
        <ul aria-label={t("local.trash.list")} className={LIST_CLASS_NAME}>
          {sortedTrash.map((item) => (
            <TrashListItem
              key={item.id}
              item={item}
              selected={selectedIds.has(item.id)}
              onSelectChange={(checked) => handleSelectItem(item.id, checked)}
              onRestore={() => restoreMutation.mutate([item.id])}
              onDelete={() => handlePermanentDelete(item)}
              disabled={restoreMutation.isPending || permanentDeleteMutation.isPending}
            />
          ))}
        </ul>
      )}

      <AlertDialog
        open={permanentDeleteOpen}
        onOpenChange={(open) => {
          if (!permanentDeleteMutation.isPending) {
            setPermanentDeleteOpen(open)
            if (!open) setItemToDelete(null)
            if (!open) setIdsToDelete([])
          }
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(idsToDelete.length > 1
                ? "local.trash.permanently_delete_batch_confirm_title"
                : "local.trash.permanently_delete_confirm_title")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {idsToDelete.length > 1
                ? t("local.trash.permanently_delete_batch_confirm_description").replace(
                    "{count}",
                    String(idsToDelete.length)
                  )
                : t("local.trash.permanently_delete_confirm_description").replace(
                    "{title}",
                    itemToDelete?.title || ""
                  )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose
              disabled={permanentDeleteMutation.isPending}
              render={<Button variant="outline" />}
            >
              {t("local.common.cancel")}
            </AlertDialogClose>
            <Button
              variant="destructive"
              disabled={permanentDeleteMutation.isPending}
              aria-busy={permanentDeleteMutation.isPending}
              onClick={confirmPermanentDelete}
            >
              {permanentDeleteMutation.isPending && (
                <LoaderCircle aria-hidden="true" className="animate-spin" />
              )}
              {t("local.trash.permanently_delete")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageSection>
  )
}

const LIST_CLASS_NAME = "grid gap-2"

const ITEM_CLASS_NAME =
  "flex items-center gap-3 rounded-xl border p-3 sm:gap-4"

function TrashListItem({
  item,
  selected,
  onSelectChange,
  onRestore,
  onDelete,
  disabled,
}: {
  item: TrashedBook
  selected: boolean
  onSelectChange: (checked: boolean) => void
  onRestore: () => void
  onDelete: () => void
  disabled: boolean
}) {
  const { t } = useTranslation()

  return (
    <li
      data-selected={selected || undefined}
      className={cn(
        ITEM_CLASS_NAME,
        "bg-card transition-colors hover:bg-accent/50 data-selected:bg-accent"
      )}
    >
      <Checkbox
        checked={selected}
        onCheckedChange={onSelectChange}
        aria-label={t("local.trash.select_item").replace("{title}", item.title)}
        disabled={disabled}
      />

      <BookCover
        bookId={item.id}
        className="h-14 w-[42px] shrink-0 rounded-md"
        iconClassName="size-5"
      />

      <div className="min-w-0 flex-1">
        <h3 title={item.title} className="truncate text-sm font-medium">
          {item.title}
        </h3>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("local.trash.deleted_at")}{" "}
          <time dateTime={item.deleted_at} className="tabular-nums">
            {formatBookDate(item.deleted_at)}
          </time>
        </p>
      </div>

      <div className="flex items-center gap-1.5">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onRestore}
          disabled={disabled}
          title={t("local.trash.restore")}
        >
          <RotateCcw aria-hidden="true" />
          <span className="hidden sm:inline">{t("local.trash.restore")}</span>
        </Button>
        <Button
          type="button"
          variant="destructive"
          size="icon-sm"
          onClick={onDelete}
          disabled={disabled}
          title={t("local.trash.permanently_delete")}
          aria-label={t("local.trash.permanently_delete")}
        >
          <Trash2 aria-hidden="true" />
        </Button>
      </div>
    </li>
  )
}
