import {
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react"
import { Pencil, Plus, Trash2, Undo2 } from "lucide-react"
import { TagColorName } from "@/features/tags/tag-color-picker"
import { TagTreeItem } from "@/features/tags/tag-dialog-parts"
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
import { Input } from "@/components/ui/input"
import { notifyError } from "@/lib/error-notifications"
import { translateTagName, useTranslation } from "@/lib/i18n"
import { isValidTagName } from "@/lib/tag-value-patterns"
import type { TagTreeNode, UndefineTagStrategy } from "@/lib/types"
import { cn } from "cn"

export function ManageTreeRow({
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
  const { locale, t } = useTranslation()
  const [renaming, setRenaming] = useState(false)
  const checked = selected.has(node.id)
  const isPendingDeletion = pendingDelete || coveredByCascade
  const displayName = translateTagName(pendingRename ?? node.name, locale)

  return (
    <TagTreeItem
      node={node}
      depth={depth}
      expanded={expanded}
      disabled={disabled}
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
          <div
            className={cn(
              "flex min-w-0 flex-1 items-center gap-2 py-1.5 text-sm",
              !disabled && !isPendingDeletion && "cursor-grab"
            )}
          >
            <Checkbox
              className="relative before:absolute before:-inset-1 before:content-['']"
              checked={checked}
              disabled={disabled || coveredByCascade}
              aria-label={t("local.tag.select").replace(
                "{name}",
                displayName
              )}
              onCheckedChange={(next) => onToggleSelect(node, next === true)}
            />
            <TagColorName
              name={pendingRename ?? node.name}
              color={pendingRecolor ?? node.color}
              disabled={disabled || isPendingDeletion}
              label={t("local.tag.color.change").replace(
                "{name}",
                displayName
              )}
              className={isPendingDeletion ? "line-through opacity-60" : ""}
              onSelect={(color) => onStageRecolor(node.id, color)}
            />
            {pendingCreate && !isPendingDeletion && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {t("local.tag.pending_create")}
              </span>
            )}
            {pendingRename !== undefined &&
              !isPendingDeletion &&
              !pendingCreate && (
                <span className="shrink-0 text-xs text-muted-foreground">
                  {t("local.tag.pending")}
                </span>
              )}
            {pendingRecolor !== undefined &&
              !isPendingDeletion &&
              !pendingCreate &&
              pendingRename === undefined && (
                <span className="shrink-0 text-xs text-muted-foreground">
                  {t("local.tag.pending")}
                </span>
              )}
            {pendingMove && !isPendingDeletion && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {t("local.tag.pending_move")}
              </span>
            )}
            {isPendingDeletion && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {t("local.tag.pending_delete")}
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
                aria-label={t("local.tag.undo_pending_delete")}
                title={t("local.tag.undo_pending_delete")}
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
                  aria-label={t("local.tag.new_child")}
                  title={t("local.tag.new_child")}
                  disabled={disabled}
                  onClick={() => onAddChild(node)}
                >
                  <Plus aria-hidden="true" />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("local.tag.rename")}
                  title={t("local.tag.rename")}
                  disabled={disabled}
                  onClick={() => setRenaming(true)}
                >
                  <Pencil aria-hidden="true" />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("local.tag.delete")}
                  title={t("local.tag.delete")}
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
  const trimmed = draft.trim()
  const isInvalid = trimmed !== "" && !isValidTagName(trimmed)

  // Enter or leaving the field stages the name; Done sends it to the server.
  const finish = (shouldStage: boolean, warnOnInvalid = false) => {
    if (finished.current || disabled) return
    if (shouldStage && isInvalid) {
      if (warnOnInvalid) {
        notifyError({
          source: "server",
          key: "library.tag.invalid_name_format",
        })
        return
      }
      finished.current = true
      onDone()
      return
    }
    finished.current = true
    if (shouldStage && trimmed !== "" && trimmed !== initialName) {
      onStage(trimmed)
    }
    onDone()
  }

  return (
    <Input
      autoFocus
      value={draft}
      aria-label={t("local.tag.rename")}
      aria-invalid={isInvalid || undefined}
      aria-busy={disabled}
      disabled={disabled}
      className="h-7 min-w-0 flex-1 text-sm"
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return
        if (event.key === "Enter") {
          event.preventDefault()
          finish(true, true)
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

export function DeleteTagDialog({
  tag,
  displayName,
  onConfirm,
  onClose,
}: {
  tag: TagTreeNode
  displayName?: string
  onConfirm: (strategy: UndefineTagStrategy) => void
  onClose: () => void
}) {
  const { locale, t } = useTranslation()
  const resolvedName = displayName ?? translateTagName(tag.name, locale)
  const hasChildren = tag.children.length > 0
  // Promoting children is the non-destructive default.
  const [strategy, setStrategy] = useState<UndefineTagStrategy>("PROMOTE")
  const options: { value: UndefineTagStrategy; label: string; hint: string }[] =
    [
      {
        value: "PROMOTE",
        label: t("local.tag.delete_strategy.promote"),
        hint: t("local.tag.delete_strategy.promote_hint"),
      },
      {
        value: "CASCADE",
        label: t("local.tag.delete_strategy.cascade"),
        hint: t("local.tag.delete_strategy.cascade_hint"),
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
          <AlertDialogTitle>
            {t("local.tag.delete_confirm_title")}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t("local.tag.delete_confirm_description").replace(
              "{name}",
              resolvedName
            )}
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
            {t("local.common.cancel")}
          </AlertDialogClose>
          <Button variant="destructive" onClick={() => onConfirm(strategy)}>
            {t("local.tag.delete")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

export function BatchDeleteTagDialog({
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
            {t("local.tag.batch_delete_confirm_title")}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t("local.tag.batch_delete_confirm_description").replace(
              "{count}",
              String(count)
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogClose render={<Button variant="outline" />}>
            {t("local.common.cancel")}
          </AlertDialogClose>
          <Button variant="destructive" onClick={onConfirm}>
            {t("local.tag.delete")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
