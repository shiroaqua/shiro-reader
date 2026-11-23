import { useSyncExternalStore } from "react"

const TOUCH_QUERY = "(hover: none) and (pointer: coarse)"

function subscribe(onChange: () => void) {
  const mql = window.matchMedia(TOUCH_QUERY)
  mql.addEventListener("change", onChange)
  return () => mql.removeEventListener("change", onChange)
}

function getSnapshot() {
  return window.matchMedia(TOUCH_QUERY).matches
}

function getServerSnapshot() {
  return false
}

/** True when the primary input is a finger: no hover, coarse pointer. */
export function useIsTouch(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot)
}
