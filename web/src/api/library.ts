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
  coverUrl: (id: Book["id"]) =>
    `${API_BASE_URL}/bookshelf/books/${encodeURIComponent(id)}/cover`,
}

export const files = {
  book,
}

export const library = {
  bookshelf,
  files,
}

export class ApiError extends Error {
  httpStatus: number
  message: string

  constructor(status: number, code: string) {
    super(code)
    this.name = "ApiError"
    this.httpStatus = status
    this.message = code
  }
}

const API_BASE_URL = "http://localhost/api/v1/library"
// Abort requests the backend never answers so the UI can report a network error.
const REQUEST_TIMEOUT_MS = 10_000

type RequestOptions = {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
  body?: unknown
  // null disables the timeout, e.g. for large uploads.
  timeoutMs?: number | null
  // false for endpoints that answer with an empty body (e.g. 201 on upload).
  expectBody?: boolean
}

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
  const {
    method = "GET",
    body,
    timeoutMs = REQUEST_TIMEOUT_MS,
    expectBody = true,
  } = options

  const controller = new AbortController()
  const timeout =
    timeoutMs === null
      ? undefined
      : setTimeout(() => controller.abort(), timeoutMs)
  const init: RequestInit = { method, signal: controller.signal }

  if (body instanceof FormData) {
    // Let the browser set the multipart boundary.
    init.body = body
  } else if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" }
    init.body = JSON.stringify(body)
  }

  try {
    let response: Response
    try {
      response = await fetch(`${API_BASE_URL}/${path}`, init)
    } catch {
      throw new ApiError(0, "network.error")
    }

    if (response.status === 204) {
      return undefined as T
    }

    if (!response.ok) {
      throw await parseError(response)
    }

    if (!expectBody) {
      return undefined as T
    }

    return (await response.json()) as T
  } finally {
    clearTimeout(timeout)
  }
}

async function parseError(response: Response) {
  try {
    const payload = (await response.json()) as { error?: { message?: string } }
    return new ApiError(
      response.status,
      typeof payload?.error?.message === "string"
        ? payload.error.message
        : "common.error.unknown"
    )
  } catch {
    return new ApiError(response.status, "common.error.unknown")
  }
}
