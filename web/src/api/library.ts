import { sendAuthenticatedRequest } from "./auth"
import { API_BASE_URL, ApiError, type RequestOptions } from "./http"
import type { Book } from "@/lib/types"
import { notifyError } from "@/lib/error-notifications"

export const book = {
  exists: async (hash: string): Promise<boolean> => {
    try {
      await performRequest<void>(`files/book?hash=${encodeURIComponent(hash)}`)
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
}

export const bookshelf = {
  createBook: async (title: string, hash: string) => {
    const book = await request<Book>("bookshelf/books", {
      method: "POST",
      body: { title, hash },
    })
    book.title = title
    book.hash = hash
    book.updated_at = book.created_at
    return book
  },
  getBooks: () => request<Book[]>("bookshelf/books"),
  renameBook: (id: Book["id"], title: string) =>
    request<void>(`bookshelf/books/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: { title },
      expectBody: false,
    }),
  deleteBook: (id: Book["id"]) =>
    request<void>(`bookshelf/books/${encodeURIComponent(id)}`, {
      method: "DELETE",
      expectBody: false,
    }),
  getCover: async (id: Book["id"]): Promise<Blob> =>
    (await sendAuthenticatedRequest(`library/bookshelf/books/${encodeURIComponent(id)}/cover`)).blob(),
  coverUrl: (id: Book["id"]) =>
    `${API_BASE_URL}/library/bookshelf/books/${encodeURIComponent(id)}/cover`,
}

export const files = {
  book,
}

export const library = {
  bookshelf,
  files,
}

export { ApiError } from "./http"

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  try {
    return await performRequest<T>(path, options)
  } catch (error) {
    if (error instanceof ApiError) notifyError(error.message)
    throw error
  }
}

async function performRequest<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const response = await sendAuthenticatedRequest(`library/${path}`, options)
  if (response.status === 204 || options.expectBody === false) return undefined as T
  return await response.json() as T
}
