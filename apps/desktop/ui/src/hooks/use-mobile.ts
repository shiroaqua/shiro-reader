import { useSyncExternalStore } from "react"
import { platform } from "@tauri-apps/plugin-os"
import { isTauriRuntime } from "@/api/tauri-bridge"

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

function readIsPortrait(): boolean {
  if (typeof window === "undefined") return false
  const orientationType = window.screen?.orientation?.type
  if (orientationType) {
    return orientationType.startsWith("portrait")
  }
  return window.matchMedia("(orientation: portrait)").matches
}

function subscribeOrientation(onChange: () => void): () => void {
  const screenOrientation = window.screen?.orientation
  if (screenOrientation) {
    screenOrientation.addEventListener("change", onChange)
    window.addEventListener("orientationchange", onChange)
    return () => {
      screenOrientation.removeEventListener("change", onChange)
      window.removeEventListener("orientationchange", onChange)
    }
  }
  const mql = window.matchMedia("(orientation: portrait)")
  mql.addEventListener("change", onChange)
  window.addEventListener("orientationchange", onChange)
  return () => {
    mql.removeEventListener("change", onChange)
    window.removeEventListener("orientationchange", onChange)
  }
}

function subscribeBottomNav(onChange: () => void): () => void {
  if (!isMobilePlatform()) return () => {}
  return subscribeOrientation(onChange)
}

function getBottomNavSnapshot(): boolean {
  return isMobilePlatform() && readIsPortrait()
}

function getServerSnapshot(): boolean {
  return false
}

/**
 * Determines whether to use the bottom navigation bar instead of the sidebar:
 * - Desktop: always false (never uses a bottom menu).
 * - Mobile portrait: true (uses bottom navigation).
 * - Mobile landscape: false (uses the same sidebar as desktop).
 */
export function useBottomNavigation(): boolean {
  return useSyncExternalStore(
    subscribeBottomNav,
    getBottomNavSnapshot,
    getServerSnapshot
  )
}
