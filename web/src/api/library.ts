import type { Bookshelf } from "@/lib/types"
import { notifyError } from "@/lib/error-notifications"

export const bookshelf = {
  create: async (name: string) =>  {
    const bookshelf = await request<Bookshelf>("bookshelves", {
      method: "POST",
      body: { name: name },
    });
    bookshelf.name = name;
    bookshelf.updated_at = bookshelf.created_at;
    return bookshelf;
  },
  list: () => request<Bookshelf[]>("bookshelves"),
  rename: (id: Bookshelf["id"], name: string): Promise<void> =>
    request<void>(`bookshelves/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: { name },
    }),
  delete: (id: Bookshelf["id"]): Promise<void> =>
    request<void>(`bookshelves/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
}

export const library = {
  bookshelf,
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


type RequestOptions = {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
  body?: unknown
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
  const { method = "GET", body } = options

  const init: RequestInit = { method }

  if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" }
    init.body = JSON.stringify(body)
  }

  let response: Response
  try {
    response = await fetch(`http://localhost/api/v1/library/${path}`, init)
  } catch {
    throw new ApiError(0, "network.error")
  }

  if (response.status === 204) {
    return undefined as T
  }

  if (!response.ok) {
    throw await parseError(response)
  }

  return (await response.json()) as T
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
