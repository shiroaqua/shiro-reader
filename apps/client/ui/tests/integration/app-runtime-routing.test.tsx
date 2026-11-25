import { act, screen, waitFor } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import type {
  BootstrapState,
  StartupPhase,
} from "@shiro/ui-shared/api/tauri-bridge"
import { applyUploadSnapshot } from "@shiro/ui-shared/features/books/book-import-store"
import { booksQueryKey } from "@shiro/ui-shared/lib/query-keys"
import {
  cloudAuthRequiredState,
  cloudIncompatibleState,
  cloudUnreachableState,
  makeBook,
  makeCloudStatus,
  makeRustErrorEvent,
  makeStartupDiagnostic,
  makeUploadSnapshot,
  makeUploadTask,
  readyCloudState,
  readyLocalState,
  setupRequiredState,
  startupFailedState,
} from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { tauriTestHost } from "../support/mock-tauri"
import { renderDesktopApp, renderMobileApp } from "../support/render"

describe("App runtime routing and event synchronization", () => {
  describe("non-Tauri browser guards", () => {
    it("shows native-required notice when desktop runs outside Tauri without preview", () => {
      renderDesktopApp({ isTauri: false })

      expect(
        screen.getByText("Open ShiroReader in the installed app.")
      ).toBeInTheDocument()
    })

    it("renders DesktopDashboardScreen directly in preview mode", async () => {
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "b1", title: "Preview Book" }),
      ])

      renderDesktopApp({ isTauri: false, preview: true, library })

      expect(
        await screen.findByRole("heading", { name: "Bookshelf" })
      ).toBeInTheDocument()
      expect(await screen.findByText("Preview Book")).toBeInTheDocument()
    })

    it("shows native-required notice when mobile runs outside Tauri without preview", () => {
      renderMobileApp({ isTauri: false })

      expect(
        screen.getByText("Open ShiroReader in the installed app.")
      ).toBeInTheDocument()
    })

    it("renders MobileDashboardScreen directly in preview mode", async () => {
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "m-preview", title: "Mobile Preview Book" }),
      ])

      renderMobileApp({ isTauri: false, preview: true, library })

      expect(
        await screen.findByRole("heading", { name: "Bookshelf" })
      ).toBeInTheDocument()
      expect(
        await screen.findByText("Mobile Preview Book")
      ).toBeInTheDocument()
      expect(
        screen.getByRole("navigation", { name: "Main navigation" })
      ).toBeInTheDocument()
    })
  })

  describe("desktop bootstrap startup phase routing", () => {
    it.each<[StartupPhase, () => BootstrapState, string]>([
      ["setup_required", () => setupRequiredState(), "Welcome to ShiroReader"],
      ["startup_failed", () => startupFailedState(), "Library Startup Failed"],
      [
        "cloud_protocol_incompatible",
        () => cloudIncompatibleState(),
        "Incompatible Server Protocol",
      ],
      [
        "cloud_unreachable",
        () => cloudUnreachableState(),
        "Cloud Server Unreachable",
      ],
      [
        "cloud_auth_required",
        () => cloudAuthRequiredState(),
        "Sign In to Cloud Library",
      ],
      ["ready", () => readyLocalState(), "Bookshelf"],
    ])("routes desktop phase %s to its matching screen", async (_phase, buildState, heading) => {
      const library = new FakeLibrary()
      renderDesktopApp({ bootstrap: buildState(), library })

      expect(
        await screen.findByRole("heading", { name: heading })
      ).toBeInTheDocument()
    })
  })

  describe("platform isolation between desktop and mobile packages", () => {
    it("renders desktop sidebar layout and skips Android back registration on desktop", async () => {
      tauriTestHost.setPlatform("linux")
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "d1", title: "Desktop Book" }),
      ])

      renderDesktopApp({ bootstrap: readyLocalState(), library })

      expect(await screen.findByText("Desktop Book")).toBeInTheDocument()
      expect(document.querySelector('[data-sidebar="sidebar"]')).not.toBeNull()
      expect(
        screen.queryByRole("navigation", { name: "Main navigation" })
      ).not.toBeInTheDocument()
      expect(tauriTestHost.backListenerCount()).toBe(0)
    })

    it.each<[StartupPhase, () => BootstrapState, string]>([
      ["setup_required", () => setupRequiredState(), "Welcome to ShiroReader"],
      ["startup_failed", () => startupFailedState(), "Library Startup Failed"],
      [
        "cloud_protocol_incompatible",
        () => cloudIncompatibleState(),
        "Incompatible Server Protocol",
      ],
      [
        "cloud_unreachable",
        () => cloudUnreachableState(),
        "Cloud Server Unreachable",
      ],
      [
        "cloud_auth_required",
        () => cloudAuthRequiredState(),
        "Sign In to Cloud Library",
      ],
      ["ready", () => readyLocalState(), "Bookshelf"],
    ])(
      "routes mobile phase %s to its mobile screen",
      async (_phase, buildState, heading) => {
        tauriTestHost.setPlatform("android")
        const library = new FakeLibrary()
        renderMobileApp({
          bootstrap: buildState(),
          library,
        })

        expect(
          await screen.findByRole("heading", { name: heading })
        ).toBeInTheDocument()
        expect(tauriTestHost.backListenerCount()).toBe(1)
        expect(tauriTestHost.dragDropListenerCount()).toBe(0)
      }
    )

    it("always renders mobile bottom navigation without desktop sidebar or drag-drop on Android and handles Back", async () => {
      tauriTestHost.setPlatform("android")
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "m1", title: "Mobile Book" }),
      ])

      const { user } = renderMobileApp({
        bootstrap: readyLocalState({
          canPickLibraryDir: false,
          canImportBooks: true,
        }),
        library,
      })

      expect(await screen.findByText("Mobile Book")).toBeInTheDocument()
      expect(
        screen.getByRole("navigation", { name: "Main navigation" })
      ).toBeInTheDocument()
      expect(document.querySelector('[data-sidebar="sidebar"]')).toBeNull()
      expect(tauriTestHost.dragDropListenerCount()).toBe(0)

      // Opening Search and pressing Android Back closes Search cleanly.
      await user.click(screen.getByRole("button", { name: "Search" }))
      expect(screen.getByRole("button", { name: "Back" })).toBeInTheDocument()
      act(() => {
        tauriTestHost.triggerAndroidBack()
      })
      expect(
        screen.queryByRole("button", { name: "Back" })
      ).not.toBeInTheDocument()

      // Navigating to Trash and pressing Android Back returns to Bookshelf without exiting.
      await user.click(screen.getByRole("button", { name: "Trash" }))
      expect(
        await screen.findByRole("heading", { name: "Trash" })
      ).toBeInTheDocument()
      act(() => {
        tauriTestHost.triggerAndroidBack()
      })
      expect(
        await screen.findByRole("heading", { name: "Bookshelf" })
      ).toBeInTheDocument()
      expect(tauriTestHost.exitCalls).toHaveLength(0)
    })
  })

  describe("buffered error replay and live runtime events", () => {
    it("replays buffered errors while suppressing duplicates matching the active startup diagnostic", async () => {
      const diag = makeStartupDiagnostic({ errorId: "diag-1" })
      renderDesktopApp({
        bootstrap: startupFailedState({
          startupError: diag,
          bufferedErrors: [
            makeRustErrorEvent({
              errorId: "diag-1",
              message: "Duplicate diagnostic should stay hidden",
            }),
            makeRustErrorEvent({
              errorId: "extra-1",
              message: "Secondary buffered warning",
            }),
          ],
        }),
      })

      expect(
        await screen.findByText("Secondary buffered warning")
      ).toBeInTheDocument()
      expect(
        screen.queryByText("Duplicate diagnostic should stay hidden")
      ).not.toBeInTheDocument()
    })

    it("suppresses buffered errors matching cloud diagnostic when not ready", async () => {
      const cloudDiag = makeStartupDiagnostic({
        errorId: "cloud-diag-1",
        message: "Cloud probe failed",
      })
      renderDesktopApp({
        bootstrap: cloudUnreachableState({
          cloud: makeCloudStatus({
            origin: "https://books.example.com",
            diagnostic: cloudDiag,
          }),
          bufferedErrors: [
            makeRustErrorEvent({
              errorId: "cloud-diag-1",
              message: "Duplicate cloud diagnostic",
            }),
            makeRustErrorEvent({
              errorId: "cloud-extra",
              message: "Additional cloud event",
            }),
          ],
        }),
      })

      expect(
        await screen.findByText("Additional cloud event")
      ).toBeInTheDocument()
      expect(
        screen.queryByText("Duplicate cloud diagnostic")
      ).not.toBeInTheDocument()
    })

    it("clears React Query cache and resets active import when runtime-state leaves ready or switches cloud user", async () => {
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "b1", title: "Cloud Book" }),
      ])

      const { queryClient } = renderDesktopApp({
        bootstrap: readyCloudState({
          cloud: makeCloudStatus({
            origin: "https://books.example.com",
            user: { userId: "user-1", username: "alice" },
            hasSavedSession: true,
          }),
        }),
        library,
      })

      expect(await screen.findByText("Cloud Book")).toBeInTheDocument()
      expect(queryClient.getQueryData(booksQueryKey)).toBeDefined()

      // Populate an active upload snapshot so leaving ready must reset and hide the upload panel.
      act(() => {
        applyUploadSnapshot(
          makeUploadSnapshot({
            revision: 2,
            visible: true,
            tasks: [
              makeUploadTask({
                id: 1,
                fileName: "in-flight.epub",
                status: {
                  state: "active",
                  stage: "uploading",
                  progressPercent: 40,
                  bytesProcessed: 400,
                  totalBytes: 1000,
                  speedBytesPerSec: 100,
                  etaSeconds: 6,
                },
              }),
            ],
          })
        )
      })
      expect(await screen.findByText("in-flight.epub")).toBeInTheDocument()

      // Leaving ready -> transitions to CloudGateScreen and clears query cache + upload panel
      act(() => {
        tauriTestHost.emitRuntimeState(cloudAuthRequiredState())
      })
      expect(
        await screen.findByRole("heading", { name: "Sign In to Cloud Library" })
      ).toBeInTheDocument()
      expect(queryClient.getQueryData(booksQueryKey)).toBeUndefined()
      expect(screen.queryByText("in-flight.epub")).not.toBeInTheDocument()

      // Switching back to ready as a different user also clears stale cache cleanly
      const user2Library = new FakeLibrary().seedBooks([
        makeBook({ id: "b2", title: "Bob Book" }),
      ])
      tauriTestHost.attachLibrary(user2Library)
      queryClient.setQueryData(booksQueryKey, [
        makeBook({ id: "stale", title: "Stale Alice Book" }),
      ])

      act(() => {
        tauriTestHost.emitRuntimeState(
          readyCloudState({
            cloud: makeCloudStatus({
              origin: "https://books.example.com",
              user: { userId: "user-2", username: "bob" },
              hasSavedSession: true,
            }),
          })
        )
      })

      expect(await screen.findByText("Bob Book")).toBeInTheDocument()
      expect(screen.queryByText("Stale Alice Book")).not.toBeInTheDocument()
    })

    it("surfaces live shiro://error events in ApiErrorAlerts", async () => {
      renderDesktopApp({
        bootstrap: readyLocalState(),
        library: new FakeLibrary(),
      })
      await screen.findByRole("heading", { name: "Bookshelf" })

      act(() => {
        tauriTestHost.emitRustError(
          makeRustErrorEvent({
            errorId: "live-err-1",
            message: "Disk write failed",
          })
        )
      })

      await waitFor(() => {
        expect(screen.getByText("Disk write failed")).toBeInTheDocument()
      })
    })

    it("updates document.title and documentElement.lang with in-app locale", async () => {
      renderDesktopApp({
        bootstrap: readyLocalState({ locale: "en" }),
        library: new FakeLibrary(),
      })

      await screen.findByRole("heading", { name: "Bookshelf" })
      expect(document.title).toBe("ShiroReader")
      expect(document.documentElement.lang).toBe("en")

      act(() => {
        tauriTestHost.emitRuntimeState(readyLocalState({ locale: "zh" }))
      })

      await waitFor(() => {
        expect(document.title).toBe("白读")
        expect(document.documentElement.lang).toBe("zh-CN")
      })
    })
  })
})
