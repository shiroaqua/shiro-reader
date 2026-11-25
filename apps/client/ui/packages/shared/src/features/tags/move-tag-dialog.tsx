import { Check, Layers, Tag as TagIcon } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@shiro/ui-shared/components/ui/dialog"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import type { TagTreeNode } from "@shiro/ui-shared/lib/types"

type TargetRow = { node: TagTreeNode; depth: number }

function flattenTargets(
  nodes: readonly TagTreeNode[],
  excluded: ReadonlySet<string>,
  depth = 0,
  rows: TargetRow[] = []
): TargetRow[] {
  for (const node of nodes) {
    if (excluded.has(node.id)) continue
    rows.push({ node, depth })
    flattenTargets(node.children, excluded, depth + 1, rows)
  }
  return rows
}

const TARGET_CLASS_NAME =
  "flex h-11 w-full items-center gap-2 rounded-lg px-3 text-left text-sm outline-none hover:bg-muted/60 focus-visible:bg-muted/60 active:bg-accent disabled:pointer-events-none disabled:opacity-60 [&_svg]:size-4 [&_svg]:shrink-0"

/**
 * Touch-friendly alternative to dragging: pick the new parent from a list.
 * `excluded` holds tags that cannot be a target (the tag's own subtree and
 * tags that are being deleted).
 */
export function MoveTagDialog({
  node,
  tree,
  excluded,
  labelFor,
  onSelect,
  onClose,
}: {
  node: TagTreeNode
  tree: readonly TagTreeNode[]
  excluded: ReadonlySet<string>
  labelFor: (tag: TagTreeNode) => string
  onSelect: (parentId: string | null) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const rows = flattenTargets(tree, excluded)

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="max-h-[min(32rem,calc(100dvh-2rem))] sm:max-w-sm">
        <DialogHeader>
          <DialogTitle className="wrap-break-word">
            {t("local.tag.move_title").replace("{name}", labelFor(node))}
          </DialogTitle>
        </DialogHeader>
        <ul className="-mx-1 flex min-h-0 flex-col gap-0.5 overflow-y-auto overscroll-contain px-1">
          <li>
            <button
              type="button"
              disabled={node.parent_id === null}
              className={TARGET_CLASS_NAME}
              onClick={() => onSelect(null)}
            >
              <Layers aria-hidden="true" />
              <span className="min-w-0 flex-1 truncate">
                {t("local.tag.move_root")}
              </span>
              {node.parent_id === null && <Check aria-hidden="true" />}
            </button>
          </li>
          {rows.map(({ node: target, depth }) => (
            <li key={target.id}>
              <button
                type="button"
                disabled={target.id === node.parent_id}
                style={{ paddingLeft: `${depth * 1.25 + 0.75}rem` }}
                className={TARGET_CLASS_NAME}
                onClick={() => onSelect(target.id)}
              >
                <TagIcon aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">
                  {labelFor(target)}
                </span>
                {target.id === node.parent_id && <Check aria-hidden="true" />}
              </button>
            </li>
          ))}
        </ul>
        <DialogClose render={<Button variant="outline" />}>
          {t("local.common.cancel")}
        </DialogClose>
      </DialogContent>
    </Dialog>
  )
}
