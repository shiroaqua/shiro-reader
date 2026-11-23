import { describe, expect, it, vi } from "vitest"
import {
  BACK_PRIORITY,
  dispatchBack,
  registerBackHandler,
} from "@/lib/back-navigation"

describe("back-navigation", () => {
  it("runs registered handlers in descending priority order and stops at the first consumer", () => {
    const calls: string[] = []
    const unsubNav = registerBackHandler(BACK_PRIORITY.navigation, () => {
      calls.push("navigation")
      return true
    })
    const unsubSearch = registerBackHandler(BACK_PRIORITY.search, () => {
      calls.push("search")
      return true
    })
    const unsubSelection = registerBackHandler(BACK_PRIORITY.selection, () => {
      calls.push("selection")
      return false
    })

    try {
      expect(dispatchBack()).toBe(true)
      expect(calls).toEqual(["search"])

      unsubSearch()
      calls.length = 0

      // Selection returns false so dispatch falls through to navigation
      expect(dispatchBack()).toBe(true)
      expect(calls).toEqual(["selection", "navigation"])
    } finally {
      unsubNav()
      unsubSelection()
    }
  })

  it("returns false when no handler consumes the press", () => {
    const unsub = registerBackHandler(BACK_PRIORITY.navigation, () => false)
    try {
      expect(dispatchBack()).toBe(false)
    } finally {
      unsub()
    }
    expect(dispatchBack()).toBe(false)
  })

  it.each(["dialog", "alertdialog", "menu"])(
    "dispatches Escape to active %s overlay without invoking registered handlers",
    (role) => {
      const overlay = document.createElement("div")
      overlay.setAttribute("role", role)
      document.body.appendChild(overlay)

      const keydownListener = vi.fn()
      document.body.addEventListener("keydown", keydownListener)

      const handler = vi.fn(() => true)
      const unsub = registerBackHandler(BACK_PRIORITY.search, handler)

      try {
        expect(dispatchBack()).toBe(true)
        expect(handler).not.toHaveBeenCalled()
        expect(keydownListener).toHaveBeenCalledTimes(1)
        const event = keydownListener.mock.calls[0]![0] as KeyboardEvent
        expect(event.key).toBe("Escape")
      } finally {
        unsub()
        document.body.removeEventListener("keydown", keydownListener)
        overlay.remove()
      }
    }
  )
})
