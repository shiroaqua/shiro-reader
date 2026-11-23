import type { LibraryBridge } from "@/api/library"
import {
  CommandError,
  type ImportBooksResult,
  type ImportProgressEvent,
  type RestoreBooksResult,
} from "@/api/tauri-bridge"
import type {
  Book,
  BookTag,
  Tag,
  TagDefinitionInput,
  TagTreeNode,
  TrashedBook,
  UndefineTagStrategy,
} from "@/lib/types"
import {
  makeCommandErrorPayload,
  makeImportResult,
  nextId,
} from "./builders"
import type { FakeChannel } from "./mock-tauri"

type ImportHandler = (
  emit: (event: ImportProgressEvent) => void
) => Promise<ImportBooksResult>

export class FakeLibrary implements LibraryBridge {
  private books = new Map<string, Book>()
  private trash = new Map<string, TrashedBook>()
  private tags = new Map<string, Tag>()
  private bindings = new Map<string, Map<string, BookTag>>()
  private importHandler: ImportHandler = async () => makeImportResult()
  private cancelResult = false
  private coverBaseUrl = "shiro-cover://localhost/book/"

  seedBooks(items: readonly Book[]): this {
    for (const book of items) {
      this.books.set(book.id, { ...book })
    }
    return this
  }

  seedTrash(items: readonly TrashedBook[]): this {
    for (const item of items) {
      this.trash.set(item.id, { ...item })
    }
    return this
  }

  seedTags(nodes: readonly TagTreeNode[]): this {
    const insert = (items: readonly TagTreeNode[], parentId: string | null) => {
      for (const node of items) {
        const { children, ...tag } = node
        this.tags.set(tag.id, { ...tag, parent_id: parentId })
        insert(children, tag.id)
      }
    }
    insert(nodes, null)
    return this
  }

  seedBindings(bookId: string, items: readonly BookTag[]): this {
    const map = new Map<string, BookTag>()
    for (const binding of items) {
      map.set(binding.tag_id, { ...binding })
    }
    this.bindings.set(bookId, map)
    return this
  }

  setImportHandler(handler: ImportHandler): this {
    this.importHandler = handler
    return this
  }

  setCancelResult(result: boolean): this {
    this.cancelResult = result
    return this
  }

  setCoverBaseUrl(urlBase: string): this {
    this.coverBaseUrl = urlBase
    return this
  }

  getBooks(): Book[] {
    return [...this.books.values()].map((book) => ({ ...book }))
  }

  getTrash(): TrashedBook[] {
    return [...this.trash.values()].map((item) => ({ ...item }))
  }

  getTags(): Tag[] {
    return [...this.tags.values()].map((tag) => ({ ...tag }))
  }

  getBookTags(bookId: string): BookTag[] {
    return [...(this.bindings.get(bookId)?.values() ?? [])].map((item) => ({
      ...item,
    }))
  }

  booksList = async (): Promise<Book[]> => this.getBooks()

  bookRename = async (id: string, title: string): Promise<void> => {
    const current = this.requireBook(id)
    this.books.set(id, {
      ...current,
      title,
      updated_at: new Date().toISOString(),
    })
  }

  booksDelete = async (ids: string[]): Promise<void> => {
    const selected = ids.map((id) => this.requireBook(id))
    const deletedAt = new Date().toISOString()
    for (const book of selected) {
      this.books.delete(book.id)
      this.trash.set(book.id, { ...book, deleted_at: deletedAt })
    }
  }

  trashList = async (): Promise<TrashedBook[]> => this.getTrash()

  trashRestore = async (ids: string[]): Promise<RestoreBooksResult> => {
    const operationId = nextId("op")
    const restoredIds: string[] = []
    const failed: RestoreBooksResult["failed"] = []
    for (const id of ids) {
      const trashed = this.trash.get(id)
      if (!trashed) {
        failed.push({
          bookId: id,
          restored: false,
          error: makeCommandErrorPayload({
            operationId,
            key: "library.book.not_found",
            itemId: id,
          }),
        })
        continue
      }
      const { deleted_at: deletedAt, ...book } = trashed
      this.trash.delete(id)
      this.books.set(id, { ...book, updated_at: deletedAt })
      restoredIds.push(id)
    }
    return { operationId, restoredIds, failed, skippedIds: [] }
  }

  trashPermanentlyDelete = async (ids: string[]): Promise<void> => {
    for (const id of ids) {
      this.requireTrash(id)
    }
    for (const id of ids) {
      this.trash.delete(id)
      this.bindings.delete(id)
    }
  }

  tagsList = async (): Promise<TagTreeNode[]> => this.buildTagTree(null)

  tagDefine = async (definition: TagDefinitionInput): Promise<Tag> => {
    if (definition.parent_id) {
      this.requireTag(definition.parent_id)
    }
    const now = new Date().toISOString()
    const created: Tag = {
      ...definition,
      id: nextId("tag"),
      created_at: now,
      updated_at: now,
    }
    this.tags.set(created.id, created)
    return { ...created }
  }

  tagRename = async (id: string, name: string): Promise<void> => {
    const current = this.requireTag(id)
    this.tags.set(id, { ...current, name })
  }

  tagRecolor = async (id: string, color: string): Promise<void> => {
    const current = this.requireTag(id)
    this.tags.set(id, { ...current, color })
  }

  tagMove = async (id: string, parentId: string | null): Promise<void> => {
    const current = this.requireTag(id)
    let ancestor = parentId
    while (ancestor) {
      if (ancestor === id) {
        throw new CommandError(
          makeCommandErrorPayload({
            key: "library.tag.cycle",
            itemId: id,
          })
        )
      }
      ancestor = this.requireTag(ancestor).parent_id
    }
    this.tags.set(id, { ...current, parent_id: parentId })
  }

  tagUndefine = async (
    id: string,
    strategy: UndefineTagStrategy
  ): Promise<void> => {
    this.removeTag(id, strategy === "CASCADE")
  }

  bookTagsList = async (bookId: string): Promise<BookTag[]> => {
    this.requireBook(bookId)
    return this.getBookTags(bookId)
  }

  bookTagBind = async (bookId: string, binding: BookTag): Promise<void> => {
    this.requireBook(bookId)
    this.requireTag(binding.tag_id)
    const map = this.bindings.get(bookId) ?? new Map<string, BookTag>()
    map.set(binding.tag_id, { ...binding })
    this.bindings.set(bookId, map)
  }

  bookTagUnbind = async (bookId: string, tagId: string): Promise<void> => {
    this.requireBook(bookId)
    this.bindings.get(bookId)?.delete(tagId)
  }

  pickAndImportBooks = async (
    onEvent: (event: ImportProgressEvent) => void
  ): Promise<ImportBooksResult> => this.importHandler(onEvent)

  cancelBookImport = async (): Promise<boolean> => this.cancelResult

  coverUrl = (id: string): string | undefined =>
    `${this.coverBaseUrl}${encodeURIComponent(id)}`

  createTauriInvokeHandlers(): Record<
    string,
    (args?: Record<string, unknown>) => unknown
  > {
    return {
      books_list: () => this.booksList(),
      book_rename: (args) =>
        this.bookRename(String(args?.id), String(args?.title)),
      books_delete: (args) =>
        this.booksDelete((args?.ids as string[] | undefined) ?? []),
      trash_list: () => this.trashList(),
      trash_restore: (args) =>
        this.trashRestore((args?.ids as string[] | undefined) ?? []),
      trash_permanently_delete: (args) =>
        this.trashPermanentlyDelete((args?.ids as string[] | undefined) ?? []),
      tags_list: () => this.tagsList(),
      tag_define: (args) =>
        this.tagDefine({
          name: String(args?.name),
          parent_id: (args?.parentId as string | null | undefined) ?? null,
          color: String(args?.color),
          allows_value: Boolean(args?.allowsValue),
          value_regex: (args?.valueRegex as string | null | undefined) ?? null,
        }),
      tag_rename: (args) =>
        this.tagRename(String(args?.id), String(args?.name)),
      tag_recolor: (args) =>
        this.tagRecolor(String(args?.id), String(args?.color)),
      tag_move: (args) =>
        this.tagMove(
          String(args?.id),
          (args?.parentId as string | null | undefined) ?? null
        ),
      tag_undefine: (args) =>
        this.tagUndefine(
          String(args?.id),
          args?.strategy as UndefineTagStrategy
        ),
      book_tags_list: (args) => this.bookTagsList(String(args?.bookId)),
      book_tag_bind: (args) =>
        this.bookTagBind(String(args?.bookId), {
          tag_id: String(args?.tagId),
          value: (args?.value as string | null | undefined) ?? null,
        }),
      book_tag_unbind: (args) =>
        this.bookTagUnbind(String(args?.bookId), String(args?.tagId)),
      pick_and_import_books: (args) => {
        const channel = args?.onEvent as
          | FakeChannel<ImportProgressEvent>
          | undefined
        return this.pickAndImportBooks((event) => {
          channel?.onmessage(event)
        })
      },
      cancel_book_import: () => this.cancelBookImport(),
    }
  }

  private buildTagTree(parentId: string | null): TagTreeNode[] {
    return [...this.tags.values()]
      .filter((tag) => tag.parent_id === parentId)
      .map((tag) => ({
        ...tag,
        children: this.buildTagTree(tag.id),
      }))
  }

  private removeTag(id: string, cascade: boolean) {
    const current = this.requireTag(id)
    for (const child of [...this.tags.values()]) {
      if (child.parent_id !== id) continue
      if (cascade) {
        this.removeTag(child.id, true)
      } else {
        this.tags.set(child.id, { ...child, parent_id: current.parent_id })
      }
    }
    this.tags.delete(id)
    for (const items of this.bindings.values()) {
      items.delete(id)
    }
  }

  private requireBook(id: string): Book {
    const book = this.books.get(id)
    if (!book) {
      throw new CommandError(
        makeCommandErrorPayload({ key: "library.book.not_found", itemId: id })
      )
    }
    return book
  }

  private requireTrash(id: string): TrashedBook {
    const item = this.trash.get(id)
    if (!item) {
      throw new CommandError(
        makeCommandErrorPayload({ key: "library.book.not_found", itemId: id })
      )
    }
    return item
  }

  private requireTag(id: string): Tag {
    const tag = this.tags.get(id)
    if (!tag) {
      throw new CommandError(
        makeCommandErrorPayload({ key: "library.tag.not_found", itemId: id })
      )
    }
    return tag
  }
}
