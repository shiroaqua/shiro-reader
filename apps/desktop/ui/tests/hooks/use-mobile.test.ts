import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { isMobilePlatform, useBottomNavigation } from "@/hooks/use-mobile"
import {
  setMediaQueryMatch,
  setScreenOrientation,
} from "../support/media"
import { tauriTestHost } from "../support/mock-tauri"

describe("use-mobile", () => {
  describe("isMobilePlatform", () => {
    it("returns false outside Tauri or on desktop OS, and true on android or ios", () => {
      tauriTestHost.setTauriRuntime(false)
      tauriTestHost.setPlatform("android")
      expect(isMobilePlatform()).toBe(false)

      tauriTestHost.setTauriRuntime(true)
      tauriTestHost.setPlatform("linux")
      expect(isMobilePlatform()).toBe(false)

      tauriTestHost.setPlatform("windows")
      expect(isMobilePlatform()).toBe(false)

      tauriTestHost.setPlatform("android")
      expect(isMobilePlatform()).toBe(true)

      tauriTestHost.setPlatform("ios")
      expect(isMobilePlatform()).toBe(true)
    })
  })

  describe("useBottomNavigation", () => {
    it("always returns false on desktop even in portrait orientation", () => {
      tauriTestHost.setPlatform("linux")
      setScreenOrientation("portrait-primary")

      const { result } = renderHook(() => useBottomNavigation())
      expect(result.current).toBe(false)
    })

    it("reacts to screen.orientation changes on mobile", () => {
      tauriTestHost.setPlatform("android")
      setScreenOrientation("portrait-primary")

      const { result } = renderHook(() => useBottomNavigation())
      expect(result.current).toBe(true)

      act(() => {
        setScreenOrientation("landscape-primary")
      })
      expect(result.current).toBe(false)

      act(() => {
        setScreenOrientation("portrait-primary")
      })
      expect(result.current).toBe(true)
    })

    it("falls back to matchMedia when screen.orientation is unavailable", () => {
      tauriTestHost.setPlatform("android")
      setScreenOrientation(null)
      setMediaQueryMatch("(orientation: portrait)", true)

      const { result } = renderHook(() => useBottomNavigation())
      expect(result.current).toBe(true)

      act(() => {
        setMediaQueryMatch("(orientation: portrait)", false)
      })
      expect(result.current).toBe(false)
    })
  })
})
