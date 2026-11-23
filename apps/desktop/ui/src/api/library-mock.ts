import { setPreviewLibrary, type LibraryBridge } from "./library"
import type { Book, BookTag, Tag, TagTreeNode, TrashedBook } from "@/lib/types"
import { notifyLocalError } from "@/lib/error-notifications"

const books = new Map<string, Book>()
const trash = new Map<string, TrashedBook>()
const tags = new Map<string, Tag>()
const bindings = new Map<string, Map<string, BookTag>>()

function addBook(title: string, type: string): Book {
  const id = crypto.randomUUID()
  const now = new Date().toISOString()
  const book = { id, title, hash: id, type, created_at: now, updated_at: now }
  books.set(id, book)
  return book
}

function requireItem<T>(items: Map<string, T>, id: string): T {
  const item = items.get(id)
  if (item) return item
  notifyLocalError("local.common.error.unknown")
  throw new Error(`Unknown preview item ${id}`)
}

function tagTree(parentId: string | null): TagTreeNode[] {
  return [...tags.values()].filter((tag) => tag.parent_id === parentId).map(
    (tag) => ({ ...tag, children: tagTree(tag.id) })
  )
}

function removeTag(id: string, cascade: boolean) {
  const current = requireItem(tags, id)
  for (const child of tags.values()) {
    if (child.parent_id !== id) continue
    if (cascade) removeTag(child.id, true)
    else tags.set(child.id, { ...child, parent_id: current.parent_id })
  }
  tags.delete(id)
  for (const items of bindings.values()) items.delete(id)
}

const previewLibrary: LibraryBridge = {
  async booksList() { return [...books.values()] },
  async bookRename(id, title) {
    books.set(id, { ...requireItem(books, id), title, updated_at: new Date().toISOString() })
  },
  async booksDelete(ids) {
    const selected = ids.map((id) => requireItem(books, id))
    for (const book of selected) {
      books.delete(book.id)
      trash.set(book.id, { ...book, deleted_at: new Date().toISOString() })
    }
  },
  async trashList() { return [...trash.values()] },
  async trashRestore(ids) {
    const operationId = crypto.randomUUID()
    const restoredIds: string[] = []
    const failed = []
    for (const id of ids) {
      const trashed = trash.get(id)
      if (!trashed) {
        notifyLocalError("local.common.error.unknown")
        failed.push({ bookId: id, restored: false, error: {
          operationId, errorId: crypto.randomUUID(), code: 0,
          key: "mock.book.not_found", itemId: id,
        } })
        continue
      }
      const { deleted_at: deletedAt, ...book } = trashed
      trash.delete(id)
      books.set(id, { ...book, updated_at: deletedAt })
      restoredIds.push(id)
    }
    return { operationId, restoredIds, failed, skippedIds: [] }
  },
  async trashPermanentlyDelete(ids) {
    ids.forEach((id) => requireItem(trash, id))
    for (const id of ids) {
      trash.delete(id)
      bindings.delete(id)
    }
  },
  async tagsList() { return tagTree(null) },
  async tagDefine(definition) {
    if (definition.parent_id) requireItem(tags, definition.parent_id)
    const now = new Date().toISOString()
    const tag = { ...definition, id: crypto.randomUUID(), created_at: now, updated_at: now }
    tags.set(tag.id, tag)
    return tag
  },
  async tagRename(id, name) { tags.set(id, { ...requireItem(tags, id), name, updated_at: new Date().toISOString() }) },
  async tagRecolor(id, color) { tags.set(id, { ...requireItem(tags, id), color, updated_at: new Date().toISOString() }) },
  async tagMove(id, parentId) {
    let ancestor = parentId
    while (ancestor) {
      if (ancestor === id) {
        notifyLocalError("local.common.error.unknown")
        throw new Error("Cannot move a preview tag into its own subtree")
      }
      ancestor = requireItem(tags, ancestor).parent_id
    }
    tags.set(id, { ...requireItem(tags, id), parent_id: parentId, updated_at: new Date().toISOString() })
  },
  async tagUndefine(id, strategy) { removeTag(id, strategy === "CASCADE") },
  async bookTagsList(id) {
    requireItem(books, id)
    return [...(bindings.get(id)?.values() ?? [])]
  },
  async bookTagBind(id, binding) {
    requireItem(books, id)
    requireItem(tags, binding.tag_id)
    const items = bindings.get(id) ?? new Map<string, BookTag>()
    items.set(binding.tag_id, binding)
    bindings.set(id, items)
  },
  async bookTagUnbind(id, tagId) {
    requireItem(books, id)
    bindings.get(id)?.delete(tagId)
  },
  async pickAndImportBooks() {
    const imported = [addBook(`Preview book ${books.size + trash.size + 1}`, "pdf")]
    return { operationId: crypto.randomUUID(), cancelled: false, imported,
      failed: [], skipped: [], abortedReason: null, summaryMessage: null }
  },
  async cancelBookImport() { return false },
  coverUrl: () => undefined,
}

export function enableLibraryPreview() {
  addBook("The Book of Tea", "epub")
  addBook("A Room of One’s Own", "pdf")
  setPreviewLibrary(previewLibrary)
}
