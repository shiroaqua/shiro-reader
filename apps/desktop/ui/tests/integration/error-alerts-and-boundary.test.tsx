import { act, fireEvent, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ApiErrorAlerts } from "@/components/api-error-alerts"
import { ErrorBoundary } from "@/components/error-boundary"
import { notifyRustError } from "@/lib/error-notifications"
import { makeRustErrorEvent } from "../support/builders"
import { renderWithProviders } from "../support/render"

function BrokenWidget(): never {
  throw new Error("Render crash")
}

describe("ApiErrorAlerts and ErrorBoundary", () => {
  describe("ApiErrorAlerts", () => {
    it("renders newest alert first, closes manually, and auto-dismisses after 3000ms", () => {
      vi.useFakeTimers()
      vi.setSystemTime(new Date("2026-01-01T12:00:00.000Z"))

      renderWithProviders(<ApiErrorAlerts />)

      act(() => {
        notifyRustError(
          makeRustErrorEvent({ errorId: "err-1", message: "First failure" })
        )
        vi.advanceTimersByTime(500)
        notifyRustError(
          makeRustErrorEvent({ errorId: "err-2", message: "Second failure" })
        )
      })

      const alerts = screen.getAllByRole("alert")
      expect(alerts).toHaveLength(2)
      expect(alerts[0]).toHaveTextContent("Second failure")
      expect(alerts[1]).toHaveTextContent("First failure")

      const closeButtons = screen.getAllByRole("button", { name: "Close" })
      fireEvent.click(closeButtons[0])

      expect(screen.queryByText("Second failure")).not.toBeInTheDocument()
      expect(screen.getByText("First failure")).toBeInTheDocument()

      act(() => {
        vi.advanceTimersByTime(2500)
      })
      expect(screen.queryByText("First failure")).not.toBeInTheDocument()
    })
  })

  describe("ErrorBoundary", () => {
    it("catches uncaught render errors and reloads when requested", async () => {
      vi.spyOn(console, "error").mockImplementation(() => {})
      const reloadSpy = vi.fn()
      const originalLocation = window.location
      Object.defineProperty(window, "location", {
        configurable: true,
        value: { ...originalLocation, reload: reloadSpy },
      })

      try {
        const { user } = renderWithProviders(
          <ErrorBoundary>
            <BrokenWidget />
          </ErrorBoundary>
        )

        expect(screen.getByText("Something went wrong")).toBeInTheDocument()
        await user.click(screen.getByRole("button", { name: "Reload" }))
        expect(reloadSpy).toHaveBeenCalledTimes(1)
      } finally {
        Object.defineProperty(window, "location", {
          configurable: true,
          value: originalLocation,
        })
      }
    })
  })
})
