import type { TranslationMessage } from "@/lib/i18n"

export const API_BASE_URL = "http://localhost/api/v1"

export class ApiError extends Error {
  httpStatus: number
  code: number
  translation: TranslationMessage

  constructor(
    status: number,
    translation: TranslationMessage,
    code = 0
  ) {
    super(translation.key)
    this.name = "ApiError"
    this.httpStatus = status
    this.code = code
    this.translation = translation
  }
}

export type RequestOptions = {
  method?: "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
  body?: unknown
  cache?: RequestCache
  timeoutMs?: number | null
  expectBody?: boolean
  signal?: AbortSignal
}

export async function sendRequest(
  path: string,
  {
    method = "GET",
    body,
    cache,
    timeoutMs = 10_000,
    signal,
  }: RequestOptions = {},
  token?: string
): Promise<Response> {
  if (signal?.aborted) {
    throw new DOMException("Aborted", "AbortError")
  }
  const controller = new AbortController()
  let timedOut = false
  const onAbort = () => controller.abort(signal?.reason)
  signal?.addEventListener("abort", onAbort, { once: true })
  const timeout =
    timeoutMs === null
      ? undefined
      : setTimeout(() => {
          timedOut = true
          controller.abort()
        }, timeoutMs)
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
  } catch (error) {
    if (signal?.aborted) throw error
    throw new ApiError(0, {
      source: "local",
      key: timedOut ? "local.network.timeout" : "local.network.error",
    })
  } finally {
    clearTimeout(timeout)
    signal?.removeEventListener("abort", onAbort)
  }
}

export async function parseJson<T>(response: Response): Promise<T> {
  try {
    return (await response.json()) as T
  } catch {
    throw new ApiError(response.status, {
      source: "local",
      key: "local.common.error.unknown",
    })
  }
}

export async function parseError(response: Response): Promise<ApiError> {
  try {
    const payload = (await response.json()) as {
      error?: { code?: unknown; message?: unknown }
    }
    const code = payload?.error?.message
    if (typeof code === "string") {
      return new ApiError(
        response.status,
        { source: "server", key: code },
        typeof payload.error?.code === "number" ? payload.error.code : 0
      )
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
