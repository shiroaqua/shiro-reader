import { StrictMode, useState } from "react"
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { setPreviewLibrary } from "@/api/library"
import { BookSearch } from "@/features/books/book-search"
import { BookshelfBooks } from "@/features/books/bookshelf-books"
import { DesktopDropUploadOverlay } from "@/features/books/desktop-drop-upload-overlay"
import { TrashList } from "@/features/books/trash-list"
import { UploadPanel } from "@/features/books/upload-panel"
import { getErrorNotifications } from "@/lib/error-notifications"
import {
  makeBook,
  makeBookTag,
  makeRustErrorEvent,
  makeTagNode,
  makeTrashedBook,
  makeUploadSnapshot,
  makeUploadTask,
  readyLocalState,
} from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { setScreenOrientation } from "../support/media"
import { tauriTestHost } from "../support/mock-tauri"
import { renderApp, renderWithProviders } from "../support/render"

function BookshelfHarness({ initialSearch = "" }: { initialSearch?: string }) {
  const [headerOverlay, setHeaderOverlay] = useState<HTMLDivElement | null>(
    null
  )
  const [search, setSearch] = useState(initialSearch)

  return (
    <div>
      <div ref={setHeaderOverlay} />
      <BookSearch value={search} onChange={setSearch} />
      <BookshelfBooks
        headerOverlay={headerOverlay}
        search={search}
        onSearchChange={setSearch}
      />
    </div>
  )
}

describe("Bookshelf, Trash, and Upload integration", () => {
  describe("BookshelfBooks", () => {
    it("shows empty state and recovers from load error on retry", async () => {
      const library = new FakeLibrary()
      tauriTestHost.attachLibrary(library)
      tauriTestHost.onInvoke("books_list", () => {
        tauriTestHost.attachLibrary(library)
        throw new Error("Transient read error")
      })

      const { user } = renderWithProviders(<BookshelfHarness />)

      expect(
        await screen.findByText("Failed to load books.")
      ).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Retry" }))
      expect(
        await screen.findByText("Bookshelf has no books yet.")
      ).toBeInTheDocument()
    })

    it("filters books by title text and #tag queries", async () => {
      const sciFi = makeTagNode({ id: "t-scifi", name: "SciFi" })
      const dune = makeBook({ id: "b1", title: "Dune" })
      const tea = makeBook({ id: "b2", title: "The Book of Tea" })

      const library = new FakeLibrary()
        .seedBooks([dune, tea])
        .seedTags([sciFi])
        .seedBindings("b1", [makeBookTag("t-scifi")])
      tauriTestHost.attachLibrary(library)

      const { user } = renderWithProviders(<BookshelfHarness />)

      expect(await screen.findByText("Dune")).toBeInTheDocument()
      expect(screen.getByText("The Book of Tea")).toBeInTheDocument()

      const bookList = screen.getByRole("list", { name: "Books" })
      expect(bookList.querySelector("time")).toBeNull()
      expect(screen.getByRole("heading", { name: "Dune" })).toHaveClass(
        "line-clamp-3"
      )

      const searchInput = screen.getByRole("combobox", { name: "Search books" })
      await user.type(searchInput, "#SciFi")

      await waitFor(() => {
        expect(screen.getByText("Dune")).toBeInTheDocument()
        expect(screen.queryByText("The Book of Tea")).not.toBeInTheDocument()
      })
    })

    it("supports single delete without confirmation and batch shift-range delete with confirmation", async () => {
      const b1 = makeBook({
        id: "b1",
        title: "Alpha",
        updated_at: "2026-01-03T00:00:00.000Z",
      })
      const b2 = makeBook({
        id: "b2",
        title: "Bravo",
        updated_at: "2026-01-02T00:00:00.000Z",
      })
      const b3 = makeBook({
        id: "b3",
        title: "Charlie",
        updated_at: "2026-01-01T00:00:00.000Z",
      })

      const library = new FakeLibrary().seedBooks([b1, b2, b3])
      tauriTestHost.attachLibrary(library)

      const { user } = renderWithProviders(<BookshelfHarness />)
      expect(await screen.findByText("Alpha")).toBeInTheDocument()

      // Single selection -> Delete acts immediately without AlertDialog
      await user.click(screen.getByRole("button", { name: 'Select "Alpha"' }))
      expect(screen.getByText("1 selected")).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Delete" }))
      await waitFor(() => {
        expect(screen.queryByText("Alpha")).not.toBeInTheDocument()
      })
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument()
      expect(library.getTrash().map((item) => item.id)).toEqual(["b1"])

      // Ctrl+A selects remaining books, Escape clears, then Shift+Click selects range
      await user.click(screen.getByRole("button", { name: 'Select "Bravo"' }))
      await user.keyboard("{Escape}")
      expect(screen.queryByRole("toolbar")).not.toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: 'Select "Bravo"' }))
      await user.keyboard("{Shift>}")
      await user.click(screen.getByRole("button", { name: 'Select "Charlie"' }))
      await user.keyboard("{/Shift}")
      expect(screen.getByText("2 selected")).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Delete" }))
      const confirmDialog = await screen.findByRole("alertdialog")
      expect(
        within(confirmDialog).getByText("Delete selected books?")
      ).toBeInTheDocument()

      await user.click(within(confirmDialog).getByRole("button", { name: "Delete" }))
      await waitFor(() => {
        expect(screen.getByText("Bookshelf has no books yet.")).toBeInTheDocument()
      })
      expect(library.getTrash()).toHaveLength(3)
    })
  })

  describe("TrashList", () => {
    it("restores trashed books back to bookshelf and permanently deletes after confirmation", async () => {
      const t1 = makeTrashedBook({ id: "b1", title: "Restorable Book" })
      const t2 = makeTrashedBook({ id: "b2", title: "Doomed Book" })
      const library = new FakeLibrary().seedTrash([t1, t2])
      tauriTestHost.attachLibrary(library)

      const { user } = renderWithProviders(<TrashList />)

      expect(await screen.findByText("Restorable Book")).toBeInTheDocument()
      expect(screen.getByText("Doomed Book")).toBeInTheDocument()

      const firstItem = screen.getByText("Restorable Book").closest("li")!
      await user.click(
        within(firstItem).getByRole("button", { name: "Restore" })
      )

      await waitFor(() => {
        expect(screen.queryByText("Restorable Book")).not.toBeInTheDocument()
      })
      expect(library.getBooks().map((b) => b.id)).toEqual(["b1"])

      const secondItem = screen.getByText("Doomed Book").closest("li")!
      await user.click(
        within(secondItem).getByRole("button", { name: "Permanently delete" })
      )

      const alertDialog = await screen.findByRole("alertdialog")
      await user.click(
        within(alertDialog).getByRole("button", { name: "Permanently delete" })
      )

      await waitFor(() => {
        expect(screen.getByText("Trash is empty.")).toBeInTheDocument()
      })
      expect(library.getTrash()).toHaveLength(0)
    })
  })

  describe("Concurrent Upload UI, Panel, Drag-Drop, Preview & Mobile Pruning", () => {
    it("allows appending uploads while active, switches expanded/minimized badges, adjusts concurrency, and handles inline retry/dismiss without global toast spam", async () => {
      let revision = 0
      const library = new FakeLibrary().seedBooks([
        makeBook({ id: "b-init", title: "Initial Book" }),
      ])

      library.setPickHandler(async () => {
        revision += 1
        if (revision === 1) {
          return makeUploadSnapshot({
            revision,
            visible: true,
            concurrency: 4,
            completedCounter: 0,
            tasks: [
              makeUploadTask({
                id: 1,
                fileName: "tea.epub",
                status: {
                  state: "active",
                  stage: "uploading",
                  progressPercent: 45,
                  bytesProcessed: 1024 * 1024,
                  totalBytes: 2 * 1024 * 1024,
                  speedBytesPerSec: 512 * 1024,
                  etaSeconds: 2,
                },
              }),
              makeUploadTask({
                id: 2,
                fileName: "corrupt.pdf",
                status: {
                  state: "failed",
                  error: makeRustErrorEvent({
                    code: 2106,
                    key: "library.book.file.unsupported",
                    message: "This book file format is not supported.",
                  }),
                },
              }),
            ],
          })
        }
        return makeUploadSnapshot({
          revision,
          visible: true,
          concurrency: 4,
          completedCounter: 0,
          tasks: [
            makeUploadTask({
              id: 1,
              fileName: "tea.epub",
              status: {
                state: "active",
                stage: "uploading",
                progressPercent: 60,
                bytesProcessed: 1200 * 1024,
                totalBytes: 2 * 1024 * 1024,
                speedBytesPerSec: 512 * 1024,
                etaSeconds: 2,
              },
            }),
            makeUploadTask({
              id: 2,
              fileName: "corrupt.pdf",
              status: {
                state: "failed",
                error: makeRustErrorEvent({
                  code: 2106,
                  key: "library.book.file.unsupported",
                  message: "This book file format is not supported.",
                }),
              },
            }),
            makeUploadTask({ id: 3, fileName: "appended.epub" }),
          ],
        })
      })

      const retrySpy = vi.spyOn(library, "uploadRetryTask")
      const dismissSpy = vi.spyOn(library, "uploadDismissTask")
      const concurrencySpy = vi.spyOn(library, "uploadSetConcurrency")

      const { user } = renderApp({
        bootstrap: readyLocalState(),
        library,
      })

      expect(await screen.findByText("Initial Book")).toBeInTheDocument()

      // First click enqueues active + failed tasks; button stays enabled for appending!
      const uploadBtn = screen.getByRole("button", { name: "Upload book" })
      await user.click(uploadBtn)

      expect(await screen.findByText("tea.epub")).toBeInTheDocument()
      expect(screen.getByText(/Uploading 45%/)).toBeInTheDocument()
      expect(
        screen.getByText("This book file format is not supported.")
      ).toBeInTheDocument()
      // Per-task failure must NOT flood global ApiErrorAlerts.
      expect(getErrorNotifications()).toHaveLength(0)

      // Upload button is NOT disabled during active upload -> clicking immediately appends!
      expect(uploadBtn).not.toBeDisabled()
      await user.click(uploadBtn)
      expect(await screen.findByText("appended.epub")).toBeInTheDocument()

      // Minimize to pulsing circular FAB and verify remaining + failed badges.
      await user.click(
        screen.getByRole("button", { name: "Minimize upload panel" })
      )
      const expandBtn = screen.getByRole("button", {
        name: "Expand upload panel",
      })
      expect(
        expandBtn.querySelector("[data-upload-badge-remaining]")
      ).toHaveTextContent("2")
      expect(
        expandBtn.querySelector("[data-upload-badge-failed]")
      ).toHaveTextContent("1")

      // Expand back and adjust concurrency via gear settings.
      await user.click(expandBtn)
      await user.click(
        screen.getByRole("button", { name: "Upload concurrency settings" })
      )
      const concurrencyInput = screen.getByLabelText(/Concurrent uploads/i)
      fireEvent.change(concurrencyInput, { target: { value: "8" } })
      expect(concurrencySpy).toHaveBeenCalledWith(8)

      // Retry and dismiss failed task inline.
      await user.click(screen.getByRole("button", { name: "Retry" }))
      expect(retrySpy).toHaveBeenCalledWith(2)

      // Re-emit a failed task and dismiss it.
      act(() => {
        tauriTestHost.emitUploadState(
          makeUploadSnapshot({
            revision: 99,
            visible: true,
            tasks: [
              makeUploadTask({
                id: 9,
                fileName: "bad2.pdf",
                status: {
                  state: "failed",
                  error: makeRustErrorEvent({ message: "Corrupt PDF" }),
                },
              }),
            ],
          })
        )
      })
      expect(await screen.findByText("Corrupt PDF")).toBeInTheDocument()
      await user.click(screen.getByRole("button", { name: "Dismiss" }))
      expect(dismissSpy).toHaveBeenCalledWith(9)

      // Bookshelf refreshes across batches when completedCounter advances.
      library.seedBooks([makeBook({ id: "b-new", title: "Fresh Upload" })])
      act(() => {
        tauriTestHost.emitUploadState(
          makeUploadSnapshot({
            revision: 100,
            visible: false,
            completedCounter: 1,
            stats: { total: 0, remaining: 0, active: 0, succeeded: 0, failed: 0 },
            tasks: [],
          })
        )
      })
      expect(await screen.findByText("Fresh Upload")).toBeInTheDocument()
    })

    it("caps rendered task items at 100 and shows overflow summary for massive queues", async () => {
      tauriTestHost.setTauriRuntime(false)
      const tasks = Array.from({ length: 125 }, (_, i) =>
        makeUploadTask({ id: i + 1, fileName: `book-${i + 1}.epub` })
      )
      const fake = new FakeLibrary().setUploadSnapshot(
        makeUploadSnapshot({ revision: 1, visible: true, tasks })
      )
      setPreviewLibrary(fake)

      renderWithProviders(<UploadPanel />)
      act(() => {
        fake.emitUploadSnapshot(
          makeUploadSnapshot({ revision: 2, visible: true, tasks })
        )
      })

      // Also apply directly via store since UploadPanel doesn't mount sync on its own.
      const { applyUploadSnapshot } = await import(
        "@/features/books/book-import-store"
      )
      act(() => {
        applyUploadSnapshot(
          makeUploadSnapshot({ revision: 3, visible: true, tasks })
        )
      })

      expect(document.querySelectorAll("[data-upload-task]")).toHaveLength(100)
      expect(
        screen.getByText("25 more queued for upload…")
      ).toBeInTheDocument()
    })

    it("prevents concurrency input async snap-back and rejects malformed integer suffixes", async () => {
      tauriTestHost.setTauriRuntime(false)
      let resolveFirst!: (snap: ReturnType<typeof makeUploadSnapshot>) => void
      let resolveSecond!: (snap: ReturnType<typeof makeUploadSnapshot>) => void
      const fake = new FakeLibrary()
      let callIndex = 0
      const setConcurrencySpy = vi
        .spyOn(fake, "uploadSetConcurrency")
        .mockImplementation(() => {
          callIndex += 1
          if (callIndex === 1) {
            return new Promise((resolve) => {
              resolveFirst = resolve
            })
          }
          return new Promise((resolve) => {
            resolveSecond = resolve
          })
        })
      setPreviewLibrary(fake)

      const { user } = renderWithProviders(<UploadPanel />)
      const { applyUploadSnapshot } = await import(
        "@/features/books/book-import-store"
      )
      act(() => {
        applyUploadSnapshot(
          makeUploadSnapshot({
            revision: 1,
            visible: true,
            concurrency: 4,
            tasks: [makeUploadTask({ id: 1, fileName: "queued.epub" })],
          })
        )
      })

      await user.click(
        screen.getByRole("button", { name: "Upload concurrency settings" })
      )
      const concurrencyInput = screen.getByLabelText(
        /Concurrent uploads/i
      ) as HTMLInputElement
      expect(concurrencyInput.value).toBe("4")

      // Typing "1" dispatches async IPC; input stays "1" instead of snapping back to "4".
      fireEvent.change(concurrencyInput, { target: { value: "1" } })
      expect(concurrencyInput.value).toBe("1")
      expect(setConcurrencySpy).toHaveBeenNthCalledWith(1, 1)

      // Typing "12" while the first IPC is unresolved yields "12", never "42".
      fireEvent.change(concurrencyInput, { target: { value: "12" } })
      expect(concurrencyInput.value).toBe("12")
      expect(setConcurrencySpy).toHaveBeenNthCalledWith(2, 12)

      // Stale first response must not overwrite active draft "12".
      await act(async () => {
        resolveFirst(
          makeUploadSnapshot({
            revision: 2,
            visible: true,
            concurrency: 1,
            tasks: [makeUploadTask({ id: 1, fileName: "queued.epub" })],
          })
        )
        await Promise.resolve()
      })
      expect(concurrencyInput.value).toBe("12")

      await act(async () => {
        resolveSecond(
          makeUploadSnapshot({
            revision: 3,
            visible: true,
            concurrency: 12,
            tasks: [makeUploadTask({ id: 1, fileName: "queued.epub" })],
          })
        )
        await Promise.resolve()
      })

      // Malformed string "2abc" is rejected (never parsed as 2) and blur reverts to 12.
      fireEvent.change(concurrencyInput, { target: { value: "2abc" } })
      expect(concurrencyInput.value).toBe("2abc")
      expect(setConcurrencySpy).toHaveBeenCalledTimes(2)

      fireEvent.blur(concurrencyInput)
      expect(concurrencyInput.value).toBe("12")
      expect(setConcurrencySpy).toHaveBeenCalledTimes(2)
    })

    it("enqueues dropped paths only once under React StrictMode and clears overlay on Escape/blur", async () => {
      tauriTestHost.setDeferDragDropRegistration(true)
      const enqueuePaths = vi.fn(async () =>
        makeUploadSnapshot({ revision: 1, visible: true, tasks: [] })
      )
      const library = new FakeLibrary().setEnqueuePathsHandler(enqueuePaths)
      tauriTestHost.attachLibrary(library)

      renderWithProviders(
        <StrictMode>
          <DesktopDropUploadOverlay enabled />
        </StrictMode>
      )

      // Flush deferred async listener registrations after StrictMode double-mount!
      await act(async () => {
        tauriTestHost.flushDragDropRegistrations()
        await Promise.resolve()
      })
      expect(tauriTestHost.dragDropListenerCount()).toBe(1)

      act(() => {
        tauriTestHost.emitDragDrop({
          type: "enter",
          paths: ["/books/a.epub"],
          position: { x: 10, y: 10 },
        })
      })
      expect(screen.getByText("Drop e-books to upload")).toBeInTheDocument()

      // Escape dismisses stuck overlay cleanly.
      fireEvent.keyDown(window, { key: "Escape" })
      expect(
        screen.queryByText("Drop e-books to upload")
      ).not.toBeInTheDocument()

      // Blur also clears overlay cleanly.
      act(() => {
        tauriTestHost.emitDragDrop({
          type: "over",
          position: { x: 20, y: 20 },
        })
      })
      expect(screen.getByText("Drop e-books to upload")).toBeInTheDocument()
      fireEvent(window, new Event("blur"))
      expect(
        screen.queryByText("Drop e-books to upload")
      ).not.toBeInTheDocument()

      // Drop invokes upload_enqueue_paths EXACTLY once.
      await act(async () => {
        tauriTestHost.emitDragDrop({
          type: "drop",
          paths: ["/books/a.epub", "/books/folder"],
          position: { x: 20, y: 20 },
        })
        await Promise.resolve()
      })
      expect(enqueuePaths).toHaveBeenCalledTimes(1)
      expect(enqueuePaths).toHaveBeenCalledWith([
        "/books/a.epub",
        "/books/folder",
      ])
    })

    it("supports dev:mock preview mode upload flow and shows desktop empty-bookshelf quick upload button", async () => {
      const library = new FakeLibrary()
      let rev = 0
      library.setPickHandler(async () => {
        rev += 1
        return makeUploadSnapshot({
          revision: rev,
          visible: true,
          tasks: [makeUploadTask({ id: 1, fileName: "Preview book 1.epub" })],
        })
      })
      setPreviewLibrary(library)

      const { user } = renderApp({ isTauri: false, preview: true })

      expect(
        await screen.findByText("Bookshelf has no books yet.")
      ).toBeInTheDocument()
      // Both toolbar and EmptyState show upload button on desktop.
      const uploadButtons = screen.getAllByRole("button", {
        name: "Upload book",
      })
      expect(uploadButtons.length).toBeGreaterThanOrEqual(2)

      await user.click(uploadButtons[0]!)
      expect(
        await screen.findByText("Preview book 1.epub")
      ).toBeInTheDocument()
    })

    it("hides ALL upload buttons, panel, and drop overlay on unsupported mobile platforms (e.g. iOS)", async () => {
      for (const orientation of ["portrait", "landscape"] as const) {
        tauriTestHost.reset()
        tauriTestHost.setPlatform("ios")
        setScreenOrientation(
          orientation === "portrait" ? "portrait-primary" : "landscape-primary"
        )
        const library = new FakeLibrary()

        const { unmount } = renderApp({
          bootstrap: readyLocalState({
            canPickLibraryDir: false,
            canImportBooks: false,
          }),
          library,
        })

        expect(
          await screen.findByText("Bookshelf has no books yet.")
        ).toBeInTheDocument()
        expect(
          screen.queryByRole("button", { name: "Upload book" })
        ).not.toBeInTheDocument()
        expect(document.querySelector("[data-upload-panel]")).toBeNull()
        expect(tauriTestHost.dragDropListenerCount()).toBe(0)

        // Even if a snapshot were somehow present, MobileUploadScreen stays hidden when canImportBooks is false.
        const { applyUploadSnapshot, openMobileUploadPanel } = await import(
          "@/features/books/book-import-store"
        )
        act(() => {
          applyUploadSnapshot(
            makeUploadSnapshot({
              revision: 1,
              visible: true,
              tasks: [makeUploadTask({ id: 1, fileName: "hidden.epub" })],
            })
          )
          openMobileUploadPanel()
        })
        expect(document.querySelector("[data-upload-panel]")).toBeNull()
        unmount()
      }
    })

    it("on Android opens full-screen upload UI ONLY via notification tap and closes via button or Back gesture", async () => {
      for (const orientation of ["portrait", "landscape"] as const) {
        tauriTestHost.reset()
        tauriTestHost.setPlatform("android")
        setScreenOrientation(
          orientation === "portrait" ? "portrait-primary" : "landscape-primary"
        )
        const library = new FakeLibrary()
        let rev = 0
        library.setPickHandler(async () => {
          rev += 1
          return makeUploadSnapshot({
            revision: rev,
            visible: true,
            stats: {
              total: 2,
              remaining: 1,
              active: 1,
              succeeded: 1,
              failed: 0,
            },
            tasks: [
              makeUploadTask({
                id: 1,
                fileName: "Done Android.epub",
                status: {
                  state: "succeeded",
                  bookId: "b-1",
                  title: "Done Android",
                },
              }),
              makeUploadTask({
                id: 2,
                fileName: "Active Android.epub",
                status: {
                  state: "active",
                  stage: "uploading",
                  progressPercent: 60,
                  bytesProcessed: 600,
                  totalBytes: 1000,
                  speedBytesPerSec: 1024,
                  etaSeconds: 1,
                },
              }),
            ],
          })
        })

        const { user, unmount } = renderApp({
          bootstrap: readyLocalState({
            canPickLibraryDir: false,
            canImportBooks: true,
          }),
          library,
        })

        expect(
          await screen.findByText("Bookshelf has no books yet.")
        ).toBeInTheDocument()
        // Upload button is available on Android (both toolbar and empty state), drag-drop stays disabled.
        const uploadButtons = screen.getAllByRole("button", {
          name: "Upload book",
        })
        expect(uploadButtons.length).toBeGreaterThanOrEqual(2)
        expect(tauriTestHost.dragDropListenerCount()).toBe(0)

        // Starting an upload must keep user on bookshelf and NOT render UploadPanel / FAB automatically.
        await user.click(uploadButtons[0]!)
        await act(async () => {
          await Promise.resolve()
        })
        expect(document.querySelector("[data-upload-panel]")).toBeNull()
        expect(
          screen.queryByRole("button", { name: "Expand upload panel" })
        ).not.toBeInTheDocument()

        // Tapping the Android notification emits shiro://upload-open-ui -> opens full-screen upload UI.
        act(() => {
          tauriTestHost.emitUploadOpenUi()
        })
        const panel = document.querySelector("[data-upload-panel]")
        expect(panel).not.toBeNull()
        expect(panel).toHaveClass("fixed", "inset-0")
        expect(screen.getByText("Done Android.epub")).toBeInTheDocument()
        expect(screen.getByText("Active Android.epub")).toBeInTheDocument()
        expect(library.mobileUiOpenHistory).toEqual([true])

        // Close button closes the full-screen UI and notifies backend.
        await user.click(
          screen.getByRole("button", { name: "Close upload screen" })
        )
        expect(document.querySelector("[data-upload-panel]")).toBeNull()
        expect(library.mobileUiOpenHistory).toEqual([true, false])

        // Re-open after completion (tap completed notification) and close via Android hardware Back gesture.
        rev += 1
        act(() => {
          tauriTestHost.emitUploadState(
            makeUploadSnapshot({
              revision: rev,
              visible: true,
              completedCounter: 2,
              stats: {
                total: 2,
                remaining: 0,
                active: 0,
                succeeded: 2,
                failed: 0,
              },
              tasks: [
                makeUploadTask({
                  id: 1,
                  fileName: "Done Android.epub",
                  status: {
                    state: "succeeded",
                    bookId: "b-1",
                    title: "Done Android",
                  },
                }),
                makeUploadTask({
                  id: 2,
                  fileName: "Active Android.epub",
                  status: {
                    state: "succeeded",
                    bookId: "b-2",
                    title: "Active Android",
                  },
                }),
              ],
            })
          )
          tauriTestHost.emitUploadOpenUi()
        })
        expect(document.querySelector("[data-upload-panel]")).not.toBeNull()
        expect(screen.getByText("2/2 processed")).toBeInTheDocument()

        act(() => {
          tauriTestHost.triggerAndroidBack()
        })
        expect(document.querySelector("[data-upload-panel]")).toBeNull()
        expect(tauriTestHost.exitCalls).toHaveLength(0)
        expect(library.mobileUiOpenHistory).toEqual([true, false, true, false])

        unmount()
      }
    })

    it("keeps underlying Settings dialog open and lets mobile upload inputs focus & close independently via Back", async () => {
      tauriTestHost.reset()
      tauriTestHost.setPlatform("android")
      setScreenOrientation("portrait-primary")
      const library = new FakeLibrary()

      const { user } = renderApp({
        bootstrap: readyLocalState({
          canPickLibraryDir: false,
          canImportBooks: true,
        }),
        library,
      })

      expect(
        await screen.findByText("Bookshelf has no books yet.")
      ).toBeInTheDocument()

      // Open Settings dialog from portrait header menu.
      await user.click(screen.getByRole("button", { name: "More" }))
      await user.click(await screen.findByText("Settings"))
      expect(await screen.findByText("Color Theme")).toBeInTheDocument()

      // Open full-screen upload screen via notification tap over the open Settings dialog.
      const overDialogSnap = makeUploadSnapshot({
        revision: 1,
        visible: true,
        concurrency: 4,
        stats: {
          total: 1,
          remaining: 1,
          active: 1,
          succeeded: 0,
          failed: 0,
        },
        tasks: [
          makeUploadTask({
            id: 1,
            fileName: "OverDialog.epub",
            status: {
              state: "active",
              stage: "uploading",
              progressPercent: 50,
              bytesProcessed: 500,
              totalBytes: 1000,
              speedBytesPerSec: 1024,
              etaSeconds: 1,
            },
          }),
        ],
      })
      act(() => {
        library.setUploadSnapshot(overDialogSnap)
        tauriTestHost.emitUploadState(overDialogSnap)
        tauriTestHost.emitUploadOpenUi()
      })

      expect(document.querySelector("[data-upload-panel]")).not.toBeNull()
      // Settings dialog stays mounted underneath.
      expect(screen.getByText("Color Theme")).toBeInTheDocument()

      // Focus concurrency settings & update concurrency without closing Settings dialog.
      await user.click(
        screen.getByRole("button", { name: "Upload concurrency settings" })
      )
      const concurrencyInput = (await screen.findByLabelText(
        /Concurrent uploads/i
      )) as HTMLInputElement
      await user.clear(concurrencyInput)
      await user.type(concurrencyInput, "8{Enter}")
      expect(concurrencyInput.value).toBe("8")
      expect(
        screen.getByRole("dialog", { name: "Upload Queue" })
      ).toBeInTheDocument()
      expect(screen.getByText("Color Theme")).toBeInTheDocument()

      // First Android Back closes ONLY MobileUploadScreen; Settings stays open.
      act(() => {
        tauriTestHost.triggerAndroidBack()
      })
      expect(document.querySelector("[data-upload-panel]")).toBeNull()
      expect(screen.getByText("Color Theme")).toBeInTheDocument()
      expect(library.mobileUiOpenHistory).toEqual([true, false])

      // Re-open via notification tap and verify Escape closes ONLY MobileUploadScreen too.
      act(() => {
        tauriTestHost.emitUploadOpenUi()
      })
      expect(
        screen.getByRole("dialog", { name: "Upload Queue" })
      ).toBeInTheDocument()
      await user.keyboard("{Escape}")
      expect(document.querySelector("[data-upload-panel]")).toBeNull()
      expect(screen.getByText("Color Theme")).toBeInTheDocument()

      // Next Android Back closes Settings dialog.
      act(() => {
        tauriTestHost.triggerAndroidBack()
      })
      expect(screen.queryByText("Color Theme")).not.toBeInTheDocument()
      expect(tauriTestHost.exitCalls).toHaveLength(0)
    })
  })
})
