import { renderHook, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { useAndroidBackButton } from "@/hooks/use-android-back-button"
import { useBackHandler } from "@/hooks/use-back-handler"
import {
  BACK_PRIORITY,
  dispatchBack,
  registerBackHandler,
} from "@/lib/back-navigation"
import { tauriTestHost } from "../support/mock-tauri"

describe("useBackHandler and useAndroidBackButton", () => {
  describe("useBackHandler", () => {
    it("registers only while active, invokes latest callback, and unregisters on unmount", () => {
      const first = vi.fn()
      const second = vi.fn()

      const { rerender, unmount } = renderHook(
        ({ active, cb }) => useBackHandler(active, BACK_PRIORITY.selection, cb),
        { initialProps: { active: false, cb: first } }
      )

      expect(dispatchBack()).toBe(false)

      rerender({ active: true, cb: first })
      expect(dispatchBack()).toBe(true)
      expect(first).toHaveBeenCalledTimes(1)

      rerender({ active: true, cb: second })
      expect(dispatchBack()).toBe(true)
      expect(second).toHaveBeenCalledTimes(1)

      unmount()
      expect(dispatchBack()).toBe(false)
    })
  })

  describe("useAndroidBackButton", () => {
    it("does not register on non-android platforms or when disabled", () => {
      tauriTestHost.setPlatform("linux")
      renderHook(() => useAndroidBackButton(true))
      expect(tauriTestHost.backListenerCount()).toBe(0)

      tauriTestHost.setPlatform("android")
      renderHook(() => useAndroidBackButton(false))
      expect(tauriTestHost.backListenerCount()).toBe(0)
    })

    it("consumes handled back presses and exits app when stack is empty", async () => {
      tauriTestHost.setPlatform("android")
      renderHook(() => useAndroidBackButton(true))

      await waitFor(() => {
        expect(tauriTestHost.backListenerCount()).toBe(1)
      })

      const unsub = registerBackHandler(BACK_PRIORITY.navigation, () => true)
      tauriTestHost.triggerAndroidBack()
      expect(tauriTestHost.exitCalls).toEqual([])

      unsub()
      tauriTestHost.triggerAndroidBack()
      expect(tauriTestHost.exitCalls).toEqual([0])
    })

    it("unregisters listener immediately if unmounted before async registration finishes", async () => {
      tauriTestHost.setPlatform("android")
      tauriTestHost.setDeferBackRegistration(true)

      const { unmount } = renderHook(() => useAndroidBackButton(true))
      unmount()

      tauriTestHost.flushBackRegistrations()
      await waitFor(() => {
        expect(tauriTestHost.backUnregisterCount()).toBe(1)
      })
      expect(tauriTestHost.backListenerCount()).toBe(0)
    })
  })
})
