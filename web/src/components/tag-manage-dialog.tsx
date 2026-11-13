import { useMemo, useState, type ReactNode } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle, Pencil, Plus, Trash2 } from "lucide-react"
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
import { Dialog, DialogClose, DialogContent } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  CreateTagDialog,
  ROW_ACTIONS_CLASS_NAME,
  TAG_DIALOG_CONTENT_CLASS_NAME,
  TagDialogHeader,
  TagSearchBar,
  TagTreeBox,
  TagTreeItem,
  useTagTreeView,
} from "@/components/tag-dialog-parts"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey, tagsQueryKey } from "@/lib/query-keys"
import { collectSubtreeIds, findTag } from "@/lib/tag-tree"
import type { TagTreeNode, UndefineTagStrategy } from "@/lib/types"

type ActiveDialog =
  | { kind: "create"; parentId: string | null }
  | { kind: "delete"; tag: TagTreeNode }
  | { kind: "batch-delete"; tags: TagTreeNode[] }

/** Defines, renames and undefines tags. Every change is saved immediately. */
export function TagManageDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className={TAG_DIALOG_CONTENT_CLASS_NAME}>
        {open && <TagManageForm />}
      </DialogContent>
    </Dialog>
  )
}

function TagManageForm() {
  const { t } = useTranslation()
  const view = useTagTreeView()
  const { tagsQuery, tree, expanded, toggleExpand } = view
  const [active, setActive] = useState<ActiveDialog | null>(null)
  const [selectionDraft, setSelected] = useState<ReadonlySet<string>>(
    () => new Set()
  )
  // Tags that no longer exist (deleted here or elsewhere) drop out of the selection.
  const selectedTags = useMemo(
    () =>
      [...selectionDraft]
        .map((id) => findTag(tree, id))
        .filter((tag): tag is TagTreeNode => tag !== undefined),
    [selectionDraft, tree]
  )
  const selected = useMemo(
    () => new Set(selectedTags.map((tag) => tag.id)),
    [selectedTags]
  )

  const toggleSelect = (node: TagTreeNode, checked: boolean) =>
    setSelected((current) => {
      const next = new Set(current)
      for (const id of collectSubtreeIds(node)) {
        if (checked) next.add(id)
        else next.delete(id)
      }
      return next
    })

  const closeActive = () => setActive(null)

  const renderRow = (node: TagTreeNode, depth: number) => (
    <ManageTreeRow
      key={node.id}
      node={node}
      depth={depth}
      expanded={expanded}
      selected={selected}
      onToggleExpand={toggleExpand}
      onToggleSelect={toggleSelect}
      onAddChild={(parent) => setActive({ kind: "create", parentId: parent.id })}
      onDelete={(tag) => setActive({ kind: "delete", tag })}
      renderChild={(child) => renderRow(child, depth + 1)}
    />
  )

  return (
    <>
      <TagDialogHeader
        title={t("tag.manage")}
        end={
          <DialogClose render={<Button variant="ghost" size="sm" />}>
            {t("common.done")}
          </DialogClose>
        }
      />

      <TagSearchBar
        query={view.query}
        disabled={false}
        onQueryChange={view.setQuery}
        onDefine={() => setActive({ kind: "create", parentId: null })}
      />

      <TagTreeBox
        tree={tree}
        visibleTree={view.visibleTree}
        isLoading={tagsQuery.isPending}
        isError={tagsQuery.isError}
        errorMessage={t("tag.load_failed")}
        isRetrying={tagsQuery.isFetching}
        onRetry={() => void tagsQuery.refetch()}
        multiselectable
        renderRow={(node) => renderRow(node, 0)}
      />

      <div className="flex items-center gap-2">
        <span className="mr-auto text-xs text-muted-foreground">
          {t("tag.selected_count").replace("{count}", String(selected.size))}
        </span>
        <Button
          type="button"
          variant="outline"
          disabled={selectedTags.length === 0}
          className="text-destructive hover:text-destructive"
          onClick={() => setActive({ kind: "batch-delete", tags: selectedTags })}
        >
          <Trash2 aria-hidden="true" />
          {t("tag.batch_delete")}
        </Button>
      </div>

      {active?.kind === "create" && (
        <CreateTagDialog parentId={active.parentId} onClose={closeActive} />
      )}
      {active?.kind === "delete" && (
        <DeleteTagDialog tag={active.tag} onClose={closeActive} />
      )}
      {active?.kind === "batch-delete" && (
        <BatchDeleteTagDialog tags={active.tags} onClose={closeActive} />
      )}
    </>
  )
}

function ManageTreeRow({
  node,
  depth,
  expanded,
  selected,
  onToggleExpand,
  onToggleSelect,
  onAddChild,
  onDelete,
  renderChild,
}: {
  node: TagTreeNode
  depth: number
  expanded: ReadonlySet<string>
  selected: ReadonlySet<string>
  onToggleExpand: (id: string) => void
  onToggleSelect: (node: TagTreeNode, checked: boolean) => void
  onAddChild: (node: TagTreeNode) => void
  onDelete: (node: TagTreeNode) => void
  renderChild: (child: TagTreeNode) => ReactNode
}) {
  const { t } = useTranslation()
  const [renaming, setRenaming] = useState(false)
  const checked = selected.has(node.id)

  return (
    <TagTreeItem
      node={node}
      depth={depth}
      expanded={expanded}
      disabled={false}
      selected={checked}
      onToggleExpand={onToggleExpand}
      renderChild={renderChild}
    >
      {renaming ? (
        <RenameTagInput node={node} onDone={() => setRenaming(false)} />
      ) : (
        <>
          <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-1.5 text-sm">
            <Checkbox
              checked={checked}
              onCheckedChange={(next) => onToggleSelect(node, next === true)}
            />
            <span title={node.name} className="min-w-0 truncate">
              {node.name}
            </span>
          </label>
          <div className={ROW_ACTIONS_CLASS_NAME}>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("tag.new_child")}
              title={t("tag.new_child")}
              onClick={() => onAddChild(node)}
            >
              <Plus aria-hidden="true" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("tag.rename")}
              title={t("tag.rename")}
              onClick={() => setRenaming(true)}
            >
              <Pencil aria-hidden="true" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("tag.delete")}
              title={t("tag.delete")}
              className="text-destructive hover:text-destructive"
              onClick={() => onDelete(node)}
            >
              <Trash2 aria-hidden="true" />
            </Button>
          </div>
        </>
      )}
    </TagTreeItem>
  )
}

function RenameTagInput({
  node,
  onDone,
}: {
  node: TagTreeNode
  onDone: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState(node.name)
  const renameMutation = useMutation({
    mutationFn: (name: string) => library.tag.rename(node.id, name),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: tagsQueryKey })
      onDone()
    },
  })

  // Enter saves. An empty or unchanged name sends nothing and only leaves edit mode.
  const commit = () => {
    if (renameMutation.isPending) return
    const name = draft.trim()
    if (name === "" || name === node.name) {
      onDone()
      return
    }
    renameMutation.mutate(name)
  }

  return (
    <Input
      autoFocus
      value={draft}
      aria-label={t("tag.rename")}
      aria-busy={renameMutation.isPending}
      disabled={renameMutation.isPending}
      className="h-7 min-w-0 flex-1 text-sm"
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return
        if (event.key === "Enter") {
          event.preventDefault()
          commit()
        } else if (event.key === "Escape") {
          // Only leave edit mode; the dialog itself stays open.
          event.preventDefault()
          event.stopPropagation()
          onDone()
        }
      }}
      onBlur={() => {
        // Leaving the field drops the draft unless a save is in flight.
        if (!renameMutation.isPending) onDone()
      }}
    />
  )
}

function DeleteTagDialog({
  tag,
  onClose,
}: {
  tag: TagTreeNode
  onClose: () => void
}) {
  const { t } = useTranslation()
  const hasChildren = tag.children.length > 0
  // Promoting children is the non-destructive default.
  const [strategy, setStrategy] = useState<UndefineTagStrategy>("PROMOTE")
  const queryClient = useQueryClient()
  const deleteMutation = useMutation({
    mutationFn: () => library.tag.undefine(tag.id, strategy),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: tagsQueryKey }),
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
      ])
      onClose()
    },
  })
  const options: { value: UndefineTagStrategy; label: string; hint: string }[] = [
    {
      value: "PROMOTE",
      label: t("tag.delete_strategy.promote"),
      hint: t("tag.delete_strategy.promote_hint"),
    },
    {
      value: "CASCADE",
      label: t("tag.delete_strategy.cascade"),
      hint: t("tag.delete_strategy.cascade_hint"),
    },
  ]

  const confirm = () => {
    if (!deleteMutation.isPending) deleteMutation.mutate()
  }

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open && !deleteMutation.isPending) onClose()
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("tag.delete_confirm_title")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("tag.delete_confirm_description").replace("{name}", tag.name)}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {hasChildren && (
          <div role="radiogroup" className="grid gap-2">
            {options.map((option) => (
              <label
                key={option.value}
                className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 text-left transition-colors has-checked:border-primary has-checked:bg-primary/5"
              >
                <input
                  type="radio"
                  name="delete-strategy"
                  value={option.value}
                  checked={strategy === option.value}
                  disabled={deleteMutation.isPending}
                  onChange={() => setStrategy(option.value)}
                  className="mt-0.5 accent-primary"
                />
                <span className="grid gap-0.5">
                  <span className="text-sm font-medium">{option.label}</span>
                  <span className="text-xs text-muted-foreground">
                    {option.hint}
                  </span>
                </span>
              </label>
            ))}
          </div>
        )}
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
            onClick={confirm}
          >
            {deleteMutation.isPending && (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            )}
            {t("tag.delete")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function BatchDeleteTagDialog({
  tags,
  onClose,
}: {
  tags: TagTreeNode[]
  onClose: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const deleteMutation = useMutation({
    mutationFn: async () => {
      // Delete each tag with CASCADE strategy
      for (const tag of tags) {
        await library.tag.undefine(tag.id, "CASCADE")
      }
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: tagsQueryKey }),
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
      ])
      onClose()
    },
  })

  const confirm = () => {
    if (!deleteMutation.isPending) deleteMutation.mutate()
  }

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open && !deleteMutation.isPending) onClose()
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("tag.batch_delete_confirm_title")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("tag.batch_delete_confirm_description").replace(
              "{count}",
              String(tags.length)
            )}
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
            onClick={confirm}
          >
            {deleteMutation.isPending && (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            )}
            {t("tag.delete")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
