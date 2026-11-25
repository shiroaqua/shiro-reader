import { useEffect } from "react"
import type { PluginListener } from "@tauri-apps/api/core"
import { exit, onBackButtonPress } from "@tauri-apps/api/app"
import { platform } from "@tauri-apps/plugin-os"
import { dispatchBack } from "@shiro/ui-shared/lib/back-navigation"

/** Routes the Android back button/gesture through the UI back stack. */
export function useAndroidBackButton(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return
    let disposed = false
    let listener: PluginListener | null = null

    void (async () => {
      try {
        if (platform() !== "android") return
        const registered = await onBackButtonPress(() => {
          if (!dispatchBack()) void exit(0)
        })
        if (disposed) void registered.unregister()
        else listener = registered
      } catch {
        // Without the listener Android falls back to its default back handling.
      }
    })()

    return () => {
      disposed = true
      void listener?.unregister()
    }
  }, [enabled])
}
