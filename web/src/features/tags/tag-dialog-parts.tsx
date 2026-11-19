import {
  useMemo,
  useState,
  type PointerEventHandler,
  type ReactNode,
} from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  ChevronRight,
  Hash,
  LoaderCircle,
  Plus,
  Search,
  Tags,
} from "lucide-react"
import { library } from "@/api/library"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { NativeSelect } from "@/components/ui/native-select"
import { translateServerMessage, useTranslation } from "@/lib/i18n"
import { tagsQueryKey } from "@/lib/query-keys"
import { collectBranchIds } from "@/lib/tag-tree"
import { filterTagTree } from "@/lib/tag-filter"
import {
  VALUE_TYPES,
  isValidTagName,
  toValueRule,
  valueTypeLabelKey,
  type ValueType,
} from "@/lib/tag-value-patterns"
import type { Tag, TagTreeNode } from "@/lib/types"
import { cn } from "cn"
import {
  DEFAULT_TAG_COLOR,
  TagColorPopover,
} from "@/features/tags/tag-color-picker"

/** The dialog grows with the tag list up to a cap; past that the list, not the dialog, scrolls. */
export const TAG_DIALOG_CONTENT_CLASS_NAME =
  "max-h-[min(32rem,calc(100dvh-2rem))] sm:max-w-lg"

/** Search and expand state over a visible tag tree. */
export function useTagTreeExpansion(visibleTree: TagTreeNode[]) {
  const branchIds = useMemo(() => collectBranchIds(visibleTree), [visibleTree])
  const [expandedIds, setExpanded] = useState<ReadonlySet<string> | null>(null)
  const expanded = useMemo(
    () => expandedIds ?? new Set(branchIds),
    [expandedIds, branchIds]
  )

  const resetExpanded = () => setExpanded(null)

  const toggleExpand = (id: string) =>
    setExpanded((current) => {
      const next = new Set(current ?? branchIds)
      if (!next.delete(id)) next.add(id)
      return next
    })

  const expand = (id: string) =>
    setExpanded((current) => new Set([...(current ?? branchIds), id]))

  return { expanded, toggleExpand, expand, resetExpanded }
}

/** Tag tree query plus the search and expand state shared by the bind and manage dialogs. */
export function useTagTreeView() {
  const { locale } = useTranslation()
  const tagsQuery = useQuery({
    queryKey: tagsQueryKey,
    queryFn: () => library.tag.list(),
  })
  const tree = useMemo(() => tagsQuery.data ?? [], [tagsQuery.data])
  const [query, setQueryState] = useState("")
  const visibleTree = useMemo(
    () => filterTagTree(tree, query, locale),
    [tree, query, locale]
  )
  const { expanded, toggleExpand, expand, resetExpanded } =
    useTagTreeExpansion(visibleTree)

  const setQuery = (next: string) => {
    setQueryState(next)
    // Re-expand every branch that still matches the new filter.
    resetExpanded()
  }

  return {
    tagsQuery,
    tree,
    query,
    setQuery,
    visibleTree,
    expanded,
    toggleExpand,
    expand,
  }
}

/** Three-column header: action on the left, centered title, action on the right. */
export function TagDialogHeader({
  title,
  start,
  end,
}: {
  title: string
  start?: ReactNode
  end?: ReactNode
}) {
  return (
    <DialogHeader className="grid-cols-[1fr_auto_1fr] items-center">
      <div className="justify-self-start">{start}</div>
      <DialogTitle className="text-center">{title}</DialogTitle>
      <div className="justify-self-end">{end}</div>
    </DialogHeader>
  )
}

/** Search field with the "define tag" button on its right. */
export function TagSearchBar({
  query,
  disabled,
  onQueryChange,
  onDefine,
}: {
  query: string
  disabled: boolean
  onQueryChange: (query: string) => void
  onDefine: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-2">
      <div className="relative min-w-0 flex-1">
        <Search
          aria-hidden="true"
          className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          type="search"
          value={query}
          disabled={disabled}
          aria-label={t("local.tag.filter")}
          placeholder={t("local.tag.filter")}
          className="pl-8"
          onChange={(event) => onQueryChange(event.target.value)}
        />
      </div>
      <Button
        type="button"
        variant="outline"
        disabled={disabled}
        className="shrink-0"
        onClick={onDefine}
      >
        <Plus aria-hidden="true" />
        {t("local.tag.new")}
      </Button>
    </div>
  )
}

/**
 * The box that shows the tag tree, with loading, error, empty and no-match states.
 * `renderRow` draws one top-level node; rows render their own children via `TagTreeItem`.
 */
export function TagTreeBox({
  tree,
  visibleTree,
  isLoading,
  isError,
  errorMessage,
  isRetrying,
  onRetry,
  multiselectable,
  renderRow,
}: {
  tree: TagTreeNode[]
  visibleTree: TagTreeNode[]
  isLoading: boolean
  isError: boolean
  errorMessage: string
  isRetrying: boolean
  onRetry: () => void
  multiselectable: boolean
  renderRow: (node: TagTreeNode) => ReactNode
}) {
  const { t } = useTranslation()
  const boxClassName =
    "min-h-40 overflow-y-auto rounded-lg border bg-background p-1"

  if (isLoading)
    return (
      <div
        role="status"
        aria-busy="true"
        className={cn(
          boxClassName,
          "flex items-center justify-center gap-2 text-sm text-muted-foreground"
        )}
      >
        <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
        {t("local.tag.loading")}
      </div>
    )

  if (isError)
    return (
      <div
        role="status"
        className={cn(
          boxClassName,
          "flex flex-col items-center justify-center gap-2 text-sm text-muted-foreground"
        )}
      >
        <p>{errorMessage}</p>
        <Button
          variant="outline"
          size="sm"
          disabled={isRetrying}
          onClick={onRetry}
        >
          {t("local.common.retry")}
        </Button>
      </div>
    )

  if (tree.length === 0)
    return (
      <div
        role="status"
        className={cn(
          boxClassName,
          "flex flex-col items-center justify-center gap-2 border-dashed text-center text-muted-foreground"
        )}
      >
        <Tags aria-hidden="true" className="size-10" />
        <p className="text-sm font-medium text-foreground">
          {t("local.tag.empty")}
        </p>
        <p className="text-sm">{t("local.tag.empty_hint")}</p>
      </div>
    )

  return (
    <div className={boxClassName} data-tag-tree-root>
      {visibleTree.length === 0 && (
        <p
          role="status"
          className="py-6 text-center text-sm text-muted-foreground"
        >
          {t("local.tag.no_match")}
        </p>
      )}
      <ul
        role="tree"
        aria-multiselectable={multiselectable || undefined}
        aria-label={t("local.tag.list")}
      >
        {visibleTree.map(renderRow)}
      </ul>
    </div>
  )
}

/** One tree item: indent, expand toggle, the row content, then its children when expanded. */
export function TagTreeItem({
  node,
  depth,
  expanded,
  disabled,
  selected,
  onToggleExpand,
  onPointerDown,
  children,
  renderChild,
}: {
  node: TagTreeNode
  depth: number
  expanded: ReadonlySet<string>
  disabled: boolean
  selected?: boolean
  onToggleExpand: (id: string) => void
  onPointerDown?: PointerEventHandler<HTMLDivElement>
  children: ReactNode
  renderChild: (child: TagTreeNode) => ReactNode
}) {
  const { t } = useTranslation()
  const hasChildren = node.children.length > 0
  const isOpen = hasChildren && expanded.has(node.id)

  return (
    <li
      role="treeitem"
      aria-selected={selected}
      aria-expanded={hasChildren ? isOpen : undefined}
    >
      <div
        data-tag-id={node.id}
        onPointerDown={onPointerDown}
        style={{ paddingLeft: `${depth * 1.25 + 0.25}rem` }}
        className="group/row flex min-h-9 items-center gap-1 rounded-md pr-1 transition-colors focus-within:bg-muted/60 hover:bg-muted/60"
      >
        {hasChildren ? (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t(isOpen ? "local.tag.collapse" : "local.tag.expand")}
            disabled={disabled}
            onClick={() => onToggleExpand(node.id)}
          >
            <ChevronRight
              aria-hidden="true"
              className={cn("transition-transform", isOpen && "rotate-90")}
            />
          </Button>
        ) : (
          <span aria-hidden="true" className="size-6 shrink-0" />
        )}
        {children}
      </div>
      {isOpen && <ul role="group">{node.children.map(renderChild)}</ul>}
    </li>
  )
}

export function CreateTagDialog({
  parentId,
  onClose,
  onStage,
}: {
  parentId: string | null
  onClose: () => void
  onStage?: (definition: Omit<Tag, "id">) => void
}) {
  const { locale, t } = useTranslation()
  const [name, setName] = useState("")
  const [valueType, setValueType] = useState<ValueType>("none")
  const [color, setColor] = useState(DEFAULT_TAG_COLOR)
  const trimmedName = name.trim()
  const isNameInvalid = trimmedName !== "" && !isValidTagName(trimmedName)
  const queryClient = useQueryClient()
  const createMutation = useMutation({
    mutationFn: () =>
      library.tag.define({
        name: trimmedName,
        parent_id: parentId,
        color,
        ...toValueRule(valueType),
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: tagsQueryKey })
      onClose()
    },
  })
  const canSubmit =
    trimmedName !== "" && !isNameInvalid && !createMutation.isPending

  const submit = () => {
    if (!canSubmit) return
    const definition = {
      name: trimmedName,
      parent_id: parentId,
      color,
      ...toValueRule(valueType),
    }
    if (onStage) {
      onStage(definition)
      onClose()
      return
    }
    createMutation.mutate()
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !createMutation.isPending) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t(parentId ? "local.tag.new_child" : "local.tag.new")}
          </DialogTitle>
        </DialogHeader>
        <div className="grid gap-4">
          <label className="grid gap-1.5 text-sm font-medium">
            {t("local.tag.name")}
            <Input
              autoFocus
              value={name}
              disabled={createMutation.isPending}
              aria-invalid={isNameInvalid || undefined}
              placeholder={t("local.tag.name_placeholder")}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.nativeEvent.isComposing) return
                if (event.key === "Enter") {
                  event.preventDefault()
                  submit()
                }
              }}
            />
            {isNameInvalid && (
              <span className="text-xs font-normal text-destructive">
                {translateServerMessage(
                  "library.tag.invalid_name_format",
                  locale
                )}
              </span>
            )}
          </label>
          <div className="grid gap-1.5 text-sm font-medium">
            <span>{t("local.tag.color")}</span>
            <TagColorPopover
              color={color}
              disabled={createMutation.isPending}
              onSelect={setColor}
              trigger={
                <button
                  type="button"
                  aria-label={t("local.tag.color.choose")}
                  disabled={createMutation.isPending}
                  className="flex h-8 items-center gap-2 rounded-lg border border-input px-2.5 text-sm font-normal transition-colors outline-none hover:bg-muted/60 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50 dark:bg-input/30"
                >
                  <Hash
                    aria-hidden="true"
                    className="size-4"
                    style={{ color }}
                  />
                  <span className="font-mono text-xs tracking-wide text-muted-foreground uppercase">
                    {color}
                  </span>
                </button>
              }
            />
          </div>
          <div className="grid gap-1.5 text-sm font-medium">
            <span>{t("local.tag.value_type")}</span>
            <NativeSelect
              value={valueType}
              aria-label={t("local.tag.value_type")}
              disabled={createMutation.isPending}
              onChange={(event) =>
                setValueType(event.target.value as ValueType)
              }
            >
              {VALUE_TYPES.map((type) => (
                <option key={type} value={type}>
                  {t(valueTypeLabelKey(type))}
                </option>
              ))}
            </NativeSelect>
            <span className="text-xs font-normal text-muted-foreground">
              {t("local.tag.allows_value_hint")}
            </span>
          </div>
        </div>
        <DialogFooter>
          <DialogClose
            disabled={createMutation.isPending}
            render={<Button variant="outline" />}
          >
            {t("local.common.cancel")}
          </DialogClose>
          <Button
            disabled={!canSubmit}
            aria-busy={createMutation.isPending}
            onClick={submit}
          >
            {createMutation.isPending && (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            )}
            {t("local.tag.create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
