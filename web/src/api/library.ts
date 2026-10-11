import type { Bookshelf } from "@/lib/types"

export const library = {
  getBookshelves: () => request<Bookshelf[]>("bookshelves"),
}

async function request<T>(path: string): Promise<T> {
  let response: Response
  try {
    response = await fetch(`http://localhost/api/v1/library/${path}`)
  } catch {
    throw new ApiError(0, "network.error")
  }

  if (!response.ok) 
    throw await parseError(response);
  else
    return await response.json()
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

async function parseError(response: Response) {
  try {
    const payload = (await response.json()) as { error?: { message?: string } }
    return new ApiError(
      response.status,
      payload.error?.message ?? "unknown"
    )
  } catch {
    return new ApiError(response.status, "unknown")
  }
}