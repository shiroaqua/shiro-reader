import { describe, expect, it, vi } from "vitest"
import { getErrorNotifications } from "@shiro/ui-shared/lib/error-notifications"
import { triggerLongPressHaptic } from "@shiro/ui-shared/lib/haptics"
import { tauriTestHost } from "../../support/mock-tauri"

describe("triggerLongPressHaptic", () => {
  it("invokes the native command on Android inside Tauri", async () => {
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => undefined)

    tauriTestHost.setPlatform("android")
    triggerLongPressHaptic()

    await Promise.resolve()

    expect(tauriTestHost.invokeCalls("trigger_long_press_haptic")).toHaveLength(
      1
    )
  })

  it("does nothing on desktop platforms inside Tauri", () => {
    tauriTestHost.setPlatform("linux")
    triggerLongPressHaptic()

    expect(tauriTestHost.invokeCalls("trigger_long_press_haptic")).toHaveLength(
      0
    )
  })

  it("swallows native invoke errors without showing an error notification", async () => {
    tauriTestHost.setPlatform("android")
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => {
      throw new Error("haptic unavailable")
    })

    triggerLongPressHaptic()
    await Promise.resolve()
    await Promise.resolve()

    expect(getErrorNotifications()).toHaveLength(0)
  })

  it("falls back to navigator.vibrate outside Tauri and ignores failures", () => {
    tauriTestHost.setTauriRuntime(false)
    const originalDescriptor = Object.getOwnPropertyDescriptor(
      navigator,
      "vibrate"
    )

    try {
      const vibrateMock = vi.fn(() => true)
      Object.defineProperty(navigator, "vibrate", {
        value: vibrateMock,
        configurable: true,
      })

      triggerLongPressHaptic()
      expect(vibrateMock).toHaveBeenCalledWith(10)

      const throwingMock = vi.fn(() => {
        throw new Error("blocked")
      })
      Object.defineProperty(navigator, "vibrate", {
        value: throwingMock,
        configurable: true,
      })

      expect(() => triggerLongPressHaptic()).not.toThrow()

      Reflect.deleteProperty(navigator, "vibrate")
      expect(() => triggerLongPressHaptic()).not.toThrow()
    } finally {
      if (originalDescriptor) {
        Object.defineProperty(navigator, "vibrate", originalDescriptor)
      } else {
        Reflect.deleteProperty(navigator, "vibrate")
      }
    }
  })
})
