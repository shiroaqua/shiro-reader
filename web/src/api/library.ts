import { sendAuthenticatedRequest } from "./auth"
import { ApiError, parseJson, type RequestOptions } from "./http"
import type {
  Book,
  BookTag,
  TrashedBook,
  UndefineTagStrategy,
  Tag,
  TagTreeNode,
} from "@/lib/types"
import { notifyError } from "@/lib/error-notifications"
import { ErrorCode } from "@/lib/error-codes"

export const bookshelf = {
  book: {
    create: (title: string, hash: string) => {
      return request<Book>("bookshelf/books", {
        method: "POST",
        body: { title, hash },
      })
    },
    list: () => request<Book[]>("bookshelf/books"),
    rename: (id: Book["id"], title: string) =>
      request<void>(`bookshelf/books/${encodeURIComponent(id)}`, {
        method: "PATCH",
        body: { title },
        expectBody: false,
      }),
    delete: (id: Book["id"]) =>
      request<void>(`bookshelf/books/${encodeURIComponent(id)}`, {
        method: "DELETE",
        expectBody: false,
      }),
    deleteBatch: (ids: Book["id"][]) =>
      request<void>("bookshelf/books", {
        method: "DELETE",
        body: { ids },
        expectBody: false,
      }),
    getCover: async (id: Book["id"], signal?: AbortSignal): Promise<Blob> =>
      (
        await sendAuthenticatedRequest(
          `library/bookshelf/books/${encodeURIComponent(id)}/cover`,
          { signal }
        )
      ).blob(),

    getTags: (id: Book["id"]) =>
      request<BookTag[]>(`bookshelf/books/${encodeURIComponent(id)}/tags`),
    bindTag: (id: Book["id"], binding: BookTag) =>
      request<void>(`bookshelf/books/${encodeURIComponent(id)}/tags`, {
        method: "POST",
        body: binding,
        expectBody: false,
      }),
    unbindTag: (id: Book["id"], tagId: Tag["id"]) =>
      request<void>(`bookshelf/books/${encodeURIComponent(id)}/tags`, {
        method: "DELETE",
        body: { tag_id: tagId },
        expectBody: false,
      }),
  },
  trash: {
    list: () => request<TrashedBook[]>("bookshelf/trash"),
    restore: (id: Book["id"]) =>
      request<void>(`bookshelf/trash/${encodeURIComponent(id)}/restore`, {
        method: "POST",
        expectBody: false,
      }),
    restoreBatch: async (
      ids: Book["id"][]
    ): Promise<{ restoredIds: Book["id"][]; failedIds: Book["id"][] }> => {
      const restoredIds: Book["id"][] = []
      const failedIds: Book["id"][] = []
      let firstError: ApiError | undefined
      const queue = [...ids]
      const concurrency = Math.min(4, queue.length)
      await Promise.all(
        Array.from({ length: concurrency }, async () => {
          while (queue.length > 0) {
            const id = queue.shift()!
            try {
              await performRequest<void>(
                `bookshelf/trash/${encodeURIComponent(id)}/restore`,
                {
                  method: "POST",
                  expectBody: false,
                }
              )
              restoredIds.push(id)
            } catch (error) {
              failedIds.push(id)
              if (!firstError && error instanceof ApiError) {
                firstError = error
              }
            }
          }
        })
      )
      if (firstError) {
        notifyError(firstError.translation)
      } else if (failedIds.length > 0) {
        notifyError({ source: "local", key: "local.common.error.unknown" })
      }
      return { restoredIds, failedIds }
    },
    permanentlyDelete: (id: Book["id"]) =>
      request<void>(`bookshelf/trash/${encodeURIComponent(id)}`, {
        method: "DELETE",
        expectBody: false,
      }),
    permanentlyDeleteBatch: (ids: Book["id"][]) =>
      request<void>("bookshelf/trash", {
        method: "DELETE",
        body: { ids },
        expectBody: false,
      }),
  },
}

export const files = {
  book: {
    exists: async (hash: string): Promise<boolean> => {
      try {
        await performRequest<void>(
          `files/book?hash=${encodeURIComponent(hash)}`
        )
        return true
      } catch (error) {
        if (error instanceof ApiError && error.httpStatus === 404) return false
        if (error instanceof ApiError) notifyError(error.translation)
        throw error
      }
    },
    upload: async (hash: string, file: File): Promise<void> => {
      const form = new FormData()
      form.append("hash", hash)
      form.append("file", file, file.name)
      try {
        await performRequest<void>("files/book", {
          method: "POST",
          body: form,
          timeoutMs: null,
          expectBody: false,
        })
      } catch (error) {
        if (
          error instanceof ApiError &&
          error.code === ErrorCode.bookFileAlreadyExists
        )
          return
        if (error instanceof ApiError) notifyError(error.translation)
        throw error
      }
    },
  },
}

export const tag = {
  list: () => request<TagTreeNode[]>("tags"),
  define: (tag: Omit<Tag, "id">) =>
    request<Tag>("tags", { method: "POST", body: tag }),
  rename: (id: Tag["id"], name: string) =>
    request<void>(`tags/${encodeURIComponent(id)}/name`, {
      method: "PATCH",
      body: { name },
      expectBody: false,
    }),
  recolor: (id: Tag["id"], color: string) =>
    request<void>(`tags/${encodeURIComponent(id)}/color`, {
      method: "PATCH",
      body: { color },
      expectBody: false,
    }),
  move: (id: Tag["id"], parentId: Tag["parent_id"]) =>
    request<void>(`tags/${encodeURIComponent(id)}/parent`, {
      method: "PATCH",
      body: { parent_id: parentId },
      expectBody: false,
    }),
  undefine: (id: Tag["id"], strategy: UndefineTagStrategy) =>
    request<void>(
      `tags/${encodeURIComponent(id)}?strategy=${encodeURIComponent(strategy)}`,
      {
        method: "DELETE",
        expectBody: false,
      }
    ),
}

export const library = {
  bookshelf,
  tag,
  files,
}

export { ApiError } from "./http"

async function request<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  try {
    return await performRequest<T>(path, options)
  } catch (error) {
    if (error instanceof ApiError) notifyError(error.translation)
    throw error
  }
}

async function performRequest<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  const response = await sendAuthenticatedRequest(`library/${path}`, options)
  if (response.status === 204 || options.expectBody === false)
    return undefined as T
  return parseJson<T>(response)
}
