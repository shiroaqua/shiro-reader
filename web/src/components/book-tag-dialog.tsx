import { useMemo, useState, type ReactNode } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { LoaderCircle, Plus, Settings2 } from "lucide-react"
import { library } from "@/api/library"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogClose, DialogContent } from "@/components/ui/dialog"
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
import { TagManageDialog } from "@/components/tag-manage-dialog"
import { TagValueInput } from "@/components/tag-value-input"
import { useTranslation } from "@/lib/i18n"
import { bookTagsQueryKey } from "@/lib/query-keys"
import { collectSubtreeIds, findTag } from "@/lib/tag-tree"
import { isValueValid } from "@/lib/tag-value-patterns"
import type { Book, BookTag, TagTreeNode } from "@/lib/types"

/** Picks which tags are bound to a book. Changes are saved only when "Done" is pressed. */
export function BookTagDialog({
  book,
  open,
  onOpenChange,
}: {
  book: Book
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const queryClient = useQueryClient()
  const saveMutation = useMutation({
    mutationFn: async (changes: ReturnType<typeof diffBindings>) => {
      for (const tagId of changes.unbind) {
        await library.bookshelf.book.unbindTag(book.id, tagId)
      }
      for (const binding of changes.bind) {
        await library.bookshelf.book.bindTag(book.id, binding)
      }
    },
    onSettled: async (_data, error) => {
      // Reload even after a failure: earlier requests may already have succeeded.
      await queryClient.invalidateQueries({
        queryKey: bookTagsQueryKey(book.id),
        refetchType: "all",
      })
      if (!error) onOpenChange(false)
    },
  })

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!saveMutation.isPending) onOpenChange(next)
      }}
    >
      <DialogContent className={TAG_DIALOG_CONTENT_CLASS_NAME}>
        {open && (
          <BookTagForm
            key={book.id}
            bookId={book.id}
            isSaving={saveMutation.isPending}
            onSave={(changes) => saveMutation.mutate(changes)}
            onClose={() => onOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function BookTagForm({
  bookId,
  isSaving,
  onSave,
  onClose,
}: {
  bookId: string
  isSaving: boolean
  onSave: (changes: ReturnType<typeof diffBindings>) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const view = useTagTreeView()
  const { tagsQuery, tree, expanded, toggleExpand } = view
  const bindingsQuery = useQuery({
    queryKey: bookTagsQueryKey(bookId),
    queryFn: () => library.bookshelf.book.getTags(bookId),
    staleTime: 0,
  })
  const existingIds = useMemo(
    () => new Set(tree.flatMap(collectSubtreeIds)),
    [tree]
  )
  // Parent of the tag being defined; null parentId defines a top-level tag.
  const [creating, setCreating] = useState<{ parentId: string | null } | null>(
    null
  )
  const [managing, setManaging] = useState(false)
  // tag id -> value (an empty string means the tag is bound without a value)
  const initialSelection = useMemo(
    () =>
      new Map(
        bindingsQuery.data?.map((binding) => [
          binding.tag_id,
          binding.value ?? "",
        ])
      ),
    [bindingsQuery.data]
  )
  const [selectionDraft, setSelected] = useState<ReadonlyMap<
    string,
    string
  > | null>(null)
  // Tags undefined in the manage dialog must not stay selected, or saving would try to bind them.
  const selected = useMemo(
    () =>
      selectionDraft === null
        ? initialSelection
        : new Map([...selectionDraft].filter(([id]) => existingIds.has(id))),
    [selectionDraft, initialSelection, existingIds]
  )
  const changes = useMemo(
    () => diffBindings(bindingsQuery.data ?? [], selected),
    [bindingsQuery.data, selected]
  )
  const hasChanges = changes.bind.length + changes.unbind.length > 0
  const hasInvalidValue = useMemo(
    () =>
      [...selected].some(([id, value]) => {
        const node = findTag(tree, id)
        return node !== undefined && !isValueValid(node, value)
      }),
    [selected, tree]
  )
  const isLoading =
    tagsQuery.isPending ||
    bindingsQuery.isPending ||
    (bindingsQuery.isFetching && selectionDraft === null)
  const isError = tagsQuery.isError || bindingsQuery.isError
  const canSubmit =
    !isSaving &&
    !isLoading &&
    !isError &&
    !tagsQuery.isFetching &&
    !bindingsQuery.isFetching &&
    !hasInvalidValue

  const toggleSelect = (node: TagTreeNode, checked: boolean) =>
    setSelected((current) => {
      const next = new Map(current ?? initialSelection)
      for (const id of collectSubtreeIds(node)) {
        if (checked) next.set(id, next.get(id) ?? "")
        else next.delete(id)
      }
      return next
    })

  const changeValue = (id: string, value: string) =>
    setSelected((current) =>
      new Map(current ?? initialSelection).set(id, value)
    )

  // Nothing changed: "Done" simply closes the dialog.
  const submit = () => {
    if (!canSubmit) return
    if (hasChanges) onSave(changes)
    else onClose()
  }

  const renderRow = (node: TagTreeNode, depth: number): ReactNode => (
    <BindTreeRow
      key={node.id}
      node={node}
      depth={depth}
      expanded={expanded}
      selected={selected}
      disabled={isSaving}
      onToggleExpand={toggleExpand}
      onToggleSelect={toggleSelect}
      onChangeValue={changeValue}
      onAddChild={(parent) => setCreating({ parentId: parent.id })}
      renderChild={(child) => renderRow(child, depth + 1)}
    />
  )

  return (
    <>
      <TagDialogHeader
        title={t("book.tags")}
        start={
          <DialogClose
            disabled={isSaving}
            render={<Button variant="ghost" size="sm" />}
          >
            {t("common.cancel")}
          </DialogClose>
        }
        end={
          <Button
            size="sm"
            disabled={!canSubmit}
            aria-busy={isSaving}
            onClick={submit}
          >
            {isSaving && (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            )}
            {t("common.done")}
          </Button>
        }
      />

      <TagSearchBar
        query={view.query}
        disabled={isSaving}
        onQueryChange={view.setQuery}
        onDefine={() => setCreating({ parentId: null })}
      />

      <TagTreeBox
        tree={tree}
        visibleTree={view.visibleTree}
        isLoading={isLoading}
        isError={isError}
        errorMessage={t("tag.bind_load_failed")}
        isRetrying={tagsQuery.isFetching || bindingsQuery.isFetching}
        onRetry={() => {
          void tagsQuery.refetch()
          void bindingsQuery.refetch()
        }}
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
          disabled={isSaving}
          onClick={() => setManaging(true)}
        >
          <Settings2 aria-hidden="true" />
          {t("tag.manage")}
        </Button>
      </div>

      {creating && (
        <CreateTagDialog
          parentId={creating.parentId}
          onClose={() => setCreating(null)}
        />
      )}
      <TagManageDialog open={managing} onOpenChange={setManaging} />
    </>
  )
}

function BindTreeRow({
  node,
  depth,
  expanded,
  selected,
  disabled,
  onToggleExpand,
  onToggleSelect,
  onChangeValue,
  onAddChild,
  renderChild,
}: {
  node: TagTreeNode
  depth: number
  expanded: ReadonlySet<string>
  selected: ReadonlyMap<string, string>
  disabled: boolean
  onToggleExpand: (id: string) => void
  onToggleSelect: (node: TagTreeNode, checked: boolean) => void
  onChangeValue: (id: string, value: string) => void
  onAddChild: (node: TagTreeNode) => void
  renderChild: (child: TagTreeNode) => ReactNode
}) {
  const { t } = useTranslation()
  const checked = selected.has(node.id)
  const value = selected.get(node.id) ?? ""

  return (
    <TagTreeItem
      node={node}
      depth={depth}
      expanded={expanded}
      disabled={disabled}
      selected={checked}
      onToggleExpand={onToggleExpand}
      renderChild={renderChild}
    >
      <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-1.5 text-sm">
        <Checkbox
          checked={checked}
          disabled={disabled}
          onCheckedChange={(next) => onToggleSelect(node, next === true)}
        />
        <span title={node.name} className="min-w-0 truncate">
          {node.name}
        </span>
      </label>
      {checked && node.allows_value && (
        <TagValueInput
          tag={node}
          value={value}
          disabled={disabled}
          onChange={(next) => onChangeValue(node.id, next)}
        />
      )}
      <div className={ROW_ACTIONS_CLASS_NAME}>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={disabled}
          aria-label={t("tag.new_child")}
          title={t("tag.new_child")}
          onClick={() => onAddChild(node)}
        >
          <Plus aria-hidden="true" />
        </Button>
      </div>
    </TagTreeItem>
  )
}

/** Which tags to bind (new or changed value) and which to unbind, compared with the initial bindings. */
function diffBindings(
  initial: BookTag[],
  selected: ReadonlyMap<string, string>
) {
  const before = new Map<string, string>(
    initial.map((binding): [string, string] => [
      binding.tag_id,
      binding.value ?? "",
    ])
  )
  const bind: BookTag[] = []
  for (const [tagId, value] of selected) {
    if (before.get(tagId) !== value) {
      bind.push({ tag_id: tagId, value: value === "" ? null : value })
    }
  }
  const unbind = [...before.keys()].filter((tagId) => !selected.has(tagId))
  return { bind, unbind }
}
