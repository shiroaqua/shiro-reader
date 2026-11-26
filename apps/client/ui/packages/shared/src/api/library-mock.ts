import { setPreviewLibrary, type LibraryBridge } from "./library"
import type { UploadPanelSnapshot, UploadTask } from "./tauri-bridge"
import type { Book, BookTag, Tag, TagTreeNode, TrashedBook } from "@shiro/ui-shared/lib/types"
import { notifyLocalError } from "@shiro/ui-shared/lib/error-notifications"

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
  return [...tags.values()]
    .filter((tag) => tag.parent_id === parentId)
    .map((tag) => ({ ...tag, children: tagTree(tag.id) }))
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

// -----------------------------------------------------------------------------
// In-memory Mock Upload Queue (for pnpm dev:mock preview)
// -----------------------------------------------------------------------------

interface MockTaskEntry {
  task: UploadTask
  timer?: ReturnType<typeof setInterval>
  dismissTimer?: ReturnType<typeof setTimeout>
}

const mockUploadListeners = new Set<(snapshot: UploadPanelSnapshot) => void>()
const mockTasks = new Map<number, MockTaskEntry>()
const mockPendingQueue: number[] = []
let mockNextTaskId = 1
let mockRevision = 0
let mockVisible = false
let mockConcurrency = 4
let mockCompletedCounter = 0
let mockTotal = 0
let mockSucceeded = 0
let mockFailed = 0
let mockPickSeq = 1

function buildMockUploadSnapshot(): UploadPanelSnapshot {
  const activeCount = [...mockTasks.values()].filter(
    (entry) => entry.task.status.state === "active"
  ).length
  const queuedCount = mockPendingQueue.length
  const hasDismissibleErrors = [...mockTasks.values()].some(
    (entry) => entry.task.status.state === "failed"
  )
  return {
    revision: mockRevision,
    visible: mockVisible,
    concurrency: mockConcurrency,
    completedCounter: mockCompletedCounter,
    stats: {
      total: mockTotal,
      remaining: activeCount + queuedCount,
      active: activeCount,
      succeeded: mockSucceeded,
      failed: mockFailed,
    },
    hasDismissibleErrors,
    tasks: [...mockTasks.values()].map((entry) => entry.task),
  }
}

function maybeResetMockPanel() {
  const activeCount = [...mockTasks.values()].filter(
    (entry) => entry.task.status.state === "active"
  ).length
  if (mockTasks.size === 0 && activeCount === 0 && mockPendingQueue.length === 0) {
    mockVisible = false
    mockTotal = 0
    mockSucceeded = 0
    mockFailed = 0
  }
}

function emitMockUploadState(): UploadPanelSnapshot {
  mockRevision += 1
  const snapshot = buildMockUploadSnapshot()
  mockUploadListeners.forEach((listener) => listener(snapshot))
  return snapshot
}

function pumpMockUploadQueue() {
  const activeCount = () =>
    [...mockTasks.values()].filter(
      (entry) => entry.task.status.state === "active"
    ).length

  while (activeCount() < mockConcurrency && mockPendingQueue.length > 0) {
    const taskId = mockPendingQueue.shift()!
    const entry = mockTasks.get(taskId)
    if (!entry || entry.task.status.state !== "queued") continue

    const totalBytes = 4 * 1024 * 1024
    let step = 0
    entry.task = {
      ...entry.task,
      status: {
        state: "active",
        stage: "staging",
        progressPercent: 10,
        bytesProcessed: Math.round(totalBytes * 0.1),
        totalBytes,
        speedBytesPerSec: 2 * 1024 * 1024,
        etaSeconds: 2,
      },
    }

    entry.timer = setInterval(() => {
      const current = mockTasks.get(taskId)
      if (!current || current.task.status.state !== "active") {
        if (entry.timer) clearInterval(entry.timer)
        return
      }
      step += 1
      if (step < 4) {
        const progress = Math.min(95, 10 + step * 25)
        current.task = {
          ...current.task,
          status: {
            state: "active",
            stage: step >= 3 ? "finalizing" : "uploading",
            progressPercent: progress,
            bytesProcessed: Math.round((totalBytes * progress) / 100),
            totalBytes,
            speedBytesPerSec: 2 * 1024 * 1024,
            etaSeconds: Math.max(0, 4 - step),
          },
        }
        emitMockUploadState()
        return
      }

      if (entry.timer) clearInterval(entry.timer)
      const isUnsupported =
        !current.task.fileName.toLowerCase().endsWith(".pdf") &&
        !current.task.fileName.toLowerCase().endsWith(".epub")
      if (isUnsupported) {
        mockFailed += 1
        current.task = {
          ...current.task,
          status: {
            state: "failed",
            error: {
              operationId: crypto.randomUUID(),
              errorId: crypto.randomUUID(),
              code: 2106,
              key: "library.book.file.unsupported",
              locale: "en",
              message: "This book file format is not supported.",
              itemId: current.task.fileName,
            },
          },
        }
      } else {
        const title = current.task.fileName.replace(/\.(pdf|epub)$/i, "")
        const ext = current.task.fileName.toLowerCase().endsWith(".epub")
          ? "epub"
          : "pdf"
        const book = addBook(title, ext)
        mockSucceeded += 1
        mockCompletedCounter += 1
        current.task = {
          ...current.task,
          status: {
            state: "succeeded",
            bookId: book.id,
            title: book.title,
          },
        }
        current.dismissTimer = setTimeout(() => {
          mockTasks.delete(taskId)
          maybeResetMockPanel()
          emitMockUploadState()
        }, 1_000)
      }
      pumpMockUploadQueue()
      emitMockUploadState()
    }, 250)
  }
}

function enqueueMockFiles(fileNames: string[]): UploadPanelSnapshot {
  if (fileNames.length === 0) return buildMockUploadSnapshot()
  mockVisible = true
  for (const fileName of fileNames) {
    const id = mockNextTaskId++
    mockTasks.set(id, {
      task: { id, fileName, status: { state: "queued" } },
    })
    mockPendingQueue.push(id)
    mockTotal += 1
  }
  pumpMockUploadQueue()
  return emitMockUploadState()
}

const previewLibrary: LibraryBridge = {
  async booksList() {
    return [...books.values()]
  },
  async bookRename(id, title) {
    books.set(id, {
      ...requireItem(books, id),
      title,
      updated_at: new Date().toISOString(),
    })
  },
  async booksDelete(ids) {
    const selected = ids.map((id) => requireItem(books, id))
    for (const book of selected) {
      books.delete(book.id)
      trash.set(book.id, { ...book, deleted_at: new Date().toISOString() })
    }
  },
  async trashList() {
    return [...trash.values()]
  },
  async trashRestore(ids) {
    const operationId = crypto.randomUUID()
    const restoredIds: string[] = []
    const failed = []
    for (const id of ids) {
      const trashed = trash.get(id)
      if (!trashed) {
        notifyLocalError("local.common.error.unknown")
        failed.push({
          bookId: id,
          restored: false,
          error: {
            operationId,
            errorId: crypto.randomUUID(),
            code: 0,
            key: "mock.book.not_found",
            itemId: id,
          },
        })
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
  async tagsList() {
    return tagTree(null)
  },
  async tagDefine(definition) {
    if (definition.parent_id) requireItem(tags, definition.parent_id)
    const now = new Date().toISOString()
    const tag = {
      ...definition,
      value_regex: null,
      id: crypto.randomUUID(),
      created_at: now,
      updated_at: now,
    }
    tags.set(tag.id, tag)
    return tag
  },
  async tagRename(id, name) {
    tags.set(id, {
      ...requireItem(tags, id),
      name,
      updated_at: new Date().toISOString(),
    })
  },
  async tagRecolor(id, color) {
    tags.set(id, {
      ...requireItem(tags, id),
      color,
      updated_at: new Date().toISOString(),
    })
  },
  async tagMove(id, parentId) {
    let ancestor = parentId
    while (ancestor) {
      if (ancestor === id) {
        notifyLocalError("local.common.error.unknown")
        throw new Error("Cannot move a preview tag into its own subtree")
      }
      ancestor = requireItem(tags, ancestor).parent_id
    }
    tags.set(id, {
      ...requireItem(tags, id),
      parent_id: parentId,
      updated_at: new Date().toISOString(),
    })
  },
  async tagUndefine(id, strategy) {
    removeTag(id, strategy === "CASCADE")
  },
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
  async listenUploadState(handler) {
    mockUploadListeners.add(handler)
    return () => {
      mockUploadListeners.delete(handler)
    }
  },
  async uploadGetState() {
    return buildMockUploadSnapshot()
  },
  async uploadPickAndEnqueue() {
    const seq = mockPickSeq++
    return enqueueMockFiles([`Preview book ${seq}.epub`])
  },
  async uploadEnqueuePaths(paths) {
    const names = paths.map(
      (path) => path.split(/[\\/]/).filter(Boolean).pop() ?? path
    )
    return enqueueMockFiles(names)
  },
  async uploadRetryTask(taskId) {
    const entry = mockTasks.get(taskId)
    if (!entry || entry.task.status.state !== "failed") {
      return buildMockUploadSnapshot()
    }
    mockFailed = Math.max(0, mockFailed - 1)
    entry.task = { ...entry.task, status: { state: "queued" } }
    mockPendingQueue.push(taskId)
    pumpMockUploadQueue()
    return emitMockUploadState()
  },
  async uploadCancelTask(taskId) {
    const entry = mockTasks.get(taskId)
    if (!entry) return buildMockUploadSnapshot()
    if (entry.task.status.state === "queued") {
      const idx = mockPendingQueue.indexOf(taskId)
      if (idx >= 0) mockPendingQueue.splice(idx, 1)
    } else if (entry.task.status.state === "active") {
      if (entry.timer) clearInterval(entry.timer)
    } else {
      return buildMockUploadSnapshot()
    }
    mockTasks.delete(taskId)
    mockTotal = Math.max(0, mockTotal - 1)
    pumpMockUploadQueue()
    maybeResetMockPanel()
    return emitMockUploadState()
  },
  async uploadCancelAll() {
    for (const [id, entry] of [...mockTasks.entries()]) {
      if (
        entry.task.status.state === "queued" ||
        entry.task.status.state === "active"
      ) {
        if (entry.timer) clearInterval(entry.timer)
        mockTasks.delete(id)
        mockTotal = Math.max(0, mockTotal - 1)
      }
    }
    mockPendingQueue.length = 0
    maybeResetMockPanel()
    return emitMockUploadState()
  },
  async uploadDismissTask(taskId) {
    const entry = mockTasks.get(taskId)
    if (!entry) return buildMockUploadSnapshot()
    if (entry.task.status.state === "failed") {
      mockFailed = Math.max(0, mockFailed - 1)
      mockTotal = Math.max(0, mockTotal - 1)
      mockTasks.delete(taskId)
    } else if (entry.task.status.state === "succeeded") {
      if (entry.dismissTimer) clearTimeout(entry.dismissTimer)
      mockTasks.delete(taskId)
    }
    maybeResetMockPanel()
    return emitMockUploadState()
  },
  async uploadDismissErrors() {
    for (const [id, entry] of [...mockTasks.entries()]) {
      if (entry.task.status.state === "failed") {
        mockTasks.delete(id)
        mockFailed = Math.max(0, mockFailed - 1)
        mockTotal = Math.max(0, mockTotal - 1)
      }
    }
    maybeResetMockPanel()
    return emitMockUploadState()
  },
  async uploadSetConcurrency(concurrency) {
    mockConcurrency = Math.min(16, Math.max(1, Math.floor(concurrency)))
    pumpMockUploadQueue()
    return emitMockUploadState()
  },
  async listenUploadOpenUi() {
    return () => {}
  },
  async uploadConsumePendingOpenUi() {
    return false
  },
  async uploadSetMobileUiOpen() {},
  coverUrl: () => undefined,
}

export function enableLibraryPreview() {
  addBook("The Book of Tea", "epub")
  addBook("A Room of One’s Own", "pdf")
  setPreviewLibrary(previewLibrary)
}
