import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { tagNameColor } from "@/features/tags/tag-color-picker"
import { collectSubtreeIds } from "@/lib/tag-tree"
import type { TagTreeNode } from "@/lib/types"
import type { PendingDelete } from "@/features/tags/tag-manage-model"

export function useTagTreeDrag({
  isSaving,
  pendingDeletes,
  cascadeDeletedIds,
  pendingRenames,
  pendingRecolors,
  onMove,
}: {
  isSaving: boolean
  pendingDeletes: ReadonlyMap<string, PendingDelete>
  cascadeDeletedIds: ReadonlySet<string>
  pendingRenames: ReadonlyMap<string, string>
  pendingRecolors: ReadonlyMap<string, string>
  onMove: (id: string, parentId: string | null) => void
}) {
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
      isSaving ||
      pendingDeletes.has(node.id) ||
      cascadeDeletedIds.has(node.id) ||
      (event.target instanceof Element &&
        event.target.closest("button, input, a, [role='checkbox']"))
    )
      return

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
      )
        return null
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
      if (target) onMove(node.id, target.parentId)
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

  return { isDragging, startDrag }
}
