import type { TranslationMessage } from "@/lib/i18n"

export const API_BASE_URL = "http://localhost/api/v1"

export class ApiError extends Error {
  httpStatus: number
  translation: TranslationMessage

  constructor(status: number, translation: TranslationMessage) {
    super(translation.key)
    this.name = "ApiError"
    this.httpStatus = status
    this.translation = translation
  }
}

export type RequestOptions = {
  method?: "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
  body?: unknown
  cache?: RequestCache
  timeoutMs?: number | null
  expectBody?: boolean
}

export async function sendRequest(
  path: string,
  { method = "GET", body, cache, timeoutMs = 10_000 }: RequestOptions = {},
  token?: string
): Promise<Response> {
  const controller = new AbortController()
  const timeout = timeoutMs === null ? undefined : setTimeout(() => controller.abort(), timeoutMs)
  const headers: Record<string, string> = {}
  if (token) headers.Authorization = `Bearer ${token}`
  const init: RequestInit = { method, headers, signal: controller.signal }
  if (cache) init.cache = cache
  if (body instanceof FormData) {
    init.body = body
  } else if (body !== undefined) {
    headers["Content-Type"] = "application/json"
    init.body = JSON.stringify(body)
  }
  try {
    return await fetch(`${API_BASE_URL}/${path}`, init)
  } catch {
    throw new ApiError(0, { source: "local", key: "local.network.error" })
  } finally {
    clearTimeout(timeout)
  }
}

export async function parseError(response: Response): Promise<ApiError> {
  try {
    const payload = (await response.json()) as { error?: { message?: string } }
    const code = payload?.error?.message
    if (typeof code === "string") {
      return new ApiError(response.status, { source: "server", key: code })
    }
    return new ApiError(response.status, {
      source: "local",
      key: "local.common.error.unknown",
    })
  } catch {
    return new ApiError(response.status, {
      source: "local",
      key: "local.common.error.unknown",
    })
  }
}
