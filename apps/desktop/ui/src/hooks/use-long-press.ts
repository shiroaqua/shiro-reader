import {
  useCallback,
  useEffect,
  useRef,
  type PointerEvent as ReactPointerEvent,
} from "react"

const LONG_PRESS_MS = 420
const MOVE_TOLERANCE_PX = 10

/**
 * Long-press detection for touch pointers. Scrolling or moving the finger
 * cancels it. After a long press fires, call `consumeClick` from the click
 * handler to swallow the click the browser still dispatches on release.
 */
export function useLongPress(onLongPress: () => void, enabled: boolean) {
  const timer = useRef<number | null>(null)
  const origin = useRef<{ x: number; y: number } | null>(null)
  const fired = useRef(false)
  const latest = useRef(onLongPress)

  useEffect(() => {
    latest.current = onLongPress
  })

  const cancel = useCallback(() => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current)
      timer.current = null
    }
    origin.current = null
  }, [])

  useEffect(() => cancel, [cancel])

  const onPointerDown = (event: ReactPointerEvent) => {
    if (!enabled || event.pointerType !== "touch") return
    cancel()
    fired.current = false
    origin.current = { x: event.clientX, y: event.clientY }
    timer.current = window.setTimeout(() => {
      timer.current = null
      fired.current = true
      latest.current()
    }, LONG_PRESS_MS)
  }

  const onPointerMove = (event: ReactPointerEvent) => {
    const start = origin.current
    if (!start) return
    if (
      Math.hypot(event.clientX - start.x, event.clientY - start.y) >
      MOVE_TOLERANCE_PX
    ) {
      cancel()
    }
  }

  const consumeClick = () => {
    const wasLongPress = fired.current
    fired.current = false
    return wasLongPress
  }

  return {
    handlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp: cancel,
      onPointerCancel: cancel,
      onPointerLeave: cancel,
    },
    consumeClick,
  }
}
