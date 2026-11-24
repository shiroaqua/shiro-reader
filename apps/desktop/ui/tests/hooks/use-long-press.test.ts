import type { PointerEvent as ReactPointerEvent } from "react"
import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { useLongPress } from "@/hooks/use-long-press"
import { tauriTestHost } from "../support/mock-tauri"

function pointerEvent(
  x: number,
  y: number,
  pointerType: "touch" | "mouse" = "touch"
): ReactPointerEvent {
  return {
    clientX: x,
    clientY: y,
    pointerType,
  } as ReactPointerEvent
}

describe("useLongPress", () => {
  it("fires after 420ms on touch hold and consumes the subsequent click once", () => {
    vi.useFakeTimers()
    const onLongPress = vi.fn()
    const { result } = renderHook(() => useLongPress(onLongPress, true))

    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(100, 100, "touch"))
    })

    vi.advanceTimersByTime(419)
    expect(onLongPress).not.toHaveBeenCalled()

    vi.advanceTimersByTime(1)
    expect(onLongPress).toHaveBeenCalledTimes(1)
    expect(result.current.consumeClick()).toBe(true)
    expect(result.current.consumeClick()).toBe(false)
  })

  it("ignores mouse pointers and disabled state", () => {
    vi.useFakeTimers()
    const onLongPress = vi.fn()

    const { result: mouseHook } = renderHook(() =>
      useLongPress(onLongPress, true)
    )
    act(() => {
      mouseHook.current.handlers.onPointerDown(pointerEvent(0, 0, "mouse"))
    })
    vi.advanceTimersByTime(500)
    expect(onLongPress).not.toHaveBeenCalled()

    const { result: disabledHook } = renderHook(() =>
      useLongPress(onLongPress, false)
    )
    act(() => {
      disabledHook.current.handlers.onPointerDown(pointerEvent(0, 0, "touch"))
    })
    vi.advanceTimersByTime(500)
    expect(onLongPress).not.toHaveBeenCalled()
  })

  it("cancels when finger moves beyond 10px or lifts before threshold", () => {
    vi.useFakeTimers()
    const onLongPress = vi.fn()
    const { result } = renderHook(() => useLongPress(onLongPress, true))

    // Small jitter inside tolerance does not cancel
    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(10, 10))
      result.current.handlers.onPointerMove(pointerEvent(16, 16))
    })
    vi.advanceTimersByTime(420)
    expect(onLongPress).toHaveBeenCalledTimes(1)
    result.current.consumeClick()

    // Movement > 10px cancels
    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(10, 10))
      result.current.handlers.onPointerMove(pointerEvent(22, 10))
    })
    vi.advanceTimersByTime(500)
    expect(onLongPress).toHaveBeenCalledTimes(1)

    // Early pointerUp cancels
    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(10, 10))
      vi.advanceTimersByTime(200)
      result.current.handlers.onPointerUp()
    })
    vi.advanceTimersByTime(500)
    expect(onLongPress).toHaveBeenCalledTimes(1)
    expect(result.current.consumeClick()).toBe(false)
  })

  it("triggers haptic feedback on mobile when firing and cancels if disabled mid-hold", () => {
    vi.useFakeTimers()
    tauriTestHost.setPlatform("android")
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => undefined)
    const onLongPress = vi.fn()

    const { result, rerender } = renderHook(
      ({ enabled }) => useLongPress(onLongPress, enabled),
      { initialProps: { enabled: true } }
    )

    // Completing the hold fires haptic once
    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(10, 10, "touch"))
    })
    vi.advanceTimersByTime(420)
    expect(onLongPress).toHaveBeenCalledTimes(1)
    expect(tauriTestHost.invokeCalls("trigger_long_press_haptic")).toHaveLength(
      1
    )
    result.current.consumeClick()

    // Early release cancels without haptic
    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(10, 10, "touch"))
      vi.advanceTimersByTime(200)
      result.current.handlers.onPointerUp()
    })
    vi.advanceTimersByTime(500)
    expect(onLongPress).toHaveBeenCalledTimes(1)
    expect(tauriTestHost.invokeCalls("trigger_long_press_haptic")).toHaveLength(
      1
    )

    // Disabling mid-hold cancels the pending timer without haptic
    act(() => {
      result.current.handlers.onPointerDown(pointerEvent(10, 10, "touch"))
      vi.advanceTimersByTime(200)
    })
    rerender({ enabled: false })
    vi.advanceTimersByTime(500)
    expect(onLongPress).toHaveBeenCalledTimes(1)
    expect(tauriTestHost.invokeCalls("trigger_long_press_haptic")).toHaveLength(
      1
    )
  })
})
