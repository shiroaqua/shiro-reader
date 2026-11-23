import { act, screen, waitFor } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import type { BootstrapState, StartupPhase } from "@/api/tauri-bridge"
import { startBookImport } from "@/features/books/book-import-store"
import { booksQueryKey } from "@/lib/query-keys"
import {
  cloudAuthRequiredState,
  cloudIncompatibleState,
  cloudUnreachableState,
  makeBook,
  makeCloudStatus,
  makeImportResult,
  makeRustErrorEvent,
  makeStartupDiagnostic,
  readyCloudState,
  readyLocalState,
  setupRequiredState,
  startupFailedState,
} from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { tauriTestHost } from "../support/mock-tauri"
import { renderApp } from "../support/render"

describe("App runtime routing and event synchronization", () => {
  describe("non-Tauri browser guards", () => {
    it("shows native-required notice when running outside Tauri without preview", () => {
      renderApp({ isTauri: false })

      expect(
        screen.getByText("Open ShiroReader in the installed app.")
      ).toBeInTheDocument()
    })

    it("renders DashboardScreen directly in preview mode", async () => {
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "b1", title: "Preview Book" }),
      ])

      renderApp({ isTauri: false, preview: true, library })

      expect(
        await screen.findByRole("heading", { name: "Bookshelf" })
      ).toBeInTheDocument()
      expect(await screen.findByText("Preview Book")).toBeInTheDocument()
    })
  })

  describe("bootstrap startup phase routing", () => {
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
    ])("routes phase %s to its matching screen", async (_phase, buildState, heading) => {
      const library = new FakeLibrary()
      renderApp({ bootstrap: buildState(), library })

      expect(
        await screen.findByRole("heading", { name: heading })
      ).toBeInTheDocument()
    })
  })

  describe("buffered error replay and live runtime events", () => {
    it("replays buffered errors while suppressing duplicates matching the active startup diagnostic", async () => {
      const diag = makeStartupDiagnostic({ errorId: "diag-1" })
      renderApp({
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
      renderApp({
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
      const library = new FakeLibrary()
        .seedBooks([makeBook({ id: "b1", title: "Cloud Book" })])
        .setImportHandler(() => new Promise(() => {}))

      const { queryClient } = renderApp({
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

      // Start an in-flight import so leaving ready must reset it
      act(() => {
        void startBookImport(queryClient)
      })
      expect(
        await screen.findByRole("button", { name: /Uploading/i })
      ).toBeDisabled()

      // Leaving ready -> transitions to CloudGateScreen and clears query cache + import state
      act(() => {
        tauriTestHost.emitRuntimeState(cloudAuthRequiredState())
      })
      expect(
        await screen.findByRole("heading", { name: "Sign In to Cloud Library" })
      ).toBeInTheDocument()
      expect(queryClient.getQueryData(booksQueryKey)).toBeUndefined()

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

      // Clean up import store with a resolved handler if needed by afterEach
      library.setImportHandler(async () => makeImportResult())
    })

    it("surfaces live shiro://error events in ApiErrorAlerts", async () => {
      renderApp({ bootstrap: readyLocalState(), library: new FakeLibrary() })
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
      renderApp({
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
