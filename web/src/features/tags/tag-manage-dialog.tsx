import { useMemo, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Dialog, DialogClose, DialogContent } from "@/components/ui/dialog"
import {
  CreateTagDialog,
  TAG_DIALOG_CONTENT_CLASS_NAME,
  TagDialogHeader,
  TagSearchBar,
  TagTreeBox,
  useTagTreeView,
} from "@/features/tags/tag-dialog-parts"
import {
  BatchDeleteTagDialog,
  DeleteTagDialog,
  ManageTreeRow,
} from "@/features/tags/tag-manage-controls"
import {
  addPendingCreates,
  applyPendingMoves,
  collectCascadeDeletedIds,
  collectSelectedRoots,
  collectTagMetadata,
  commitTagChanges,
  prepareChanges,
  resolveParentAfterPartialCommit,
  TagManageCommitError,
  type PendingCreate,
  type PendingDelete,
} from "@/features/tags/tag-manage-model"
import { useTagTreeDrag } from "@/features/tags/use-tag-tree-drag"
import { useTranslation } from "@/lib/i18n"
import { booksQueryKey, tagsQueryKey } from "@/lib/query-keys"
import { collectSubtreeIds, findTag } from "@/lib/tag-tree"
import { filterTagTree } from "@/lib/tag-filter"
import type { Tag, TagTreeNode, UndefineTagStrategy } from "@/lib/types"

type ActiveDialog =
  | { kind: "create"; parentId: string | null }
  | { kind: "delete"; tag: TagTreeNode }
  | { kind: "batch-delete"; tags: TagTreeNode[]; count: number }

/** Stages tag definitions, moves, renames and deletions until Done is pressed. */
export function TagManageDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [isSaving, setIsSaving] = useState(false)

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!isSaving) onOpenChange(next)
      }}
    >
      <DialogContent className={TAG_DIALOG_CONTENT_CLASS_NAME}>
        {open && (
          <TagManageForm
            onClose={() => onOpenChange(false)}
            onSavingChange={setIsSaving}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function TagManageForm({
  onClose,
  onSavingChange,
}: {
  onClose: () => void
  onSavingChange: (isSaving: boolean) => void
}) {
  const { t } = useTranslation()
  const view = useTagTreeView()
  const { tagsQuery, expanded, toggleExpand, expand } = view
  const [active, setActive] = useState<ActiveDialog | null>(null)
  const [pendingCreates, setPendingCreates] = useState<PendingCreate[]>([])
  const [pendingRenames, setPendingRenames] = useState<
    ReadonlyMap<string, string>
  >(() => new Map())
  const [pendingRecolors, setPendingRecolors] = useState<
    ReadonlyMap<string, string>
  >(() => new Map())
  const [pendingMoves, setPendingMoves] = useState<
    ReadonlyMap<string, string | null>
  >(() => new Map())
  const [pendingDeletes, setPendingDeletes] = useState<
    ReadonlyMap<string, PendingDelete>
  >(() => new Map())
  const [selectionDraft, setSelected] = useState<ReadonlySet<string>>(
    () => new Set()
  )
  const tree = useMemo(
    () =>
      applyPendingMoves(
        addPendingCreates(view.tree, pendingCreates),
        pendingMoves
      ),
    [view.tree, pendingCreates, pendingMoves]
  )
  const visibleTree = useMemo(
    () => filterTagTree(tree, view.query),
    [tree, view.query]
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
  const selectedRoots = useMemo(
    () => collectSelectedRoots(tree, selected),
    [tree, selected]
  )
  const tagMetadata = useMemo(() => collectTagMetadata(tree), [tree])
  const cascadeDeletedIds = useMemo(
    () => collectCascadeDeletedIds(tree, pendingDeletes),
    [tree, pendingDeletes]
  )
  const pendingCount =
    pendingCreates.length +
    pendingMoves.size +
    pendingRenames.size +
    pendingRecolors.size +
    pendingDeletes.size
  const queryClient = useQueryClient()
  const saveMutation = useMutation({
    mutationFn: commitTagChanges,
    onMutate: () => onSavingChange(true),
    onError: (error, changes) => {
      if (!(error instanceof TagManageCommitError)) return
      const createdDraftIds = new Set(error.progress.createdIds.keys())
      const movedIds = new Set(error.progress.movedIds)
      const renamedIds = new Set(error.progress.renamedIds)
      const recoloredIds = new Set(error.progress.recoloredIds)
      const deletedIds = new Set(error.progress.deletedIds)
      const removedIds = new Set<string>()
      for (const [id, strategy] of changes.deletes) {
        if (!deletedIds.has(id)) continue
        const tag = findTag(tree, id)
        if (strategy === "CASCADE" && tag) {
          for (const descendantId of collectSubtreeIds(tag)) {
            removedIds.add(descendantId)
          }
        } else {
          removedIds.add(id)
        }
      }
      setPendingRenames(
        (current) =>
          new Map(
            [...current].filter(
              ([id]) => !renamedIds.has(id) && !removedIds.has(id)
            )
          )
      )
      setPendingRecolors(
        (current) =>
          new Map(
            [...current].filter(
              ([id]) => !recoloredIds.has(id) && !removedIds.has(id)
            )
          )
      )
      setPendingMoves(
        (current) =>
          new Map(
            [...current]
              .filter(([id]) => !movedIds.has(id) && !removedIds.has(id))
              .map(([id, parentId]) => [
                id,
                parentId
                  ? (error.progress.createdIds.get(parentId) ?? parentId)
                  : null,
              ])
          )
      )
      setPendingDeletes(
        (current) => new Map([...current].filter(([id]) => !removedIds.has(id)))
      )
      setPendingCreates((current) =>
        current
          .filter(
            (create) =>
              !createdDraftIds.has(create.draftId) &&
              !removedIds.has(create.draftId)
          )
          .map((create) => {
            const committedParentId = resolveParentAfterPartialCommit(
              create.parent_id,
              deletedIds,
              pendingDeletes,
              pendingCreates,
              tree
            )
            return {
              ...create,
              parent_id: committedParentId
                ? (error.progress.createdIds.get(committedParentId) ??
                  committedParentId)
                : null,
            }
          })
      )
    },
    onSettled: async (_data, error) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: tagsQueryKey }),
        queryClient.invalidateQueries({ queryKey: booksQueryKey }),
      ])
      onSavingChange(false)
      if (!error) onClose()
    },
  })

  const stageCreate = (definition: Omit<Tag, "id">) => {
    setPendingCreates((current) => [
      ...current,
      { ...definition, draftId: `draft:${crypto.randomUUID()}` },
    ])
  }

  const stageRename = (id: string, name: string) => {
    if (pendingCreates.some((create) => create.draftId === id)) {
      setPendingCreates((current) =>
        current.map((create) =>
          create.draftId === id ? { ...create, name } : create
        )
      )
      return
    }
    setPendingRenames((current) => {
      const next = new Map(current)
      const originalName = findTag(tree, id)?.name
      if (name === originalName) next.delete(id)
      else next.set(id, name)
      return next
    })
  }

  const stageRecolor = (id: string, color: string) => {
    if (pendingCreates.some((create) => create.draftId === id)) {
      setPendingCreates((current) =>
        current.map((create) =>
          create.draftId === id ? { ...create, color } : create
        )
      )
      return
    }
    setPendingRecolors((current) => {
      const next = new Map(current)
      const originalColor = findTag(tree, id)?.color
      if (color.toLowerCase() === originalColor?.toLowerCase()) next.delete(id)
      else next.set(id, color)
      return next
    })
  }

  const stageDelete = (id: string, strategy: UndefineTagStrategy) =>
    setPendingDeletes((current) => new Map(current).set(id, { strategy }))

  const restoreDelete = (id: string) =>
    setPendingDeletes((current) => {
      const next = new Map(current)
      next.delete(id)
      return next
    })

  const stageMove = (id: string, parentId: string | null) => {
    const node = findTag(tree, id)
    if (
      !node ||
      node.parent_id === parentId ||
      (parentId &&
        (collectSubtreeIds(node).includes(parentId) ||
          pendingDeletes.has(parentId) ||
          cascadeDeletedIds.has(parentId)))
    ) {
      return
    }
    if (parentId) expand(parentId)
    if (id.startsWith("draft:")) {
      setPendingCreates((current) =>
        current.map((create) =>
          create.draftId === id ? { ...create, parent_id: parentId } : create
        )
      )
    } else {
      setPendingMoves((current) => {
        const next = new Map(current)
        if (view.tree.length && findTag(view.tree, id)?.parent_id === parentId)
          next.delete(id)
        else next.set(id, parentId)
        return next
      })
    }
  }

  const { isDragging, startDrag } = useTagTreeDrag({
    isSaving: saveMutation.isPending,
    pendingDeletes,
    cascadeDeletedIds,
    pendingRenames,
    pendingRecolors,
    onMove: stageMove,
  })

  const submit = () => {
    if (saveMutation.isPending) return
    const changes = prepareChanges(
      view.tree,
      tree,
      tagMetadata,
      pendingCreates,
      pendingMoves,
      pendingRenames,
      pendingRecolors,
      pendingDeletes
    )
    if (
      changes.creates.length === 0 &&
      changes.moves.length === 0 &&
      changes.renames.length === 0 &&
      changes.recolors.length === 0 &&
      changes.deletes.length === 0
    ) {
      onClose()
      return
    }
    saveMutation.mutate(changes)
  }

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
      pendingRename={pendingRenames.get(node.id)}
      pendingRecolor={pendingRecolors.get(node.id)}
      pendingMove={pendingMoves.has(node.id)}
      pendingDelete={pendingDeletes.has(node.id)}
      pendingCreate={pendingCreates.some(
        (create) => create.draftId === node.id
      )}
      coveredByCascade={
        cascadeDeletedIds.has(node.id) && !pendingDeletes.has(node.id)
      }
      disabled={saveMutation.isPending}
      dragging={isDragging}
      onStageRename={stageRename}
      onStageRecolor={stageRecolor}
      onPointerDown={(event) => startDrag(node, event)}
      onRestoreDelete={restoreDelete}
      onAddChild={(parent) =>
        setActive({ kind: "create", parentId: parent.id })
      }
      onDelete={(tag) => setActive({ kind: "delete", tag })}
      renderChild={(child) => renderRow(child, depth + 1)}
    />
  )

  return (
    <>
      <TagDialogHeader
        title={t("tag.manage")}
        start={
          <DialogClose
            disabled={saveMutation.isPending}
            render={<Button variant="ghost" size="sm" />}
          >
            {t("common.cancel")}
          </DialogClose>
        }
        end={
          <Button
            size="sm"
            disabled={saveMutation.isPending}
            aria-busy={saveMutation.isPending}
            onClick={submit}
          >
            {saveMutation.isPending && (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            )}
            {t("common.done")}
          </Button>
        }
      />

      <TagSearchBar
        query={view.query}
        disabled={saveMutation.isPending}
        onQueryChange={view.setQuery}
        onDefine={() => setActive({ kind: "create", parentId: null })}
      />

      <TagTreeBox
        tree={tree}
        visibleTree={visibleTree}
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
          {pendingCount > 0 && (
            <span className="ml-2">
              {t("tag.pending_changes").replace(
                "{count}",
                String(pendingCount)
              )}
            </span>
          )}
        </span>
        <Button
          type="button"
          variant="outline"
          disabled={selectedRoots.length === 0 || saveMutation.isPending}
          className="text-destructive hover:text-destructive"
          onClick={() =>
            setActive({
              kind: "batch-delete",
              tags: selectedRoots,
              count: selectedTags.length,
            })
          }
        >
          <Trash2 aria-hidden="true" />
          {t("tag.batch_delete")}
        </Button>
      </div>

      {active?.kind === "create" && (
        <CreateTagDialog
          parentId={active.parentId}
          onStage={stageCreate}
          onClose={closeActive}
        />
      )}
      {active?.kind === "delete" && (
        <DeleteTagDialog
          tag={active.tag}
          onConfirm={(strategy) => {
            stageDelete(active.tag.id, strategy)
            closeActive()
          }}
          onClose={closeActive}
        />
      )}
      {active?.kind === "batch-delete" && (
        <BatchDeleteTagDialog
          count={active.count}
          onConfirm={() => {
            for (const tag of active.tags) stageDelete(tag.id, "CASCADE")
            closeActive()
          }}
          onClose={closeActive}
        />
      )}
    </>
  )
}
