import { useEffect, useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  ArrowDownWideNarrow,
  ArrowUpNarrowWide,
  BookOpen,
  LoaderCircle,
  RotateCcw,
  Trash,
  Trash2,
} from "lucide-react"
import { library } from "@/api/library"
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
import { Skeleton } from "@/components/ui/skeleton"
import { formatBookDate } from "@/lib/format"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey, trashQueryKey } from "@/lib/query-keys"
import type { TrashedBook } from "@/lib/types"

type SortField = "title" | "deleted_at"
type SortOrder = "asc" | "desc"
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

  const isAllSelected = trash && trash.length > 0 && selectedIds.size === trash.length
  const isSomeSelected = selectedIds.size > 0 && !isAllSelected

  return (
    <section
      aria-labelledby="trash-title"
      className="flex flex-1 flex-col gap-5 p-6"
    >
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          {trash && trash.length > 0 && (
            <>
              <Checkbox
                id="select-all"
                checked={isAllSelected}
                onCheckedChange={handleSelectAll}
                aria-label={t("local.trash.select_all")}
                className={isSomeSelected ? "data-[state=checked]:bg-muted-foreground" : ""}
              />
              {selectedIds.size > 0 && (
                <span className="text-sm text-muted-foreground">
                  {t("local.trash.selected_count").replace(
                    "{count}",
                    String(selectedIds.size)
                  )}
                </span>
              )}
            </>
          )}
        </div>
        <div className="flex items-center gap-2">
          {selectedIds.size > 0 && (
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
          )}
          {selectedIds.size > 0 && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleRestoreSelected}
              disabled={restoreMutation.isPending}
              aria-busy={restoreMutation.isPending}
            >
              {restoreMutation.isPending && (
                <LoaderCircle aria-hidden="true" className="animate-spin" />
              )}
              <RotateCcw aria-hidden="true" />
              {t("local.trash.restore_selected")}
            </Button>
          )}
          <SortTrashMenu sort={sort} onChange={setSort} />
        </div>
      </div>

      {isPending ? (
        <div
          aria-busy="true"
          aria-label={t("local.trash.loading")}
          className="space-y-3"
        >
          {Array.from({ length: 5 }, (_, index) => (
            <div key={index} className="flex items-center gap-4 rounded-lg border border-border/50 p-4">
              <Skeleton className="size-5" />
              <Skeleton className="size-16 rounded-md" />
              <div className="flex-1 space-y-2">
                <Skeleton className="h-5 w-3/4" />
                <Skeleton className="h-4 w-1/4" />
              </div>
              <Skeleton className="h-9 w-20" />
            </div>
          ))}
        </div>
      ) : isError ? (
        <p role="status" className="text-sm text-muted-foreground">
          {t("local.trash.load_failed")}
        </p>
      ) : trash.length === 0 ? (
        <div
          role="status"
          className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"
        >
          <div className="flex size-20 items-center justify-center rounded-2xl bg-muted/80">
            <Trash aria-hidden="true" className="size-10" />
          </div>
          <p className="text-sm font-medium text-foreground">
            {t("local.trash.empty")}
          </p>
        </div>
      ) : (
        <ul aria-label={t("local.trash.list")} className="space-y-2">
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
    </section>
  )
}

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
  const [coverFailed, setCoverFailed] = useState(false)
  const [coverUrl, setCoverUrl] = useState<string>()

  // Load cover image
  useEffect(() => {
    let active = true
    let objectUrl: string | undefined
    void library.bookshelf.book
      .getCover(item.id)
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
  }, [item.id])

  return (
    <li className="group flex items-center gap-4 rounded-lg border border-border/50 bg-card p-4 transition-colors hover:border-border hover:bg-accent/5">
      <Checkbox
        checked={selected}
        onCheckedChange={onSelectChange}
        aria-label={t("local.trash.select_item").replace("{title}", item.title)}
        disabled={disabled}
      />

      <div className="relative h-14 w-[42px] flex-shrink-0 overflow-hidden rounded-md border border-border/50 bg-muted">
        {coverUrl && !coverFailed ? (
          <img
            src={coverUrl}
            alt=""
            loading="lazy"
            className="size-full object-contain"
            onError={() => setCoverFailed(true)}
          />
        ) : (
          <div className="flex size-full items-center justify-center bg-gradient-to-br from-muted to-muted-foreground/5 text-muted-foreground">
            <BookOpen aria-hidden="true" className="size-6 opacity-40" />
          </div>
        )}
      </div>

      <div className="flex-1 min-w-0">
        <h3
          title={item.title}
          className="font-medium text-sm truncate text-foreground"
        >
          {item.title}
        </h3>
        <p className="text-xs text-muted-foreground mt-1">
          {t("local.trash.deleted_at")}{" "}
          <time dateTime={item.deleted_at}>
            {formatBookDate(item.deleted_at)}
          </time>
        </p>
      </div>

      <div className="flex items-center gap-2">
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
          variant="ghost"
          size="sm"
          onClick={onDelete}
          disabled={disabled}
          title={t("local.trash.permanently_delete")}
          className="text-destructive hover:text-destructive hover:bg-destructive/10"
        >
          <Trash2 aria-hidden="true" />
        </Button>
      </div>
    </li>
  )
}

function SortTrashMenu({
  sort,
  onChange,
}: {
  sort: TrashSort
  onChange: (sort: TrashSort) => void
}) {
  const { t } = useTranslation()
  const Icon = sort.order === "asc" ? ArrowUpNarrowWide : ArrowDownWideNarrow

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button type="button" variant="outline" size="sm" />}
      >
        <Icon aria-hidden="true" />
        {t(
          sort.field === "title"
            ? "local.book.sort.title"
            : "local.trash.sort.deleted_date"
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent aria-label={t("local.book.sort")}>
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("local.book.sort.by")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.field}
            onValueChange={(field: SortField) => onChange({ ...sort, field })}
          >
            <DropdownMenuRadioItem value="title">
              {t("local.book.sort.title")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="deleted_at">
              {t("local.trash.sort.deleted_date")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("local.book.sort.order")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.order}
            onValueChange={(order: SortOrder) => onChange({ ...sort, order })}
          >
            <DropdownMenuRadioItem value="asc">
              {t("local.book.sort.asc")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="desc">
              {t("local.book.sort.desc")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
