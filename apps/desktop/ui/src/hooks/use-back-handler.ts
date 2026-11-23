import { useEffect, useRef } from "react"
import { registerBackHandler } from "@/lib/back-navigation"

/** While `active`, the Android back action calls `onBack` instead of exiting. */
export function useBackHandler(
  active: boolean,
  priority: number,
  onBack: () => void
) {
  const latest = useRef(onBack)

  useEffect(() => {
    latest.current = onBack
  })

  useEffect(() => {
    if (!active) return
    return registerBackHandler(priority, () => {
      latest.current()
      return true
    })
  }, [active, priority])
}
