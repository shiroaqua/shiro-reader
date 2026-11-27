import { useState } from "react"
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import { BookSearch } from "@shiro/ui-shared/features/books/book-search"
import { BookshelfBooks } from "@shiro/ui-shared/features/books/bookshelf-books"
import {
  formatBookDateTime,
  formatBookPublishedDate,
} from "@shiro/ui-shared/lib/format"
import {
  makeBook,
  makeBookMetadata,
  makeBookTag,
  makeCommandErrorPayload,
  makeTagNode,
  readyLocalState,
} from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { tauriTestHost } from "../support/mock-tauri"
import { renderMobileApp, renderWithProviders } from "../support/render"

function BookshelfHarness() {
  const [headerOverlay, setHeaderOverlay] = useState<HTMLDivElement | null>(
    null
  )
  const [search, setSearch] = useState("")

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

describe("BookPropertiesDialog integration", () => {
  it("opens from desktop right-click menu, displays application fields in view mode, and edits or clears changed fields", async () => {
    const book = makeBook({
      id: "b-epub",
      title: "Twelve Kingdoms",
      type: "epub",
      created_at: "2026-11-23T08:00:00.000Z",
      updated_at: "2026-11-27T08:00:00.000Z",
    })
    const pdfBook = makeBook({
      id: "b-pdf",
      title: "Sample PDF",
      type: "pdf",
      created_at: "2026-11-20T08:00:00.000Z",
      updated_at: "2026-11-21T08:00:00.000Z",
    })
    const epubMeta = makeBookMetadata({
      book_id: book.id,
      subtitle: "Shadow of the Moon",
      authors: ["Fuyumi Ono", "Akihiro Yamada"],
      publisher: "Kodansha",
      published_date: "2021-09-27",
      language: "zh-TW",
      description: "Classic fantasy epic",
      identifier: null,
      type: "EPUB",
      size: 10_905_190,
      page_count: null,
      created_at: book.created_at,
      updated_at: book.updated_at,
    })
    const pdfMeta = makeBookMetadata({
      book_id: pdfBook.id,
      type: "PDF",
      size: 2_097_152,
      page_count: 320,
      created_at: pdfBook.created_at,
      updated_at: pdfBook.updated_at,
    })

    const library = new FakeLibrary()
      .seedBooks([book, pdfBook])
      .seedMetadata([epubMeta, pdfMeta])
    tauriTestHost.attachLibrary(library)

    const { user } = renderWithProviders(<BookshelfHarness />)

    const heading = await screen.findByRole("heading", {
      name: "Twelve Kingdoms",
    })
    const card = heading.closest("li")!
    fireEvent.contextMenu(card)

    const propertiesItem = await screen.findByRole("menuitem", {
      name: "Properties",
    })
    await user.click(propertiesItem)

    const dialog = await screen.findByRole("dialog", { name: "Book Details" })
    const header = dialog.querySelector("header")!
    expect(within(header).getByText("Book Details")).toBeInTheDocument()
    const editButton = within(header).getByRole("button", { name: "Edit" })
    expect(editButton).toBeEnabled()

    // Cover hero displays Title -> Subtitle -> Author in exact order and omits "Book title" label.
    expect(within(dialog).queryByText("Book title")).not.toBeInTheDocument()
    const dl = dialog.querySelector("dl")!
    const heroColumn = dl.previousElementSibling!.lastElementChild as HTMLElement
    const heroTexts = Array.from(heroColumn.querySelectorAll("p")).map(
      (node) => node.textContent
    )
    expect(heroTexts).toEqual([
      "Twelve Kingdoms",
      "Shadow of the Moon",
      "Fuyumi Ono, Akihiro Yamada",
    ])

    // Property list starts with Description and does not duplicate Subtitle or Author below the hero.
    const dtLabels = Array.from(dl.querySelectorAll("dt")).map(
      (node) => node.textContent
    )
    expect(dtLabels).toEqual([
      "Description",
      "Language",
      "Publisher",
      "Identifier",
      "Format",
      "File size",
      "Published date",
      "Date added",
      "Date updated",
    ])
    expect(within(dl).queryByText("Subtitle")).not.toBeInTheDocument()
    expect(within(dl).queryByText("Author")).not.toBeInTheDocument()

    const publisherTerm = within(dl).getByText("Publisher")
    expect(publisherTerm.tagName).toBe("DT")
    expect(publisherTerm).toHaveClass("font-semibold")
    expect(publisherTerm.nextElementSibling?.tagName).toBe("DD")
    expect(publisherTerm.nextElementSibling).toHaveTextContent("Kodansha")
    expect(publisherTerm.nextElementSibling).not.toHaveClass("bg-muted/50")

    expect(
      within(dl).getByText("Published date").nextElementSibling
    ).toHaveTextContent(formatBookPublishedDate("2021-09-27", "en")!)
    // Server "zh-TW" is fuzzy-matched and translated to its display label.
    expect(within(dl).getByText("繁體中文")).toBeInTheDocument()
    expect(within(dl).queryByText("zh-TW")).not.toBeInTheDocument()
    expect(within(dl).getByText("Classic fantasy epic")).toBeInTheDocument()
    expect(within(dl).getByText("Unknown")).toBeInTheDocument()
    expect(within(dl).getByText("EPUB")).toBeInTheDocument()
    expect(within(dl).getByText("10.4 MB")).toBeInTheDocument()
    expect(
      within(dl).getByText("Date added").nextElementSibling
    ).toHaveTextContent(formatBookDateTime(book.created_at, "en"))
    expect(
      within(dl).getByText("Date updated").nextElementSibling
    ).toHaveTextContent(formatBookDateTime(book.updated_at, "en"))
    expect(within(dl).queryByText("Pages")).not.toBeInTheDocument()
    expect(within(dialog).queryByText("Series")).not.toBeInTheDocument()
    expect(
      within(dialog).queryByRole("button", { name: "Delete" })
    ).not.toBeInTheDocument()
    expect(
      within(dialog).queryByRole("button", { name: "Search" })
    ).not.toBeInTheDocument()
    // No tags bound -> Tags section is completely hidden.
    expect(
      within(dialog).queryByRole("region", { name: "Tags" })
    ).not.toBeInTheDocument()

    // Switch to Edit mode: hides read-only fields, bolds all labels, Save starts disabled.
    await user.click(editButton)
    expect(
      await screen.findByRole("dialog", { name: "Edit Metadata" })
    ).toBeInTheDocument()
    expect(within(dialog).queryByText("Format")).not.toBeInTheDocument()
    expect(within(dialog).queryByText("File size")).not.toBeInTheDocument()
    expect(within(dialog).queryByText("Date added")).not.toBeInTheDocument()
    expect(within(dialog).queryByText("Date updated")).not.toBeInTheDocument()

    const saveButton = within(dialog).getByRole("button", { name: "Save" })
    expect(saveButton).toBeDisabled()

    const titleInput = within(dialog).getByLabelText(/Book title/)
    const subtitleInput = within(dialog).getByLabelText("Subtitle")
    const author1Input = within(dialog).getByLabelText("Author 1")
    const author2Input = within(dialog).getByLabelText("Author 2")
    const publisherInput = within(dialog).getByLabelText("Publisher")
    const publishedDateInput = within(dialog).getByLabelText("Published date")
    const datePickerInput = within(dialog).getByLabelText("Pick date")
    const languageSelect = within(dialog).getByLabelText("Language")
    const identifierInput = within(dialog).getByLabelText("Identifier")
    const descriptionInput = within(dialog).getByLabelText("Description")

    for (const control of [
      titleInput,
      subtitleInput,
      publisherInput,
      publishedDateInput,
      languageSelect,
      identifierInput,
      descriptionInput,
    ]) {
      const label = dialog.querySelector(`label[for="${control.id}"]`)
      expect(label).not.toBeNull()
      expect(label).toHaveClass("font-semibold")
    }
    expect(author1Input).toHaveValue("Fuyumi Ono")
    expect(author2Input).toHaveValue("Akihiro Yamada")

    // Subtitle stays in the cover-row right column directly below Book title, while Author sits below the cover row.
    const editForm = dialog.querySelector("form")!
    const editHeroRow = editForm.firstElementChild as HTMLElement
    expect(within(editHeroRow).getByLabelText(/Book title/)).toBe(titleInput)
    expect(within(editHeroRow).getByLabelText("Subtitle")).toBe(subtitleInput)
    expect(within(editHeroRow).queryByLabelText("Author 1")).not.toBeInTheDocument()
    expect(titleInput.closest("div")?.nextElementSibling).toContainElement(subtitleInput)
    expect(
      Array.from(editForm.children)
        .slice(2)
        .map((field) => field.querySelector("label")?.textContent)
    ).toEqual([
      "Description",
      "Language",
      "Publisher",
      "Identifier",
      "Published date",
    ])

    // Adding an empty author row does not dirty the form.
    await user.click(within(dialog).getByRole("button", { name: "Add author" }))
    const author3Input = within(dialog).getByLabelText("Author 3")
    expect(author3Input).toHaveFocus()
    expect(saveButton).toBeDisabled()
    await user.click(within(dialog).getByRole("button", { name: "Remove author 3" }))
    expect(within(dialog).queryByLabelText("Author 3")).not.toBeInTheDocument()

    // Published date supports manual text input + auxiliary date picker; Language uses select matched to zh-Hant.
    expect(publishedDateInput).not.toHaveAttribute("type", "date")
    expect(publishedDateInput).toHaveValue("2021-09-27")
    expect(datePickerInput).toHaveAttribute("type", "date")
    expect(datePickerInput).toHaveValue("2021-09-27")
    expect(languageSelect.tagName).toBe("SELECT")
    expect(languageSelect).toHaveValue("zh-Hant")

    // Empty title disables Save.
    await user.clear(titleInput)
    expect(saveButton).toBeDisabled()

    await user.type(titleInput, "  Twelve Kingdoms Revised  ")
    await user.clear(subtitleInput)
    await user.type(subtitleInput, "  Sea of Wind  ")
    await user.click(within(dialog).getByRole("button", { name: "Remove author 2" }))
    await user.click(within(dialog).getByRole("button", { name: "Add author" }))
    await user.type(within(dialog).getByLabelText("Author 2"), "  Fuyumi Ono ")
    await user.click(within(dialog).getByRole("button", { name: "Add author" }))
    await user.type(within(dialog).getByLabelText("Author 3"), "  Natsumi  ")
    await user.clear(publisherInput)
    await user.type(publisherInput, "   ")
    // Pick a date via the auxiliary native date picker.
    fireEvent.change(datePickerInput, { target: { value: "2025-12-01" } })
    expect(publishedDateInput).toHaveValue("2025-12-01")
    await user.selectOptions(languageSelect, "ja")
    await user.type(identifierInput, " 9780306406157 ")
    expect(saveButton).toBeEnabled()

    tauriTestHost.clearInvokeCalls()
    await user.click(saveButton)

    // Returns to Book Details view after save completes.
    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })

    // Only changed fields are submitted: title renamed, publisher cleared, published_date & language & identifier changed;
    // unchanged description was never sent.
    expect(tauriTestHost.invokeCalls("book_rename").map((c) => c.args)).toEqual(
      [{ id: book.id, title: "Twelve Kingdoms Revised" }]
    )
    expect(
      tauriTestHost.invokeCalls("book_subtitle_change").map((c) => c.args)
    ).toEqual([{ id: book.id, subtitle: "Sea of Wind" }])
    expect(
      tauriTestHost.invokeCalls("book_authors_change").map((c) => c.args)
    ).toEqual([{ id: book.id, authors: ["Fuyumi Ono", "Natsumi"] }])
    expect(
      tauriTestHost.invokeCalls("book_publisher_clear").map((c) => c.args)
    ).toEqual([{ id: book.id }])
    expect(
      tauriTestHost.invokeCalls("book_published_date_change").map((c) => c.args)
    ).toEqual([{ id: book.id, publishedDate: "2025-12-01" }])
    expect(
      tauriTestHost.invokeCalls("book_language_change").map((c) => c.args)
    ).toEqual([{ id: book.id, language: "ja" }])
    expect(
      tauriTestHost.invokeCalls("book_identifier_change").map((c) => c.args)
    ).toEqual([{ id: book.id, identifier: "9780306406157" }])
    expect(tauriTestHost.invokeCalls("book_description_change")).toHaveLength(0)

    const updatedDl = dialog.querySelector("dl")!
    const updatedHero = updatedDl.previousElementSibling!
      .lastElementChild as HTMLElement
    expect(
      Array.from(updatedHero.querySelectorAll("p")).map((n) => n.textContent)
    ).toEqual([
      "Twelve Kingdoms Revised",
      "Sea of Wind",
      "Fuyumi Ono, Natsumi",
    ])
    expect(
      within(updatedDl).getByText("Published date").nextElementSibling
    ).toHaveTextContent(formatBookPublishedDate("2025-12-01", "en")!)
    expect(within(updatedDl).getByText("日本語")).toBeInTheDocument()
    expect(within(updatedDl).getByText("9780306406157")).toBeInTheDocument()

    // Edit again to clear subtitle, all authors, language & published date.
    await user.click(within(dialog).getByRole("button", { name: "Edit" }))
    await user.clear(within(dialog).getByLabelText("Subtitle"))
    await user.click(within(dialog).getByRole("button", { name: "Remove author 2" }))
    await user.click(within(dialog).getByRole("button", { name: "Remove author 1" }))
    await user.selectOptions(within(dialog).getByLabelText("Language"), "")
    await user.clear(within(dialog).getByLabelText("Published date"))
    tauriTestHost.clearInvokeCalls()
    await user.click(within(dialog).getByRole("button", { name: "Save" }))
    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })
    expect(
      tauriTestHost.invokeCalls("book_subtitle_clear").map((c) => c.args)
    ).toEqual([{ id: book.id }])
    expect(
      tauriTestHost.invokeCalls("book_authors_clear").map((c) => c.args)
    ).toEqual([{ id: book.id }])
    expect(
      tauriTestHost.invokeCalls("book_language_clear").map((c) => c.args)
    ).toEqual([{ id: book.id }])
    expect(
      tauriTestHost.invokeCalls("book_published_date_clear").map((c) => c.args)
    ).toEqual([{ id: book.id }])

    // Close and verify PDF hides unset Subtitle/Author in cover hero without duplicating them in <dl>, and shows Pages when non-null.
    await user.click(within(dialog).getByRole("button", { name: "Close" }))
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })

    const pdfHeading = screen.getByRole("heading", { name: "Sample PDF" })
    fireEvent.contextMenu(pdfHeading.closest("li")!)
    await user.click(
      await screen.findByRole("menuitem", { name: "Properties" })
    )
    const pdfDialog = await screen.findByRole("dialog", {
      name: "Book Details",
    })
    const pdfDl = pdfDialog.querySelector("dl")!
    const pdfHero = pdfDl.previousElementSibling!
      .lastElementChild as HTMLElement
    expect(
      Array.from(pdfHero.querySelectorAll("p")).map((n) => n.textContent)
    ).toEqual(["Sample PDF"])
    expect(
      Array.from(pdfDl.querySelectorAll("dt")).map((n) => n.textContent)
    ).toEqual([
      "Description",
      "Language",
      "Publisher",
      "Identifier",
      "Format",
      "File size",
      "Pages",
      "Published date",
      "Date added",
      "Date updated",
    ])
    expect(within(pdfDl).queryByText("Subtitle")).not.toBeInTheDocument()
    expect(within(pdfDl).queryByText("Author")).not.toBeInTheDocument()
    expect(within(pdfDl).getByText("Pages")).toBeInTheDocument()
    expect(within(pdfDl).getByText("320")).toBeInTheDocument()
  })

  it("displays created and updated timestamps with minutes and seconds using Chinese regional conventions", async () => {
    const createdAt = new Date(2026, 0, 2, 2, 11, 5).toISOString()
    const updatedAt = new Date(2026, 0, 3, 15, 4, 9).toISOString()
    const book = makeBook({
      id: "b-zh-time",
      title: "中文日期测试",
      created_at: createdAt,
      updated_at: updatedAt,
    })
    const meta = makeBookMetadata({
      book_id: book.id,
      published_date: "2021-09-27",
      created_at: createdAt,
      updated_at: updatedAt,
    })
    const library = new FakeLibrary().seedBooks([book]).seedMetadata([meta])
    tauriTestHost.attachLibrary(library)

    const { user } = renderWithProviders(<BookshelfHarness />, {
      locale: "zh",
    })
    const card = (
      await screen.findByRole("heading", { name: "中文日期测试" })
    ).closest("li")!
    fireEvent.contextMenu(card)
    await user.click(await screen.findByRole("menuitem", { name: "属性" }))

    const dialog = await screen.findByRole("dialog", { name: "书籍详情" })
    const dl = dialog.querySelector("dl")!
    expect(
      within(dl).getByText("出版日期").nextElementSibling
    ).toHaveTextContent("2021年9月27日")
    expect(
      within(dl).getByText("添加日期").nextElementSibling
    ).toHaveTextContent("2026年1月2日 02:11:05")
    expect(
      within(dl).getByText("更新日期").nextElementSibling
    ).toHaveTextContent("2026年1月3日 15:04:09")
  })

  it("retains edit mode and avoids resending already-committed fields after partial save failure", async () => {
    const book = makeBook({ id: "b-partial", title: "Original" })
    const meta = makeBookMetadata({
      book_id: book.id,
      publisher: null,
      identifier: null,
    })
    const library = new FakeLibrary().seedBooks([book]).seedMetadata([meta])
    tauriTestHost.attachLibrary(library)

    let failIdentifier = true
    const originalIdentifierChange = library.bookIdentifierChange
    tauriTestHost.onInvoke("book_identifier_change", async (args) => {
      if (failIdentifier) {
        throw new CommandError(
          makeCommandErrorPayload({
            code: 2015,
            key: "library.book.invalid_identifier",
            itemId: book.id,
          })
        )
      }
      return originalIdentifierChange(
        String(args?.id),
        String(args?.identifier)
      )
    })

    const { user } = renderWithProviders(<BookshelfHarness />)
    const card = (
      await screen.findByRole("heading", { name: "Original" })
    ).closest("li")!
    fireEvent.contextMenu(card)
    await user.click(
      await screen.findByRole("menuitem", { name: "Properties" })
    )

    const dialog = await screen.findByRole("dialog", { name: "Book Details" })
    await user.click(within(dialog).getByRole("button", { name: "Edit" }))

    await user.type(within(dialog).getByLabelText("Subtitle"), "Part I")
    await user.click(within(dialog).getByRole("button", { name: "Add author" }))
    await user.type(within(dialog).getByLabelText("Author 1"), "Alice")
    await user.type(within(dialog).getByLabelText("Publisher"), "Shiro Press")
    await user.type(within(dialog).getByLabelText("Identifier"), "bad-id")

    tauriTestHost.clearInvokeCalls()
    await user.click(within(dialog).getByRole("button", { name: "Save" }))

    await waitFor(() => {
      expect(tauriTestHost.invokeCalls("book_identifier_change")).toHaveLength(
        1
      )
    })
    expect(
      screen.getByRole("dialog", { name: "Edit Metadata" })
    ).toBeInTheDocument()
    expect(within(dialog).getByLabelText("Subtitle")).toHaveValue("Part I")
    expect(within(dialog).getByLabelText("Author 1")).toHaveValue("Alice")
    expect(within(dialog).getByLabelText("Publisher")).toHaveValue(
      "Shiro Press"
    )
    expect(within(dialog).getByLabelText("Identifier")).toHaveValue("bad-id")

    // Fix identifier and retry: subtitle, authors, and publisher were already committed and must NOT be resent.
    failIdentifier = false
    const idInput = within(dialog).getByLabelText("Identifier")
    await user.clear(idInput)
    await user.type(idInput, "9780306406157")

    tauriTestHost.clearInvokeCalls()
    await user.click(within(dialog).getByRole("button", { name: "Save" }))

    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })
    expect(tauriTestHost.invokeCalls("book_subtitle_change")).toHaveLength(0)
    expect(tauriTestHost.invokeCalls("book_authors_change")).toHaveLength(0)
    expect(tauriTestHost.invokeCalls("book_publisher_change")).toHaveLength(0)
    expect(
      tauriTestHost.invokeCalls("book_identifier_change").map((c) => c.args)
    ).toEqual([{ id: book.id, identifier: "9780306406157" }])
  })

  it("renders fullscreen on mobile from long-press menu and navigates back through edit -> details -> closed", async () => {
    tauriTestHost.setPlatform("android")
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => undefined)
    tauriTestHost.onInvoke("upload_get_state", () => ({
      revision: 0,
      visible: false,
      concurrency: 4,
      completedCounter: 0,
      stats: { total: 0, remaining: 0, active: 0, succeeded: 0, failed: 0 },
      hasDismissibleErrors: false,
      tasks: [],
    }))
    tauriTestHost.onInvoke("upload_consume_pending_open_ui", () => false)

    const book = makeBook({ id: "b-mobile", title: "Mobile Book" })
    const meta = makeBookMetadata({
      book_id: book.id,
      publisher: "Mobile Press",
    })
    const library = new FakeLibrary().seedBooks([book]).seedMetadata([meta])

    const { user } = renderMobileApp({
      bootstrap: readyLocalState(),
      library,
    })

    const heading = await screen.findByRole("heading", { name: "Mobile Book" })
    const card = heading.closest("li")!

    vi.useFakeTimers()
    act(() => {
      fireEvent.pointerDown(card, {
        pointerType: "touch",
        clientX: 50,
        clientY: 50,
      })
      vi.advanceTimersByTime(420)
      fireEvent.pointerUp(card, {
        pointerType: "touch",
        clientX: 50,
        clientY: 50,
      })
      fireEvent.click(card)
    })
    vi.useRealTimers()

    await user.click(await screen.findByRole("button", { name: "Properties" }))

    const dialog = await screen.findByRole("dialog", { name: "Book Details" })
    expect(dialog).toHaveClass("inset-0")
    expect(within(dialog).getByText("Mobile Press")).toBeInTheDocument()

    await user.click(within(dialog).getByRole("button", { name: "Edit" }))
    expect(
      await screen.findByRole("dialog", { name: "Edit Metadata" })
    ).toBeInTheDocument()

    // Android back inside edit mode returns to Book Details without closing dialog.
    act(() => {
      tauriTestHost.triggerAndroidBack()
    })
    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })
    expect(tauriTestHost.exitCalls).toEqual([])

    // Android back inside Book Details closes the screen without exiting the app.
    act(() => {
      tauriTestHost.triggerAndroidBack()
    })
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })
    expect(tauriTestHost.exitCalls).toEqual([])
  })

  it("supports manual partial dates and preserves unmatched server languages without false dirty state", async () => {
    const book = makeBook({ id: "b-partial-date", title: "Partial Meta Book" })
    const meta = makeBookMetadata({
      book_id: book.id,
      published_date: "2024-05",
      language: "tlh",
    })
    const library = new FakeLibrary().seedBooks([book]).seedMetadata([meta])
    tauriTestHost.attachLibrary(library)

    const { user } = renderWithProviders(<BookshelfHarness />)
    const card = (
      await screen.findByRole("heading", { name: "Partial Meta Book" })
    ).closest("li")!
    fireEvent.contextMenu(card)
    await user.click(
      await screen.findByRole("menuitem", { name: "Properties" })
    )

    const dialog = await screen.findByRole("dialog", { name: "Book Details" })
    expect(
      within(dialog).getByText(formatBookPublishedDate("2024-05", "en")!)
    ).toBeInTheDocument()
    expect(within(dialog).getByText("tlh")).toBeInTheDocument()

    await user.click(within(dialog).getByRole("button", { name: "Edit" }))
    const dateInput = within(dialog).getByLabelText("Published date")
    const pickerInput = within(dialog).getByLabelText("Pick date")
    expect(dateInput).toHaveValue("2024-05")
    expect(pickerInput).toHaveValue("")
    expect(within(dialog).getByLabelText("Language")).toHaveValue("tlh")
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled()

    // Editing only publisher must NOT clear partial published_date or unmatched language.
    await user.type(within(dialog).getByLabelText("Publisher"), "Press")
    tauriTestHost.clearInvokeCalls()
    await user.click(within(dialog).getByRole("button", { name: "Save" }))
    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })
    expect(tauriTestHost.invokeCalls("book_published_date_clear")).toHaveLength(
      0
    )
    expect(tauriTestHost.invokeCalls("book_language_clear")).toHaveLength(0)
    expect(
      within(dialog).getByText(formatBookPublishedDate("2024-05", "en")!)
    ).toBeInTheDocument()
    expect(within(dialog).getByText("tlh")).toBeInTheDocument()

    // Re-enter edit and manually update partial date to another partial year, then clear by deleting text.
    await user.click(within(dialog).getByRole("button", { name: "Edit" }))
    const editDateInput = within(dialog).getByLabelText("Published date")
    await user.clear(editDateInput)
    await user.type(editDateInput, "2023")
    tauriTestHost.clearInvokeCalls()
    await user.click(within(dialog).getByRole("button", { name: "Save" }))
    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })
    expect(
      tauriTestHost.invokeCalls("book_published_date_change").map((c) => c.args)
    ).toEqual([{ id: book.id, publishedDate: "2023" }])

    await user.click(within(dialog).getByRole("button", { name: "Edit" }))
    await user.clear(within(dialog).getByLabelText("Published date"))
    tauriTestHost.clearInvokeCalls()
    await user.click(within(dialog).getByRole("button", { name: "Save" }))
    await waitFor(() => {
      expect(
        screen.getByRole("dialog", { name: "Book Details" })
      ).toBeInTheDocument()
    })
    expect(
      tauriTestHost.invokeCalls("book_published_date_clear").map((c) => c.args)
    ).toEqual([{ id: book.id }])
  })

  it("displays bound tags at the bottom of Book Details and jumps to tag search on click across desktop and mobile", async () => {
    const sciFi = makeTagNode({ id: "t-scifi", name: "SciFi", color: "#75a9cc" })
    const author = makeTagNode({
      id: "t-author",
      name: "Author",
      color: "#a4bd69",
      value_type: "text",
    })
    const dune = makeBook({ id: "b-dune", title: "Dune" })
    const tea = makeBook({ id: "b-tea", title: "The Book of Tea" })

    const desktopLib = new FakeLibrary()
      .seedBooks([dune, tea])
      .seedTags([sciFi, author])
      .seedBindings("b-dune", [
        makeBookTag("t-scifi"),
        makeBookTag("t-author", "Frank Herbert"),
      ])
    tauriTestHost.attachLibrary(desktopLib)

    const { user: desktopUser, unmount: unmountDesktop } = renderWithProviders(
      <BookshelfHarness />
    )
    const duneCard = (
      await screen.findByRole("heading", { name: "Dune" })
    ).closest("li")!
    fireEvent.contextMenu(duneCard)
    await desktopUser.click(
      await screen.findByRole("menuitem", { name: "Properties" })
    )

    const desktopDialog = await screen.findByRole("dialog", {
      name: "Book Details",
    })
    const tagsSection = await within(desktopDialog).findByRole("region", {
      name: "Tags",
    })
    const sciFiButton = within(tagsSection).getByRole("button", {
      name: /SciFi/,
    })
    const authorButton = within(tagsSection).getByRole("button", {
      name: /Author.*Frank Herbert/,
    })
    expect(sciFiButton).toBeInTheDocument()
    expect(authorButton).toBeInTheDocument()

    // Clicking a valued tag closes Book Details and filters the bookshelf via BookSearch.
    await desktopUser.click(authorButton)
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Dune" })).toBeInTheDocument()
      expect(
        screen.queryByRole("heading", { name: "The Book of Tea" })
      ).not.toBeInTheDocument()
    })
    expect(
      screen.getAllByRole("button", {
        name: "Remove filter Author: Frank Herbert",
      }).length
    ).toBeGreaterThanOrEqual(1)

    unmountDesktop()

    // Mobile: tapping a tag in Book Details closes details, opens the mobile search bar, and filters books.
    tauriTestHost.reset()
    tauriTestHost.setPlatform("android")
    tauriTestHost.onInvoke("trigger_long_press_haptic", () => undefined)
    tauriTestHost.onInvoke("upload_get_state", () => ({
      revision: 0,
      visible: false,
      concurrency: 4,
      completedCounter: 0,
      stats: { total: 0, remaining: 0, active: 0, succeeded: 0, failed: 0 },
      hasDismissibleErrors: false,
      tasks: [],
    }))
    tauriTestHost.onInvoke("upload_consume_pending_open_ui", () => false)

    const mobileLib = new FakeLibrary()
      .seedBooks([dune, tea])
      .seedTags([sciFi, author])
      .seedBindings("b-dune", [makeBookTag("t-scifi")])

    const { user: mobileUser } = renderMobileApp({
      bootstrap: readyLocalState(),
      library: mobileLib,
    })

    const mobileCard = (
      await screen.findByRole("heading", { name: "Dune" })
    ).closest("li")!
    vi.useFakeTimers()
    act(() => {
      fireEvent.pointerDown(mobileCard, {
        pointerType: "touch",
        clientX: 50,
        clientY: 50,
      })
      vi.advanceTimersByTime(420)
      fireEvent.pointerUp(mobileCard, {
        pointerType: "touch",
        clientX: 50,
        clientY: 50,
      })
      fireEvent.click(mobileCard)
    })
    vi.useRealTimers()

    await mobileUser.click(
      await screen.findByRole("button", { name: "Properties" })
    )
    const mobileDialog = await screen.findByRole("dialog", {
      name: "Book Details",
    })
    const mobileTagsSection = await within(mobileDialog).findByRole("region", {
      name: "Tags",
    })
    await mobileUser.click(
      within(mobileTagsSection).getByRole("button", { name: /SciFi/ })
    )

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })
    expect(
      await screen.findByRole("combobox", { name: "Search books" })
    ).toBeInTheDocument()
    await waitFor(() => {
      expect(
        screen.getAllByRole("button", { name: "Remove filter SciFi" }).length
      ).toBeGreaterThanOrEqual(1)
    })
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Dune" })).toBeInTheDocument()
      expect(
        screen.queryByRole("heading", { name: "The Book of Tea" })
      ).not.toBeInTheDocument()
    })
  })
})
