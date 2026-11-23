import { useState } from "react"
import { screen, waitFor, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { BookSearch } from "@/features/books/book-search"
import { BookshelfBooks } from "@/features/books/bookshelf-books"
import { TrashList } from "@/features/books/trash-list"
import { UploadBookButton } from "@/features/books/upload-book-button"
import {
  makeBook,
  makeBookTag,
  makeCommandErrorPayload,
  makeImportResult,
  makeTagNode,
  makeTrashedBook,
} from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { tauriTestHost } from "../support/mock-tauri"
import { renderWithProviders } from "../support/render"

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

  describe("UploadBookButton", () => {
    it("streams import progress, allows cancelling, and displays failure/skip summary", async () => {
      let emitEvent!: Parameters<FakeLibrary["pickAndImportBooks"]>[0]
      let resolveImport!: (result: ReturnType<typeof makeImportResult>) => void

      const library = new FakeLibrary()
        .setCancelResult(true)
        .setImportHandler(
          (emit) =>
            new Promise((resolve) => {
              emitEvent = emit
              resolveImport = resolve
            })
        )
      const cancelSpy = vi.spyOn(library, "cancelBookImport")
      tauriTestHost.attachLibrary(library)

      const { user } = renderWithProviders(<UploadBookButton />)

      await user.click(screen.getByRole("button", { name: "Upload book" }))

      emitEvent({ stage: "started", operationId: "op-1", total: 2 })
      emitEvent({
        stage: "itemProgress",
        operationId: "op-1",
        index: 0,
        total: 2,
        fileName: "tea.epub",
        bytesRead: 50,
        totalBytes: 100,
      })

      expect(await screen.findByText("tea.epub · 50%")).toBeInTheDocument()
      const cancelBtn = screen.getByRole("button", { name: "Cancel import" })
      await user.click(cancelBtn)
      expect(cancelSpy).toHaveBeenCalledTimes(1)

      resolveImport(
        makeImportResult({
          failed: [
            {
              fileName: "corrupt.pdf",
              error: makeCommandErrorPayload({
                key: "library.book.invalid_file",
              }),
            },
          ],
          skipped: ["duplicate.epub"],
          summaryMessage: "1 failed, 1 skipped",
        })
      )

      expect(await screen.findByText("1 failed, 1 skipped")).toBeInTheDocument()
      expect(screen.getByText("Failed corrupt.pdf")).toBeInTheDocument()
      expect(screen.getByText("Skipped duplicate.epub")).toBeInTheDocument()
    })
  })
})
