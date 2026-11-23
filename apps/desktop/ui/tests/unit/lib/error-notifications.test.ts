import { describe, expect, it, vi } from "vitest"
import {
  dismissError,
  getErrorNotifications,
  markRustErrorHandled,
  notifyLocalError,
  notifyRustError,
  subscribeToErrors,
} from "@/lib/error-notifications"
import { makeRustErrorEvent } from "../../support/builders"

describe("error-notifications", () => {
  it("deduplicates Rust errors by errorId and suppresses pre-handled diagnostic ids", () => {
    const first = makeRustErrorEvent({ errorId: "err-1", message: "First" })
    notifyRustError(first)
    notifyRustError(first)
    markRustErrorHandled("err-handled")
    notifyRustError(makeRustErrorEvent({ errorId: "err-handled" }))

    expect(getErrorNotifications()).toHaveLength(1)
    expect(getErrorNotifications()[0]).toMatchObject({
      errorId: "err-1",
      translation: { source: "rust", message: "First" },
    })
  })

  it("evicts the oldest seen Rust errorId after tracking 256 unique ids", () => {
    for (let i = 0; i < 257; i += 1) {
      markRustErrorHandled(`seen-${i}`)
    }

    // `seen-0` was evicted when `seen-256` was inserted; `seen-1` is still remembered
    notifyRustError(makeRustErrorEvent({ errorId: "seen-1" }))
    expect(getErrorNotifications()).toHaveLength(0)

    notifyRustError(makeRustErrorEvent({ errorId: "seen-0", message: "Re-shown" }))
    expect(getErrorNotifications()).toHaveLength(1)
  })

  it("moves duplicate local errors to the newest slot and caps visible stack at 5", () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"))

    const listener = vi.fn()
    const unsubscribe = subscribeToErrors(listener)

    notifyLocalError("local.common.error.unknown")
    const initialId = getErrorNotifications()[0].id

    vi.advanceTimersByTime(500)
    notifyRustError(makeRustErrorEvent({ errorId: "r1", message: "R1" }))
    notifyLocalError("local.common.error.unknown")

    // Same local error key reuses its id and moves to the tail
    expect(getErrorNotifications().map((item) => item.id)).toEqual([
      initialId + 1,
      initialId,
    ])

    for (let i = 2; i <= 6; i += 1) {
      notifyRustError(makeRustErrorEvent({ errorId: `r${i}`, message: `R${i}` }))
    }

    // Max 5 notifications kept (oldest dropped)
    expect(getErrorNotifications()).toHaveLength(5)
    expect(
      getErrorNotifications().map((item) =>
        item.translation.source === "rust" ? item.translation.message : "local"
      )
    ).toEqual(["R2", "R3", "R4", "R5", "R6"])

    unsubscribe()
    expect(listener).toHaveBeenCalled()
  })

  it("auto-dismisses notifications after 3000ms and reschedules when oldest is dismissed manually", () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"))

    notifyRustError(makeRustErrorEvent({ errorId: "a", message: "First" }))
    vi.advanceTimersByTime(1000)
    notifyRustError(makeRustErrorEvent({ errorId: "b", message: "Second" }))

    const [first, second] = getErrorNotifications()
    dismissError(first.id)

    // Remaining error was created at t=1000ms, so it dismisses at t=4000ms
    vi.advanceTimersByTime(2999)
    expect(getErrorNotifications()).toHaveLength(1)
    expect(getErrorNotifications()[0].id).toBe(second.id)

    vi.advanceTimersByTime(1)
    expect(getErrorNotifications()).toHaveLength(0)
  })
})
