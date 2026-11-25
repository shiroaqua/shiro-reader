/**
 * Hardware/gesture "back" routing for Android. Open overlays (dialogs,
 * sheets, menus) are closed first; otherwise the highest-priority registered
 * handler consumes the press. Returning false lets the caller exit the app.
 */
export const BACK_PRIORITY = {
  navigation: 10,
  selection: 30,
  search: 40,
  uploadOverlay: 50,
} as const

type BackHandler = { priority: number; handle: () => boolean }

const handlers = new Set<BackHandler>()

const OVERLAY_SELECTOR = '[role="dialog"], [role="alertdialog"], [role="menu"]'

export function registerBackHandler(
  priority: number,
  handle: () => boolean
): () => void {
  const entry: BackHandler = { priority, handle }
  handlers.add(entry)
  return () => {
    handlers.delete(entry)
  }
}

/** Returns true when the back press was consumed by the UI. */
export function dispatchBack(): boolean {
  const ordered = [...handlers].sort((a, b) => b.priority - a.priority)
  const topPriority = ordered.filter(
    (entry) => entry.priority >= BACK_PRIORITY.uploadOverlay
  )
  if (topPriority.some((entry) => entry.handle())) {
    return true
  }
  if (document.querySelector(OVERLAY_SELECTOR) !== null) {
    const target = document.activeElement ?? document.body
    target.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Escape",
        code: "Escape",
        bubbles: true,
        cancelable: true,
      })
    )
    return true
  }
  return ordered
    .filter((entry) => entry.priority < BACK_PRIORITY.uploadOverlay)
    .some((entry) => entry.handle())
}
