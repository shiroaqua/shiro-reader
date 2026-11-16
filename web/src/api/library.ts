import { sendAuthenticatedRequest } from "./auth"
import { API_BASE_URL, ApiError, type RequestOptions } from "./http"
import type {
  Book,
  BookTag,
  UndefineTagStrategy,
  Tag,
  TagTreeNode,
} from "@/lib/types"
import { notifyError } from "@/lib/error-notifications"

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
    getCover: async (id: Book["id"]): Promise<Blob> =>
      (
        await sendAuthenticatedRequest(
          `library/bookshelf/books/${encodeURIComponent(id)}/cover`
        )
      ).blob(),
    coverUrl: (id: Book["id"]) =>
      `${API_BASE_URL}/library/bookshelf/books/${encodeURIComponent(id)}/cover`,

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
        if (error instanceof ApiError) notifyError(error.message)
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
          error.message === "library.book.file.already_exists"
        )
          return
        if (error instanceof ApiError) notifyError(error.message)
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
    if (error instanceof ApiError) notifyError(error.message)
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
  return (await response.json()) as T
}
