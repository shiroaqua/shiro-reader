import { isTauriRuntime, tauriBridge } from "@shiro/ui-shared/api/tauri-bridge"
import { isMobilePlatform } from "@shiro/ui-shared/hooks/use-mobile"

const WEB_LONG_PRESS_VIBRATE_MS = 10

/**
 * Triggers a short tactile vibration when a long-press action activates on
 * mobile/touch devices. Uses native Android haptic feedback inside Tauri and
 * falls back to the Web Vibration API in non-Tauri touch browsers.
 */
export function triggerLongPressHaptic(): void {
  if (isTauriRuntime()) {
    if (!isMobilePlatform()) return
    void tauriBridge.triggerLongPressHaptic().catch(() => {
      // Best-effort feedback; never interrupt the UI or show an error toast.
    })
    return
  }

  if (
    typeof navigator !== "undefined" &&
    typeof navigator.vibrate === "function"
  ) {
    try {
      navigator.vibrate(WEB_LONG_PRESS_VIBRATE_MS)
    } catch {
      // Ignore environments that reject vibration calls.
    }
  }
}
