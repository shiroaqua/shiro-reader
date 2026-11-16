import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle, Pencil, Plus, Trash2, Undo2 } from "lucide-react"
import { TagColorName, tagNameColor } from "@/components/tag-color-picker"
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
import { filterTagTree } from "@/lib/tag-filter"
import type { Tag, TagTreeNode, UndefineTagStrategy } from "@/lib/types"

type ActiveDialog =
  | { kind: "create"; parentId: string | null }
  | { kind: "delete"; tag: TagTreeNode }
  | { kind: "batch-delete"; tags: TagTreeNode[]; count: number }

type PendingDelete = { strategy: UndefineTagStrategy }

type PendingCreate = Omit<Tag, "id"> & { draftId: string }

type TagChanges = {
  creates: PendingCreate[]
  moves: [string, string | null][]
  renames: [string, string][]
  recolors: [string, string][]
  deletes: [string, UndefineTagStrategy][]
}

type CommitProgress = {
  createdIds: Map<string, string>
  movedIds: string[]
  renamedIds: string[]
  recoloredIds: string[]
  deletedIds: string[]
}

class TagManageCommitError extends Error {
  readonly progress: CommitProgress

  constructor(originalError: unknown, progress: CommitProgress) {
    super("Failed to commit tag changes", { cause: originalError })
    this.progress = progress
  }
}

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
    mutationFn: async (changes: TagChanges) => {
      const progress: CommitProgress = {
        createdIds: new Map(),
        movedIds: [],
        renamedIds: [],
        recoloredIds: [],
        deletedIds: [],
      }
      try {
        for (const create of changes.creates) {
          const { draftId, ...definition } = create
          const parentId = definition.parent_id
            ? (progress.createdIds.get(definition.parent_id) ??
              definition.parent_id)
            : null
          const savedTag = await library.tag.define({
            ...definition,
            parent_id: parentId,
          })
          progress.createdIds.set(draftId, savedTag.id)
        }
        for (const [id, parentId] of changes.moves) {
          await library.tag.move(
            id,
            parentId ? (progress.createdIds.get(parentId) ?? parentId) : null
          )
          progress.movedIds.push(id)
        }
        for (const [id, strategy] of changes.deletes) {
          await library.tag.undefine(id, strategy)
          progress.deletedIds.push(id)
        }
        for (const [id, name] of changes.renames) {
          await library.tag.rename(id, name)
          progress.renamedIds.push(id)
        }
        for (const [id, color] of changes.recolors) {
          await library.tag.recolor(id, color)
          progress.recoloredIds.push(id)
        }
      } catch (error) {
        throw new TagManageCommitError(error, progress)
      }
    },
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

  const [isDragging, setIsDragging] = useState(false)
  const dragCleanup = useRef<(() => void) | null>(null)
  useEffect(() => () => dragCleanup.current?.(), [])

  const startDrag = (
    node: TagTreeNode,
    event: ReactPointerEvent<HTMLDivElement>
  ) => {
    if (
      event.button !== 0 ||
      event.pointerType === "touch" ||
      saveMutation.isPending ||
      pendingDeletes.has(node.id) ||
      cascadeDeletedIds.has(node.id) ||
      (event.target instanceof Element &&
        event.target.closest("button, input, a, [role='checkbox']"))
    ) return

    event.preventDefault()
    dragCleanup.current?.()
    const source = event.currentTarget
    source.setPointerCapture(event.pointerId)
    const preview = document.createElement("div")
    const checkbox = source.querySelector("[role='checkbox']")?.cloneNode(true)
    const name = document.createElement("span")
    name.textContent = pendingRenames.get(node.id) ?? node.name
    name.className = "min-w-0 truncate text-sm"
    const nameColor = tagNameColor(pendingRecolors.get(node.id) ?? node.color)
    if (nameColor) name.style.color = nameColor
    name.style.fontWeight = "500"
    if (checkbox) preview.append(checkbox)
    preview.append(name)
    const originalOpacity = source.style.opacity
    Object.assign(preview.style, {
      position: "fixed",
      left: `${event.clientX}px`,
      top: `${event.clientY}px`,
      display: "flex",
      alignItems: "center",
      gap: "0.5rem",
      width: "max-content",
      maxWidth: "min(15rem, calc(100vw - 2rem))",
      minHeight: "2.25rem",
      padding: "0.375rem 0.75rem",
      transform: "translate(-50%, -50%) scale(0.88)",
      transformOrigin: "center",
      opacity: "0.7",
      pointerEvents: "none",
      zIndex: "100",
      backgroundColor: "var(--popover)",
      color: "var(--popover-foreground)",
      border: "1px solid var(--border)",
      borderRadius: "0.375rem",
      boxShadow: "0 8px 24px rgb(0 0 0 / 0.15)",
    })
    preview.setAttribute("aria-hidden", "true")
    preview.inert = true
    document.body.append(preview)
    source.style.opacity = "0.25"
    setIsDragging(true)

    const descendants = new Set(collectSubtreeIds(node))
    const dropTargetAt = (x: number, y: number) => {
      const hit = document.elementFromPoint(x, y)
      const treeRoot = hit?.closest<HTMLElement>("[data-tag-tree-root]")
      if (!treeRoot) return null
      const row = hit?.closest<HTMLElement>("[data-tag-id]")
      const parentId = row?.dataset.tagId ?? null
      if (
        parentId === node.parent_id ||
        (parentId &&
          (descendants.has(parentId) ||
            pendingDeletes.has(parentId) ||
            cascadeDeletedIds.has(parentId)))
      ) return null
      return { element: row ?? treeRoot, parentId }
    }
    let highlighted: HTMLElement | null = null
    const highlight = (target: HTMLElement | null) => {
      if (target === highlighted) return
      highlighted?.classList.remove("ring-2", "ring-primary", "bg-primary/10")
      target?.classList.add("ring-2", "ring-primary", "bg-primary/10")
      highlighted = target
    }

    const cleanup = () => {
      highlight(null)
      source.style.opacity = originalOpacity
      preview.remove()
      window.removeEventListener("pointermove", move)
      window.removeEventListener("pointerup", drop)
      window.removeEventListener("pointercancel", cancel)
      window.removeEventListener("blur", cancel)
      window.removeEventListener("keydown", onKeyDown)
      dragCleanup.current = null
      setIsDragging(false)
    }
    const move = (pointer: PointerEvent) => {
      if (pointer.pointerId !== event.pointerId) return
      preview.style.left = `${pointer.clientX}px`
      preview.style.top = `${pointer.clientY}px`
      highlight(dropTargetAt(pointer.clientX, pointer.clientY)?.element ?? null)
    }
    const drop = (pointer: PointerEvent) => {
      if (pointer.pointerId !== event.pointerId) return
      const target = dropTargetAt(pointer.clientX, pointer.clientY)
      cleanup()
      if (target) stageMove(node.id, target.parentId)
    }
    const cancel = () => cleanup()
    const onKeyDown = (key: KeyboardEvent) => {
      if (key.key === "Escape") cleanup()
    }
    dragCleanup.current = cleanup
    window.addEventListener("pointermove", move)
    window.addEventListener("pointerup", drop)
    window.addEventListener("pointercancel", cancel)
    window.addEventListener("blur", cancel)
    window.addEventListener("keydown", onKeyDown)
  }

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

function ManageTreeRow({
  node,
  depth,
  expanded,
  selected,
  pendingRename,
  pendingRecolor,
  pendingMove,
  pendingDelete,
  pendingCreate,
  coveredByCascade,
  disabled,
  dragging,
  onToggleExpand,
  onToggleSelect,
  onStageRename,
  onStageRecolor,
  onPointerDown,
  onRestoreDelete,
  onAddChild,
  onDelete,
  renderChild,
}: {
  node: TagTreeNode
  depth: number
  expanded: ReadonlySet<string>
  selected: ReadonlySet<string>
  pendingRename: string | undefined
  pendingRecolor: string | undefined
  pendingMove: boolean
  pendingDelete: boolean
  pendingCreate: boolean
  coveredByCascade: boolean
  disabled: boolean
  dragging: boolean
  onToggleExpand: (id: string) => void
  onToggleSelect: (node: TagTreeNode, checked: boolean) => void
  onStageRename: (id: string, name: string) => void
  onStageRecolor: (id: string, color: string) => void
  onPointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void
  onRestoreDelete: (id: string) => void
  onAddChild: (node: TagTreeNode) => void
  onDelete: (node: TagTreeNode) => void
  renderChild: (child: TagTreeNode) => ReactNode
}) {
  const { t } = useTranslation()
  const [renaming, setRenaming] = useState(false)
  const checked = selected.has(node.id)
  const isPendingDeletion = pendingDelete || coveredByCascade

  return (
    <TagTreeItem
      node={node}
      depth={depth}
      expanded={expanded}
      disabled={disabled || coveredByCascade}
      selected={checked}
      onToggleExpand={onToggleExpand}
      onPointerDown={onPointerDown}
      renderChild={renderChild}
    >
      {renaming ? (
        <RenameTagInput
          initialName={pendingRename ?? node.name}
          disabled={disabled}
          onStage={(name) => onStageRename(node.id, name)}
          onDone={() => setRenaming(false)}
        />
      ) : (
        <>
          <div className="flex min-w-0 flex-1 cursor-grab items-center gap-2 py-1.5 text-sm">
            <Checkbox
              className="relative before:absolute before:-inset-1 before:content-['']"
              checked={checked}
              disabled={disabled || coveredByCascade}
              aria-label={t("tag.select").replace("{name}", node.name)}
              onCheckedChange={(next) => onToggleSelect(node, next === true)}
            />
            <TagColorName
              name={pendingRename ?? node.name}
              color={pendingRecolor ?? node.color}
              disabled={disabled || isPendingDeletion}
              label={t("tag.color.change").replace(
                "{name}",
                pendingRename ?? node.name
              )}
              className={isPendingDeletion ? "line-through opacity-60" : ""}
              onSelect={(color) => onStageRecolor(node.id, color)}
            />
            {pendingCreate && !isPendingDeletion && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {t("tag.pending_create")}
              </span>
            )}
            {pendingRename !== undefined &&
              !isPendingDeletion &&
              !pendingCreate && (
                <span className="shrink-0 text-xs text-muted-foreground">
                  {t("tag.pending")}
                </span>
              )}
            {pendingRecolor !== undefined &&
              !isPendingDeletion &&
              !pendingCreate &&
              pendingRename === undefined && (
                <span className="shrink-0 text-xs text-muted-foreground">
                  {t("tag.pending")}
                </span>
              )}
            {pendingMove && !isPendingDeletion && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {t("tag.pending_move")}
              </span>
            )}
            {isPendingDeletion && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {t("tag.pending_delete")}
              </span>
            )}
          </div>
          <div
            className={`flex shrink-0 items-center gap-0.5 ${dragging ? "invisible" : ""}`}
          >
            {pendingDelete ? (
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={t("tag.undo_pending_delete")}
                title={t("tag.undo_pending_delete")}
                disabled={disabled}
                onClick={() => onRestoreDelete(node.id)}
              >
                <Undo2 aria-hidden="true" />
              </Button>
            ) : coveredByCascade ? null : (
              <>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("tag.new_child")}
                  title={t("tag.new_child")}
                  disabled={disabled}
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
                  disabled={disabled}
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
                  disabled={disabled}
                  onClick={() => onDelete(node)}
                >
                  <Trash2 aria-hidden="true" />
                </Button>
              </>
            )}
          </div>
        </>
      )}
    </TagTreeItem>
  )
}

function RenameTagInput({
  initialName,
  disabled,
  onStage,
  onDone,
}: {
  initialName: string
  disabled: boolean
  onStage: (name: string) => void
  onDone: () => void
}) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState(initialName)
  const finished = useRef(false)

  // Enter or leaving the field stages the name; Done sends it to the server.
  const finish = (shouldStage: boolean) => {
    if (finished.current || disabled) return
    finished.current = true
    const name = draft.trim()
    if (shouldStage && name !== "" && name !== initialName) onStage(name)
    onDone()
  }

  return (
    <Input
      autoFocus
      value={draft}
      aria-label={t("tag.rename")}
      aria-busy={disabled}
      disabled={disabled}
      className="h-7 min-w-0 flex-1 text-sm"
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return
        if (event.key === "Enter") {
          event.preventDefault()
          finish(true)
        } else if (event.key === "Escape") {
          // Only leave edit mode; the dialog itself stays open.
          event.preventDefault()
          event.stopPropagation()
          finish(false)
        }
      }}
      onBlur={() => finish(true)}
    />
  )
}

function DeleteTagDialog({
  tag,
  onConfirm,
  onClose,
}: {
  tag: TagTreeNode
  onConfirm: (strategy: UndefineTagStrategy) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const hasChildren = tag.children.length > 0
  // Promoting children is the non-destructive default.
  const [strategy, setStrategy] = useState<UndefineTagStrategy>("PROMOTE")
  const options: { value: UndefineTagStrategy; label: string; hint: string }[] =
    [
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

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
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
          <AlertDialogClose render={<Button variant="outline" />}>
            {t("common.cancel")}
          </AlertDialogClose>
          <Button variant="destructive" onClick={() => onConfirm(strategy)}>
            {t("tag.delete")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function BatchDeleteTagDialog({
  count,
  onConfirm,
  onClose,
}: {
  count: number
  onConfirm: () => void
  onClose: () => void
}) {
  const { t } = useTranslation()

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {t("tag.batch_delete_confirm_title")}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t("tag.batch_delete_confirm_description").replace(
              "{count}",
              String(count)
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogClose render={<Button variant="outline" />}>
            {t("common.cancel")}
          </AlertDialogClose>
          <Button variant="destructive" onClick={onConfirm}>
            {t("tag.delete")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function collectSelectedRoots(
  nodes: TagTreeNode[],
  selected: ReadonlySet<string>
): TagTreeNode[] {
  const roots: TagTreeNode[] = []
  const visit = (items: TagTreeNode[], hasSelectedAncestor: boolean) => {
    for (const node of items) {
      const isSelected = selected.has(node.id)
      if (isSelected && !hasSelectedAncestor) roots.push(node)
      visit(node.children, hasSelectedAncestor || isSelected)
    }
  }
  visit(nodes, false)
  return roots
}

function collectTagMetadata(nodes: TagTreeNode[]) {
  const metadata = new Map<string, { depth: number; ancestors: string[] }>()
  const visit = (items: TagTreeNode[], ancestors: string[]) => {
    for (const node of items) {
      metadata.set(node.id, { depth: ancestors.length, ancestors })
      visit(node.children, [...ancestors, node.id])
    }
  }
  visit(nodes, [])
  return metadata
}

function addPendingCreates(
  serverTree: TagTreeNode[],
  pendingCreates: PendingCreate[]
): TagTreeNode[] {
  const byId = new Map<string, TagTreeNode>()
  const cloneTree = (nodes: TagTreeNode[]): TagTreeNode[] =>
    nodes.map((node) => {
      const clone: TagTreeNode = { ...node, children: [] }
      byId.set(clone.id, clone)
      clone.children = cloneTree(node.children)
      return clone
    })
  const tree = cloneTree(serverTree)

  for (const create of pendingCreates) {
    const node: TagTreeNode = {
      id: create.draftId,
      name: create.name,
      parent_id: create.parent_id,
      color: create.color,
      allows_value: create.allows_value,
      value_regex: create.value_regex,
      children: [],
    }
    const parent = create.parent_id ? byId.get(create.parent_id) : undefined
    if (parent) parent.children.push(node)
    else tree.push(node)
    byId.set(node.id, node)
  }

  return tree
}

function applyPendingMoves(
  tree: TagTreeNode[],
  moves: ReadonlyMap<string, string | null>
): TagTreeNode[] {
  const detach = (nodes: TagTreeNode[], id: string): TagTreeNode | undefined => {
    const index = nodes.findIndex((node) => node.id === id)
    if (index !== -1) return nodes.splice(index, 1)[0]
    for (const node of nodes) {
      const found = detach(node.children, id)
      if (found) return found
    }
  }

  for (const [id, parentId] of moves) {
    const node = findTag(tree, id)
    const parent = parentId ? findTag(tree, parentId) : null
    if (!node || (parentId && !parent)) continue
    if (parentId && collectSubtreeIds(node).includes(parentId)) continue
    detach(tree, id)
    node.parent_id = parentId
    if (parent) parent.children.push(node)
    else tree.push(node)
  }
  return tree
}

function collectCascadeDeletedIds(
  tree: TagTreeNode[],
  pendingDeletes: ReadonlyMap<string, PendingDelete>
): Set<string> {
  const deletedIds = new Set<string>()
  for (const [id, deletion] of pendingDeletes) {
    if (deletion.strategy !== "CASCADE") continue
    const tag = findTag(tree, id)
    if (tag) {
      for (const descendantId of collectSubtreeIds(tag)) {
        deletedIds.add(descendantId)
      }
    }
  }
  return deletedIds
}

function resolveParentAfterPartialCommit(
  initialParentId: string | null,
  completedDeleteIds: ReadonlySet<string>,
  pendingDeletes: ReadonlyMap<string, PendingDelete>,
  pendingCreates: PendingCreate[],
  tree: TagTreeNode[]
): string | null {
  let parentId = initialParentId
  const visited = new Set<string>()
  while (parentId && completedDeleteIds.has(parentId)) {
    if (visited.has(parentId)) return null
    visited.add(parentId)
    if (pendingDeletes.get(parentId)?.strategy !== "PROMOTE") return null
    const pendingParent = pendingCreates.find(
      (create) => create.draftId === parentId
    )
    parentId = pendingParent
      ? pendingParent.parent_id
      : (findTag(tree, parentId)?.parent_id ?? null)
  }
  return parentId
}

function prepareChanges(
  serverTree: TagTreeNode[],
  tree: TagTreeNode[],
  metadata: Map<string, { depth: number; ancestors: string[] }>,
  pendingCreates: PendingCreate[],
  pendingMoves: ReadonlyMap<string, string | null>,
  pendingRenames: ReadonlyMap<string, string>,
  pendingRecolors: ReadonlyMap<string, string>,
  pendingDeletes: ReadonlyMap<string, PendingDelete>
): TagChanges {
  const cascadeDeletedIds = collectCascadeDeletedIds(tree, pendingDeletes)
  const pendingCreatesById = new Map(
    pendingCreates.map((create) => [create.draftId, create])
  )

  const resolveParentId = (initialParentId: string | null) => {
    let parentId = initialParentId
    const visited = new Set<string>()
    while (parentId && pendingDeletes.has(parentId)) {
      if (visited.has(parentId)) return null
      visited.add(parentId)
      if (pendingDeletes.get(parentId)?.strategy === "CASCADE") return null
      const pendingParent = pendingCreatesById.get(parentId)
      parentId = pendingParent
        ? pendingParent.parent_id
        : (findTag(tree, parentId)?.parent_id ?? null)
    }
    return parentId
  }

  const creates = pendingCreates
    .filter(
      (create) =>
        !pendingDeletes.has(create.draftId) &&
        !cascadeDeletedIds.has(create.draftId)
    )
    .map((create) => ({
      ...create,
      parent_id: resolveParentId(create.parent_id),
    }))

  const renames = [...pendingRenames].filter(
    ([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id)
  )
  const recolors = [...pendingRecolors].filter(
    ([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id)
  )
  const moves = [...pendingMoves]
    .filter(([id]) => !pendingDeletes.has(id) && !cascadeDeletedIds.has(id))
    .map(([id, parentId]) =>
      [id, resolveParentId(parentId)] as [string, string | null]
    )
    .filter(([id, parentId]) => findTag(serverTree, id)?.parent_id !== parentId)
  const pendingCreateIds = new Set(
    pendingCreates.map((create) => create.draftId)
  )
  const deletes = [...pendingDeletes]
    .filter(
      ([id]) =>
        !pendingCreateIds.has(id) &&
        !(metadata.get(id)?.ancestors ?? []).some(
          (ancestorId) => pendingDeletes.get(ancestorId)?.strategy === "CASCADE"
        )
    )
    .map(
      ([id, deletion]) =>
        [id, deletion.strategy] as [string, UndefineTagStrategy]
    )
    .sort(
      ([leftId], [rightId]) =>
        (metadata.get(rightId)?.depth ?? 0) - (metadata.get(leftId)?.depth ?? 0)
    )

  return { creates, moves, renames, recolors, deletes }
}
