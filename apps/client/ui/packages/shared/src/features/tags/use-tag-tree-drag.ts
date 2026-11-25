import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { tagNameColor } from "@shiro/ui-shared/features/tags/tag-color-picker"
import { translateTagName } from "@shiro/ui-shared/lib/i18n"
import { collectSubtreeIds } from "@shiro/ui-shared/lib/tag-tree"
import type { TagTreeNode } from "@shiro/ui-shared/lib/types"
import type { PendingDelete } from "@shiro/ui-shared/features/tags/tag-manage-model"

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
    const pointerId = event.pointerId
    try {
      source.setPointerCapture(pointerId)
    } catch {
      // Synthetic or detached pointer events may reject capture.
    }
    const preview = document.createElement("div")
    const checkbox = source.querySelector("[role='checkbox']")?.cloneNode(true)
    const name = document.createElement("span")
    name.textContent = translateTagName(
      pendingRenames.get(node.id) ?? node.name
    )
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
      const parentId = row?.dataset["tagId"] ?? null
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

    let scrollContainer: HTMLElement | null = source.closest<HTMLElement>(
      "[data-tag-tree-root]"
    )
    let scrollVelocity = 0
    let scrollFrame: number | undefined
    const stepScroll = () => {
      if (!scrollContainer || scrollVelocity === 0) {
        scrollFrame = undefined
        return
      }
      scrollContainer.scrollTop += scrollVelocity
      scrollFrame = requestAnimationFrame(stepScroll)
    }
    const updateAutoScroll = (clientY: number) => {
      if (!scrollContainer) return
      const rect = scrollContainer.getBoundingClientRect()
      const edge = 36
      if (clientY < rect.top + edge) {
        scrollVelocity = -6
      } else if (clientY > rect.bottom - edge) {
        scrollVelocity = 6
      } else {
        scrollVelocity = 0
      }
      if (scrollVelocity !== 0 && scrollFrame === undefined) {
        scrollFrame = requestAnimationFrame(stepScroll)
      }
    }

    const cleanup = () => {
      highlight(null)
      scrollVelocity = 0
      if (scrollFrame !== undefined) cancelAnimationFrame(scrollFrame)
      scrollContainer = null
      try {
        if (source.hasPointerCapture(pointerId)) {
          source.releasePointerCapture(pointerId)
        }
      } catch {
        // Element may already be detached.
      }
      source.style.opacity = originalOpacity
      preview.remove()
      window.removeEventListener("pointermove", move)
      window.removeEventListener("pointerup", drop)
      window.removeEventListener("pointercancel", cancel)
      window.removeEventListener("blur", cancel)
      window.removeEventListener("keydown", onKeyDown, true)
      dragCleanup.current = null
      setIsDragging(false)
    }
    const move = (pointer: PointerEvent) => {
      if (pointer.pointerId !== pointerId) return
      preview.style.left = `${pointer.clientX}px`
      preview.style.top = `${pointer.clientY}px`
      updateAutoScroll(pointer.clientY)
      highlight(dropTargetAt(pointer.clientX, pointer.clientY)?.element ?? null)
    }
    const drop = (pointer: PointerEvent) => {
      if (pointer.pointerId !== pointerId) return
      const target = dropTargetAt(pointer.clientX, pointer.clientY)
      cleanup()
      if (target) onMove(node.id, target.parentId)
    }
    const cancel = () => cleanup()
    const onKeyDown = (key: KeyboardEvent) => {
      if (key.key === "Escape") {
        key.preventDefault()
        key.stopPropagation()
        cleanup()
      }
    }
    dragCleanup.current = cleanup
    window.addEventListener("pointermove", move)
    window.addEventListener("pointerup", drop)
    window.addEventListener("pointercancel", cancel)
    window.addEventListener("blur", cancel)
    window.addEventListener("keydown", onKeyDown, true)
  }

  return { isDragging, startDrag }
}
