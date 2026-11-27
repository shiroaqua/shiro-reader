import type { LibraryBridge } from "@shiro/ui-shared/api/library"
import {
  CommandError,
  type RestoreBooksResult,
  type UploadPanelSnapshot,
} from "@shiro/ui-shared/api/tauri-bridge"
import type {
  Book,
  BookMetadata,
  BookTag,
  Tag,
  TagDefinitionInput,
  TagTreeNode,
  TagValueType,
  TrashedBook,
  UndefineTagStrategy,
} from "@shiro/ui-shared/lib/types"
import {
  makeCommandErrorPayload,
  makeUploadSnapshot,
  nextId,
} from "./builders"

type UploadListener = (snapshot: UploadPanelSnapshot) => void
type UploadOpenUiListener = () => void

export class FakeLibrary implements LibraryBridge {
  private books = new Map<string, Book>()
  private metadata = new Map<string, BookMetadata>()
  private trash = new Map<string, TrashedBook>()
  private tags = new Map<string, Tag>()
  private bindings = new Map<string, Map<string, BookTag>>()
  private uploadListeners = new Set<UploadListener>()
  private uploadOpenUiListeners = new Set<UploadOpenUiListener>()
  private pendingOpenUi = false
  mobileUiOpenHistory: boolean[] = []
  private uploadSnapshot: UploadPanelSnapshot = makeUploadSnapshot({
    revision: 0,
    visible: false,
    tasks: [],
  })
  private pickHandler?: () => Promise<UploadPanelSnapshot>
  private enqueuePathsHandler?: (
    paths: string[]
  ) => Promise<UploadPanelSnapshot>
  private coverBaseUrl = "shiro-cover://localhost/book/"

  seedBooks(items: readonly Book[]): this {
    for (const book of items) {
      this.books.set(book.id, { ...book })
      if (!this.metadata.has(book.id)) {
        this.metadata.set(book.id, {
          book_id: book.id,
          subtitle: null,
          authors: [],
          publisher: null,
          published_date: null,
          language: null,
          description: null,
          identifier: null,
          type: book.type.toUpperCase(),
          size: 1_048_576,
          page_count: book.type.toLowerCase() === "pdf" ? 10 : null,
          created_at: book.created_at,
          updated_at: book.updated_at,
        })
      }
    }
    return this
  }

  seedMetadata(items: readonly BookMetadata[]): this {
    for (const item of items) {
      this.metadata.set(item.book_id, {
        ...item,
        authors: [...item.authors],
      })
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

  setUploadSnapshot(snapshot: UploadPanelSnapshot): this {
    this.uploadSnapshot = snapshot
    return this
  }

  emitUploadSnapshot(snapshot: UploadPanelSnapshot): this {
    this.uploadSnapshot = snapshot
    for (const listener of [...this.uploadListeners]) {
      listener(snapshot)
    }
    return this
  }

  setPendingOpenUi(pending: boolean): this {
    this.pendingOpenUi = pending
    return this
  }

  emitUploadOpenUi(): this {
    for (const listener of [...this.uploadOpenUiListeners]) {
      listener()
    }
    return this
  }

  setPickHandler(handler: () => Promise<UploadPanelSnapshot>): this {
    this.pickHandler = handler
    return this
  }

  setEnqueuePathsHandler(
    handler: (paths: string[]) => Promise<UploadPanelSnapshot>
  ): this {
    this.enqueuePathsHandler = handler
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
    const updatedAt = new Date().toISOString()
    this.books.set(id, {
      ...current,
      title,
      updated_at: updatedAt,
    })
    const meta = this.metadata.get(id)
    if (meta) {
      this.metadata.set(id, { ...meta, updated_at: updatedAt })
    }
  }

  bookMetadataGet = async (id: string): Promise<BookMetadata> => {
    const book = this.requireBook(id)
    const existing = this.metadata.get(id)
    if (existing) {
      return { ...existing, authors: [...existing.authors] }
    }
    const fallback: BookMetadata = {
      book_id: book.id,
      subtitle: null,
      authors: [],
      publisher: null,
      published_date: null,
      language: null,
      description: null,
      identifier: null,
      type: book.type.toUpperCase(),
      size: 1_048_576,
      page_count: book.type.toLowerCase() === "pdf" ? 10 : null,
      created_at: book.created_at,
      updated_at: book.updated_at,
    }
    this.metadata.set(id, fallback)
    return { ...fallback, authors: [] }
  }

  private async updateMetadataField(
    id: string,
    field:
      | "subtitle"
      | "publisher"
      | "published_date"
      | "language"
      | "description"
      | "identifier",
    unchangedKey: string,
    nextValue: string | null
  ): Promise<void> {
    const current = await this.bookMetadataGet(id)
    if (current[field] === nextValue) {
      throw new CommandError(
        makeCommandErrorPayload({ key: unchangedKey, itemId: id })
      )
    }
    this.metadata.set(id, { ...current, [field]: nextValue })
  }

  bookSubtitleChange = (id: string, subtitle: string) =>
    this.updateMetadataField(
      id,
      "subtitle",
      "library.book.subtitle_unchanged",
      subtitle
    )

  bookSubtitleClear = (id: string) =>
    this.updateMetadataField(
      id,
      "subtitle",
      "library.book.subtitle_unchanged",
      null
    )

  bookAuthorsChange = async (id: string, authors: string[]): Promise<void> => {
    const current = await this.bookMetadataGet(id)
    if (
      current.authors.length === authors.length &&
      current.authors.every((item, index) => item === authors[index])
    ) {
      throw new CommandError(
        makeCommandErrorPayload({
          key: "library.book.authors_unchanged",
          itemId: id,
        })
      )
    }
    this.metadata.set(id, { ...current, authors: [...authors] })
  }

  bookAuthorsClear = async (id: string): Promise<void> => {
    const current = await this.bookMetadataGet(id)
    if (current.authors.length === 0) {
      throw new CommandError(
        makeCommandErrorPayload({
          key: "library.book.authors_unchanged",
          itemId: id,
        })
      )
    }
    this.metadata.set(id, { ...current, authors: [] })
  }

  bookPublisherChange = (id: string, publisher: string) =>
    this.updateMetadataField(
      id,
      "publisher",
      "library.book.publisher_unchanged",
      publisher
    )

  bookPublisherClear = (id: string) =>
    this.updateMetadataField(
      id,
      "publisher",
      "library.book.publisher_unchanged",
      null
    )

  bookPublishedDateChange = (id: string, publishedDate: string) =>
    this.updateMetadataField(
      id,
      "published_date",
      "library.book.published_date_unchanged",
      publishedDate
    )

  bookPublishedDateClear = (id: string) =>
    this.updateMetadataField(
      id,
      "published_date",
      "library.book.published_date_unchanged",
      null
    )

  bookLanguageChange = (id: string, language: string) =>
    this.updateMetadataField(
      id,
      "language",
      "library.book.language_unchanged",
      language
    )

  bookLanguageClear = (id: string) =>
    this.updateMetadataField(
      id,
      "language",
      "library.book.language_unchanged",
      null
    )

  bookDescriptionChange = (id: string, description: string) =>
    this.updateMetadataField(
      id,
      "description",
      "library.book.description_unchanged",
      description
    )

  bookDescriptionClear = (id: string) =>
    this.updateMetadataField(
      id,
      "description",
      "library.book.description_unchanged",
      null
    )

  bookIdentifierChange = (id: string, identifier: string) =>
    this.updateMetadataField(
      id,
      "identifier",
      "library.book.identifier_unchanged",
      identifier
    )

  bookIdentifierClear = (id: string) =>
    this.updateMetadataField(
      id,
      "identifier",
      "library.book.identifier_unchanged",
      null
    )

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
      this.metadata.delete(id)
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
      value_regex: null,
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

  listenUploadState = async (
    handler: (snapshot: UploadPanelSnapshot) => void
  ): Promise<() => void> => {
    this.uploadListeners.add(handler)
    return () => {
      this.uploadListeners.delete(handler)
    }
  }

  listenUploadOpenUi = async (handler: () => void): Promise<() => void> => {
    this.uploadOpenUiListeners.add(handler)
    return () => {
      this.uploadOpenUiListeners.delete(handler)
    }
  }

  uploadConsumePendingOpenUi = async (): Promise<boolean> => {
    const pending = this.pendingOpenUi
    this.pendingOpenUi = false
    return pending
  }

  uploadSetMobileUiOpen = async (open: boolean): Promise<void> => {
    this.mobileUiOpenHistory.push(open)
  }

  uploadGetState = async (): Promise<UploadPanelSnapshot> => this.uploadSnapshot

  uploadPickAndEnqueue = async (): Promise<UploadPanelSnapshot> => {
    if (this.pickHandler) {
      const next = await this.pickHandler()
      this.emitUploadSnapshot(next)
      return next
    }
    return this.uploadSnapshot
  }

  uploadEnqueuePaths = async (
    paths: string[]
  ): Promise<UploadPanelSnapshot> => {
    if (this.enqueuePathsHandler) {
      const next = await this.enqueuePathsHandler(paths)
      this.emitUploadSnapshot(next)
      return next
    }
    return this.uploadSnapshot
  }

  uploadRetryTask = async (taskId: number): Promise<UploadPanelSnapshot> => {
    const tasks = this.uploadSnapshot.tasks.map((task) =>
      task.id === taskId && task.status.state === "failed"
        ? { ...task, status: { state: "queued" as const } }
        : task
    )
    const next = makeUploadSnapshot({
      ...this.uploadSnapshot,
      revision: this.uploadSnapshot.revision + 1,
      tasks,
      completedCounter: this.uploadSnapshot.completedCounter,
    })
    this.emitUploadSnapshot(next)
    return next
  }

  uploadCancelTask = async (taskId: number): Promise<UploadPanelSnapshot> => {
    const tasks = this.uploadSnapshot.tasks.filter((task) => task.id !== taskId)
    const next = makeUploadSnapshot({
      ...this.uploadSnapshot,
      revision: this.uploadSnapshot.revision + 1,
      visible: tasks.length > 0,
      tasks,
      completedCounter: this.uploadSnapshot.completedCounter,
    })
    this.emitUploadSnapshot(next)
    return next
  }

  uploadCancelAll = async (): Promise<UploadPanelSnapshot> => {
    const tasks = this.uploadSnapshot.tasks.filter(
      (task) =>
        task.status.state !== "queued" && task.status.state !== "active"
    )
    const next = makeUploadSnapshot({
      ...this.uploadSnapshot,
      revision: this.uploadSnapshot.revision + 1,
      visible: tasks.length > 0,
      tasks,
      completedCounter: this.uploadSnapshot.completedCounter,
    })
    this.emitUploadSnapshot(next)
    return next
  }

  uploadDismissTask = async (taskId: number): Promise<UploadPanelSnapshot> => {
    const tasks = this.uploadSnapshot.tasks.filter((task) => task.id !== taskId)
    const next = makeUploadSnapshot({
      ...this.uploadSnapshot,
      revision: this.uploadSnapshot.revision + 1,
      visible: tasks.length > 0,
      tasks,
      completedCounter: this.uploadSnapshot.completedCounter,
    })
    this.emitUploadSnapshot(next)
    return next
  }

  uploadDismissErrors = async (): Promise<UploadPanelSnapshot> => {
    const tasks = this.uploadSnapshot.tasks.filter(
      (task) => task.status.state !== "failed"
    )
    const next = makeUploadSnapshot({
      ...this.uploadSnapshot,
      revision: this.uploadSnapshot.revision + 1,
      visible: tasks.length > 0,
      tasks,
      completedCounter: this.uploadSnapshot.completedCounter,
    })
    this.emitUploadSnapshot(next)
    return next
  }

  uploadSetConcurrency = async (
    concurrency: number
  ): Promise<UploadPanelSnapshot> => {
    const clamped = Math.min(16, Math.max(1, Math.floor(concurrency)))
    const next: UploadPanelSnapshot = {
      ...this.uploadSnapshot,
      revision: this.uploadSnapshot.revision + 1,
      concurrency: clamped,
    }
    this.emitUploadSnapshot(next)
    return next
  }

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
      book_metadata_get: (args) => this.bookMetadataGet(String(args?.id)),
      book_subtitle_change: (args) =>
        this.bookSubtitleChange(String(args?.id), String(args?.subtitle)),
      book_subtitle_clear: (args) => this.bookSubtitleClear(String(args?.id)),
      book_authors_change: (args) =>
        this.bookAuthorsChange(
          String(args?.id),
          (args?.authors as string[] | undefined) ?? []
        ),
      book_authors_clear: (args) => this.bookAuthorsClear(String(args?.id)),
      book_publisher_change: (args) =>
        this.bookPublisherChange(String(args?.id), String(args?.publisher)),
      book_publisher_clear: (args) => this.bookPublisherClear(String(args?.id)),
      book_published_date_change: (args) =>
        this.bookPublishedDateChange(
          String(args?.id),
          String(args?.publishedDate)
        ),
      book_published_date_clear: (args) =>
        this.bookPublishedDateClear(String(args?.id)),
      book_language_change: (args) =>
        this.bookLanguageChange(String(args?.id), String(args?.language)),
      book_language_clear: (args) => this.bookLanguageClear(String(args?.id)),
      book_description_change: (args) =>
        this.bookDescriptionChange(String(args?.id), String(args?.description)),
      book_description_clear: (args) =>
        this.bookDescriptionClear(String(args?.id)),
      book_identifier_change: (args) =>
        this.bookIdentifierChange(String(args?.id), String(args?.identifier)),
      book_identifier_clear: (args) =>
        this.bookIdentifierClear(String(args?.id)),
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
          value_type: (args?.valueType as TagValueType | undefined) ?? "none",
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
      upload_get_state: () => this.uploadGetState(),
      upload_pick_and_enqueue: () => this.uploadPickAndEnqueue(),
      upload_enqueue_paths: (args) =>
        this.uploadEnqueuePaths((args?.paths as string[] | undefined) ?? []),
      upload_retry_task: (args) => this.uploadRetryTask(Number(args?.taskId)),
      upload_cancel_task: (args) => this.uploadCancelTask(Number(args?.taskId)),
      upload_cancel_all: () => this.uploadCancelAll(),
      upload_dismiss_task: (args) =>
        this.uploadDismissTask(Number(args?.taskId)),
      upload_dismiss_errors: () => this.uploadDismissErrors(),
      upload_set_concurrency: (args) =>
        this.uploadSetConcurrency(Number(args?.concurrency)),
      upload_consume_pending_open_ui: () => this.uploadConsumePendingOpenUi(),
      upload_set_mobile_ui_open: (args) =>
        this.uploadSetMobileUiOpen(Boolean(args?.open)),
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
