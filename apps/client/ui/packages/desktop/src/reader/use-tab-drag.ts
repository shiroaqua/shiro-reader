import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react"
import type { ReaderWindowTabDto } from "@shiro/ui-shared/api/reader-bridge"

export const TAB_DRAG_THRESHOLD_PX = 6
export const TAB_STRIP_TOLERANCE_PX = 36

export type TabDragPhase =
  | "idle"
  | "pending"
  | "in_strip"
  | "cross_window"
  | "tear_off_preview"

export interface UseTabDragOptions {
  readonly windowId: string
  readonly tabs: readonly ReaderWindowTabDto[]
  readonly canCrossWindowPointerHitTest: boolean
  readonly stripRef: RefObject<HTMLElement | null>
  readonly onActivateTab: (tabId: string) => void
  readonly onReorderTabs: (tabIds: string[]) => void
  readonly onDragStart?: ((windowId: string, tabId: string) => void) | undefined
  readonly onDragOver?:
    | ((windowId: string, clientX: number, clientY: number) => void)
    | undefined
  readonly onDragCommit?:
    | ((windowId: string, clientX: number, clientY: number) => void)
    | undefined
  readonly onDragCancel?: ((windowId: string) => void) | undefined
  readonly onTearOffTab?: ((windowId: string, tabId: string) => void) | undefined
}

interface DragSession {
  readonly pointerId: number
  readonly tabId: string
  readonly startX: number
  readonly startY: number
  readonly originalOrder: string[]
  phase: Exclude<TabDragPhase, "idle">
  previewOrder: string[]
  latestX: number
  latestY: number
}

function computeReorderedIds(
  currentOrder: readonly string[],
  draggedTabId: string,
  clientX: number,
  stripEl: HTMLElement | null
): string[] {
  if (!stripEl || currentOrder.length <= 1) return [...currentOrder]

  const otherTabElements = Array.from(
    stripEl.querySelectorAll<HTMLElement>("[data-reader-tab-id]")
  ).filter((el) => el.dataset["readerTabId"] !== draggedTabId)
  if (otherTabElements.length === 0) return [...currentOrder]

  const withoutDragged = currentOrder.filter((id) => id !== draggedTabId)
  let targetIndex = withoutDragged.length

  for (let i = 0; i < otherTabElements.length; i++) {
    const el = otherTabElements[i]!
    const rect = el.getBoundingClientRect()
    const midpoint = rect.left + rect.width / 2
    if (clientX < midpoint) {
      targetIndex = i
      break
    }
  }

  withoutDragged.splice(targetIndex, 0, draggedTabId)
  return withoutDragged
}

function arraysEqual(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}

export function useTabDrag({
  windowId,
  tabs,
  canCrossWindowPointerHitTest,
  stripRef,
  onActivateTab,
  onReorderTabs,
  onDragStart,
  onDragOver,
  onDragCommit,
  onDragCancel,
  onTearOffTab,
}: UseTabDragOptions) {
  const [phase, setPhase] = useState<TabDragPhase>("idle")
  const [draggingTabId, setDraggingTabId] = useState<string | null>(null)
  const [previewOrder, setPreviewOrder] = useState<readonly string[] | null>(
    null
  )

  const sessionRef = useRef<DragSession | null>(null)
  const rafRef = useRef<number | null>(null)
  const pendingCoordsRef = useRef<{ x: number; y: number } | null>(null)

  const callbacksRef = useRef({
    onActivateTab,
    onReorderTabs,
    onDragStart,
    onDragOver,
    onDragCommit,
    onDragCancel,
    onTearOffTab,
  })
  useEffect(() => {
    callbacksRef.current = {
      onActivateTab,
      onReorderTabs,
      onDragStart,
      onDragOver,
      onDragCommit,
      onDragCancel,
      onTearOffTab,
    }
  })

  const cancelPendingRaf = useCallback(() => {
    if (rafRef.current !== null && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(rafRef.current)
    }
    rafRef.current = null
    pendingCoordsRef.current = null
  }, [])

  const resetDragState = useCallback(() => {
    cancelPendingRaf()
    sessionRef.current = null
    setPhase("idle")
    setDraggingTabId(null)
    setPreviewOrder(null)
  }, [cancelPendingRaf])

  const cancelActiveDrag = useCallback(() => {
    const current = sessionRef.current
    if (!current) return
    const wasCrossWindow = current.phase === "cross_window"
    resetDragState()
    if (wasCrossWindow) {
      callbacksRef.current.onDragCancel?.(windowId)
    }
  }, [resetDragState, windowId])

  useEffect(() => {
    if (typeof window === "undefined") return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && sessionRef.current) {
        event.preventDefault()
        cancelActiveDrag()
      }
    }
    const handleBlur = () => {
      if (sessionRef.current) {
        cancelActiveDrag()
      }
    }
    window.addEventListener("keydown", handleKeyDown)
    window.addEventListener("blur", handleBlur)
    return () => {
      window.removeEventListener("keydown", handleKeyDown)
      window.removeEventListener("blur", handleBlur)
    }
  }, [cancelActiveDrag])

  const scheduleDragOver = useCallback(
    (clientX: number, clientY: number) => {
      pendingCoordsRef.current = { x: clientX, y: clientY }
      if (rafRef.current !== null) return

      const schedule =
        typeof requestAnimationFrame === "function"
          ? requestAnimationFrame
          : (cb: FrameRequestCallback) =>
              globalThis.setTimeout(() => cb(Date.now()), 0) as unknown as number

      rafRef.current = schedule(() => {
        rafRef.current = null
        const coords = pendingCoordsRef.current
        pendingCoordsRef.current = null
        if (!coords || sessionRef.current?.phase !== "cross_window") return
        callbacksRef.current.onDragOver?.(windowId, coords.x, coords.y)
      })
    },
    [windowId]
  )

  const handleTabPointerDown = useCallback(
    (tabId: string, event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0 || event.isPrimary === false) return
      try {
        event.currentTarget.setPointerCapture(event.pointerId)
      } catch {
        // Ignore in synthetic test environments that do not implement pointer capture.
      }

      const originalOrder = tabs.map((t) => t.tabId)
      sessionRef.current = {
        pointerId: event.pointerId,
        tabId,
        startX: event.clientX,
        startY: event.clientY,
        originalOrder,
        phase: "pending",
        previewOrder: originalOrder,
        latestX: event.clientX,
        latestY: event.clientY,
      }
      setPhase("pending")
      setDraggingTabId(tabId)
      setPreviewOrder(null)
    },
    [tabs]
  )

  const handlePointerMove = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const session = sessionRef.current
      if (!session || session.pointerId !== event.pointerId) return

      session.latestX = event.clientX
      session.latestY = event.clientY

      if (session.phase === "pending") {
        const distance = Math.hypot(
          event.clientX - session.startX,
          event.clientY - session.startY
        )
        if (distance < TAB_DRAG_THRESHOLD_PX) return
        session.phase = "in_strip"
        setPhase("in_strip")
      }

      const stripEl = stripRef.current
      const stripRect = stripEl?.getBoundingClientRect()
      const withinBand =
        !stripRect ||
        (event.clientY >= stripRect.top - TAB_STRIP_TOLERANCE_PX &&
          event.clientY <= stripRect.bottom + TAB_STRIP_TOLERANCE_PX)

      if (withinBand) {
        if (session.phase === "cross_window") {
          cancelPendingRaf()
          callbacksRef.current.onDragCancel?.(windowId)
        }
        session.phase = "in_strip"
        setPhase("in_strip")

        const nextOrder = computeReorderedIds(
          session.previewOrder,
          session.tabId,
          event.clientX,
          stripEl
        )
        if (!arraysEqual(nextOrder, session.previewOrder)) {
          session.previewOrder = nextOrder
          setPreviewOrder(nextOrder)
        }
        return
      }

      // Pointer moved outside the ±36px vertical tolerance band.
      if (canCrossWindowPointerHitTest) {
        if (session.phase !== "cross_window") {
          session.phase = "cross_window"
          session.previewOrder = session.originalOrder
          setPhase("cross_window")
          setPreviewOrder(null)
          callbacksRef.current.onDragStart?.(windowId, session.tabId)
        }
        scheduleDragOver(event.clientX, event.clientY)
      } else if (session.phase !== "tear_off_preview") {
        session.phase = "tear_off_preview"
        session.previewOrder = session.originalOrder
        setPhase("tear_off_preview")
        setPreviewOrder(null)
      }
    },
    [
      canCrossWindowPointerHitTest,
      cancelPendingRaf,
      scheduleDragOver,
      stripRef,
      windowId,
    ]
  )

  const handlePointerUp = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const session = sessionRef.current
      if (!session || session.pointerId !== event.pointerId) return

      const currentPhase = session.phase
      const draggedId = session.tabId
      const finalOrder = session.previewOrder
      const originalOrder = session.originalOrder

      resetDragState()

      if (currentPhase === "pending") {
        callbacksRef.current.onActivateTab(draggedId)
        return
      }
      if (currentPhase === "in_strip") {
        if (!arraysEqual(finalOrder, originalOrder)) {
          callbacksRef.current.onReorderTabs(finalOrder)
        }
        return
      }
      if (currentPhase === "cross_window") {
        callbacksRef.current.onDragCommit?.(
          windowId,
          event.clientX,
          event.clientY
        )
        return
      }
      if (currentPhase === "tear_off_preview" && tabs.length > 1) {
        // Wayland tear-off executes ONLY on explicit pointerup while in TearOffPreview.
        callbacksRef.current.onTearOffTab?.(windowId, draggedId)
      }
    },
    [resetDragState, tabs.length, windowId]
  )

  const handlePointerCancel = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const session = sessionRef.current
      if (!session || session.pointerId !== event.pointerId) return
      cancelActiveDrag()
    },
    [cancelActiveDrag]
  )

  const orderedTabs = useMemo(() => {
    if (!previewOrder) return tabs
    const byId = new Map(tabs.map((tab) => [tab.tabId, tab]))
    const result: ReaderWindowTabDto[] = []
    for (const id of previewOrder) {
      const found = byId.get(id)
      if (found) {
        result.push(found)
        byId.delete(id)
      }
    }
    for (const leftover of byId.values()) {
      result.push(leftover)
    }
    return result
  }, [tabs, previewOrder])

  return {
    dragPhase: phase,
    draggingTabId,
    orderedTabs,
    handleTabPointerDown,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel,
    cancelActiveDrag,
  }
}
