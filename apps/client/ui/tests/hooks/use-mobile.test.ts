import { describe, expect, it } from "vitest"
import { isMobilePlatform } from "@shiro/ui-shared/hooks/use-mobile"
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

      tauriTestHost.setPlatform("macos")
      expect(isMobilePlatform()).toBe(false)

      tauriTestHost.setPlatform("android")
      expect(isMobilePlatform()).toBe(true)

      tauriTestHost.setPlatform("ios")
      expect(isMobilePlatform()).toBe(true)
    })
  })
})
