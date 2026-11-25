import { platform } from "@tauri-apps/plugin-os"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"

/**
 * Returns true when running on a mobile OS (Android / iOS) via Tauri's OS API.
 * Desktop environments (Windows / macOS / Linux) return false regardless of window width.
 */
export function isMobilePlatform(): boolean {
  if (!isTauriRuntime()) return false
  try {
    const current = platform()
    return current === "android" || current === "ios"
  } catch {
    return false
  }
}
