import { useState } from "react"
import { act, fireEvent, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ApiErrorAlerts } from "@shiro/ui-shared/components/api-error-alerts"
import { ErrorBoundary } from "@shiro/ui-shared/components/error-boundary"
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@shiro/ui-shared/components/ui/dialog"
import {
  applyUploadSnapshot,
  openMobileUploadPanel,
} from "@shiro/ui-shared/features/books/book-import-store"
import { notifyRustError } from "@shiro/ui-shared/lib/error-notifications"
import { DesktopOverlayDock } from "@shiro/desktop-ui/DesktopOverlayDock"
import { MobileOverlayHost } from "@shiro/mobile-ui/MobileOverlayHost"
import {
  makeRustErrorEvent,
  makeUploadSnapshot,
  makeUploadTask,
} from "../support/builders"
import { tauriTestHost } from "../support/mock-tauri"
import { renderWithProviders } from "../support/render"

function BrokenWidget(): never {
  throw new Error("Render crash")
}

function DialogWithDockHarness() {
  const [open, setOpen] = useState(true)
  return (
    <>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogTitle>Active Edit Modal</DialogTitle>
          <p>Draft content</p>
        </DialogContent>
      </Dialog>
      <DesktopOverlayDock />
    </>
  )
}

describe("ApiErrorAlerts, OverlayHosts, and ErrorBoundary", () => {
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
      fireEvent.click(closeButtons[0]!)

      expect(screen.queryByText("Second failure")).not.toBeInTheDocument()
      expect(screen.getByText("First failure")).toBeInTheDocument()

      act(() => {
        vi.advanceTimersByTime(2500)
      })
      expect(screen.queryByText("First failure")).not.toBeInTheDocument()
    })
  })

  describe("DesktopOverlayDock and Dialog exemption", () => {
    it("stacks ApiErrorAlerts strictly above UploadPanel inside the unified dock and prevents outside-press closing Dialog", async () => {
      tauriTestHost.setTauriRuntime(false)

      const { user } = renderWithProviders(<DialogWithDockHarness />)

      act(() => {
        notifyRustError(
          makeRustErrorEvent({ errorId: "dock-err", message: "Queue warning" })
        )
        applyUploadSnapshot(
          makeUploadSnapshot({
            revision: 1,
            visible: true,
            tasks: [
              makeUploadTask({
                id: 1,
                fileName: "dock-book.epub",
                status: {
                  state: "active",
                  stage: "uploading",
                  progressPercent: 55,
                  bytesProcessed: 550,
                  totalBytes: 1000,
                  speedBytesPerSec: 100,
                  etaSeconds: 5,
                },
              }),
            ],
          })
        )
      })

      const dock = document.querySelector("[data-floating-dock]")
      expect(dock).not.toBeNull()
      const alertsEl = dock!.querySelector("[data-error-alerts]")
      const panelEl = dock!.querySelector("[data-upload-panel]")
      expect(alertsEl).not.toBeNull()
      expect(panelEl).not.toBeNull()
      // ApiErrorAlerts precedes UploadPanel inside the flex-col dock -> strictly above it.
      expect(
        alertsEl!.compareDocumentPosition(panelEl!) &
          Node.DOCUMENT_POSITION_FOLLOWING
      ).not.toBe(0)

      // Clicking inside UploadPanel (e.g. Minimize button) must NOT dismiss the open Dialog!
      expect(screen.getByText("Active Edit Modal")).toBeInTheDocument()
      await user.click(
        screen.getByRole("button", {
          name: "Minimize upload panel",
          hidden: true,
        })
      )
      expect(screen.getByText("Active Edit Modal")).toBeInTheDocument()
      expect(
        screen.getByRole("button", {
          name: "Expand upload panel",
          hidden: true,
        })
      ).toBeInTheDocument()
    })
  })

  describe("MobileOverlayHost", () => {
    it("renders ApiErrorAlerts and MobileUploadScreen when opened, and never mounts desktop UploadPanel", () => {
      tauriTestHost.setTauriRuntime(false)

      renderWithProviders(<MobileOverlayHost />)

      act(() => {
        notifyRustError(
          makeRustErrorEvent({
            errorId: "mobile-err",
            message: "Mobile overlay warning",
          })
        )
        applyUploadSnapshot(
          makeUploadSnapshot({
            revision: 1,
            visible: true,
            tasks: [makeUploadTask({ id: 1, fileName: "mobile-host.epub" })],
          })
        )
      })

      const dock = document.querySelector("[data-floating-dock]")
      expect(dock).not.toBeNull()
      expect(dock!.querySelector("[data-error-alerts]")).not.toBeNull()
      expect(dock!.querySelector("[data-upload-panel]")).toBeNull()
      expect(
        screen.queryByRole("button", { name: "Minimize upload panel" })
      ).not.toBeInTheDocument()
      expect(
        screen.queryByRole("button", { name: "Expand upload panel" })
      ).not.toBeInTheDocument()

      act(() => {
        openMobileUploadPanel()
      })

      const mobileDialog = screen.getByRole("dialog", { name: "Upload Queue" })
      expect(mobileDialog).toHaveAttribute("data-upload-panel", "")
      expect(screen.getByText("mobile-host.epub")).toBeInTheDocument()
      expect(screen.getByText("Mobile overlay warning")).toBeInTheDocument()
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
