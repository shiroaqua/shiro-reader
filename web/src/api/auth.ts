import {
  ApiError,
  parseError,
  parseJson,
  sendRequest,
  type RequestOptions,
} from "./http"

export const AUTH_STORAGE_KEY = "shiro-reader.auth"
// Axum serves HEAD for GET routes; this checks authentication without a book payload.
const SESSION_CHECK_PATH = "library/bookshelf/books"
const SESSION_CHECK_OPTIONS = { method: "HEAD", cache: "no-store" } as const
type Session = { username: string; access_token: string; refresh_token: string }
type AuthState = { session: Session | null; isLoading: boolean }
let state: AuthState = { session: null, isLoading: true }
let generation = 0
let clockOffsetMs = 0
let refreshPending: Promise<string> | undefined
let restorePending: Promise<void> | undefined
let logoutPending: Promise<void> | undefined
let validationPending: Promise<void> | undefined
const listeners = new Set<() => void>()

export const getAuthState = () => state
export function subscribeToAuth(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function publish(session: Session | null, isLoading = false) {
  state = { session, isLoading }
  listeners.forEach((listener) => listener())
}

function persist(session: Session | null) {
  try {
    if (session) localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(session))
    else localStorage.removeItem(AUTH_STORAGE_KEY)
  } catch {
    // Storage can be disabled; the current tab still supports an in-memory session.
  }
}

function parseStoredSession(raw: string | null): Session | null {
  if (!raw) return null
  try {
    const stored: unknown = JSON.parse(raw)
    if (
      !stored ||
      typeof stored !== "object" ||
      !("username" in stored) ||
      typeof stored.username !== "string" ||
      !("access_token" in stored) ||
      typeof stored.access_token !== "string" ||
      !("refresh_token" in stored) ||
      typeof stored.refresh_token !== "string"
    ) {
      return null
    }
    return {
      username: stored.username,
      access_token: stored.access_token,
      refresh_token: stored.refresh_token,
    }
  } catch {
    return null
  }
}

function parseJwtClaims(token: string): { exp: number; iat?: number } | null {
  try {
    const parts = token.split(".")
    if (parts.length < 2) return null
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/")
    const padded = base64.padEnd(base64.length + ((4 - (base64.length % 4)) % 4), "=")
    const { exp, iat } = JSON.parse(atob(padded)) as {
      exp?: unknown
      iat?: unknown
    }
    if (typeof exp !== "number" || !Number.isFinite(exp)) return null
    return {
      exp: exp * 1000,
      iat: typeof iat === "number" && Number.isFinite(iat) ? iat * 1000 : undefined,
    }
  } catch {
    return null
  }
}

function syncClockOffset(token: string) {
  const claims = parseJwtClaims(token)
  if (claims?.iat !== undefined) {
    clockOffsetMs = claims.iat - Date.now()
  }
}

function expiresAt(token: string): number {
  return parseJwtClaims(token)?.exp ?? 0
}

function serverNow(): number {
  return Date.now() + clockOffsetMs
}

function expired(token: string): boolean {
  return expiresAt(token) <= serverNow()
}

function authError() {
  return new ApiError(401, {
    source: "local",
    key: "local.auth.session_expired",
  })
}

export function clearSession(shouldPersist = true) {
  generation++
  refreshPending = undefined
  validationPending = undefined
  if (shouldPersist) persist(null)
  publish(null)
}

export async function logout(): Promise<void> {
  if (logoutPending) return logoutPending
  const session = state.session
  if (!session) return
  const pending = (async () => {
    const response = await sendRequest("auth/logout", {
      method: "POST",
      body: { token: session.refresh_token },
    })
    if (state.session?.refresh_token !== session.refresh_token) return
    if (response.status === 204 || response.status === 401) {
      clearSession()
      return
    }
    throw await parseError(response)
  })()
  logoutPending = pending
  try {
    await pending
  } finally {
    if (logoutPending === pending) logoutPending = undefined
  }
}

export async function login(username: string, password: string): Promise<void> {
  const attempt = ++generation
  refreshPending = undefined
  validationPending = undefined
  logoutPending = undefined
  const response = await sendRequest("auth/login", {
    method: "POST",
    body: { username, password },
  })
  if (!response.ok) throw await parseError(response)
  const tokens = await parseJson<Partial<Session>>(response)
  if (
    typeof tokens.access_token !== "string" ||
    typeof tokens.refresh_token !== "string"
  ) {
    throw authError()
  }
  syncClockOffset(tokens.access_token)
  if (expired(tokens.access_token) || expired(tokens.refresh_token)) {
    throw authError()
  }
  if (generation !== attempt) throw authError()

  const session = {
    username,
    access_token: tokens.access_token,
    refresh_token: tokens.refresh_token,
  }
  persist(session)
  publish(session)
}

async function refresh(): Promise<string> {
  if (refreshPending) return refreshPending
  const session = state.session
  const attempt = generation
  if (!session || logoutPending) throw authError()
  const pending = (async () => {
    try {
      const response = await sendRequest("auth/refresh", {
        method: "POST",
        body: { token: session.refresh_token },
      })
      if (!response.ok) throw await parseError(response)
      const tokens = await parseJson<{ access_token?: unknown }>(response)
      if (typeof tokens.access_token !== "string") throw authError()
      syncClockOffset(tokens.access_token)
      if (expired(tokens.access_token)) throw authError()
      if (generation !== attempt) throw authError()

      const updated = { ...session, access_token: tokens.access_token }
      persist(updated)
      publish(updated, state.isLoading)
      return updated.access_token
    } catch (error) {
      // A connection failure does not establish that the server rejected the session.
      if (generation === attempt && error instanceof ApiError && error.httpStatus === 401) {
        clearSession()
      }
      throw error
    }
  })()
  refreshPending = pending
  try {
    return await pending
  } finally {
    if (refreshPending === pending) refreshPending = undefined
  }
}

export async function validateSession(): Promise<void> {
  if (validationPending) return validationPending
  const session = state.session
  if (!session || logoutPending) return
  const pending = sendAuthenticatedRequest(SESSION_CHECK_PATH, SESSION_CHECK_OPTIONS)
    .then(() => undefined)
  validationPending = pending
  try {
    await pending
  } finally {
    if (validationPending === pending) validationPending = undefined
  }
}

export async function restoreSession(): Promise<void> {
  if (restorePending) return restorePending
  const attempt = generation
  const pending = (async () => {
    try {
      const stored = parseStoredSession(localStorage.getItem(AUTH_STORAGE_KEY))
      if (!stored) {
        clearSession()
        return
      }
      publish(stored, true)
    } catch {
      if (generation === attempt) clearSession()
      return
    }
    try {
      await validateSession()
    } catch {
      // Keep credentials on transient failures; confirmed invalid tokens clear themselves.
    } finally {
      if (generation === attempt) publish(state.session)
    }
  })()
  restorePending = pending
  try {
    await pending
  } finally {
    if (restorePending === pending) restorePending = undefined
  }
}

export function monitorSession() {
  const check = () => {
    if (document.visibilityState === "hidden" || state.isLoading || logoutPending) return
    void validateSession().catch(() => {})
  }
  let expiryTimer: ReturnType<typeof setTimeout> | undefined
  const scheduleExpiry = () => {
    clearTimeout(expiryTimer)
    if (!state.session || state.isLoading) return
    const expiry = Math.min(
      expiresAt(state.session.access_token),
      expiresAt(state.session.refresh_token)
    )
    const remaining = Math.max(0, expiry - serverNow())
    if (remaining > 2_147_483_647) {
      expiryTimer = setTimeout(scheduleExpiry, 2_147_483_647)
    } else {
      expiryTimer = setTimeout(check, remaining)
    }
  }
  const handleStorage = (event: StorageEvent) => {
    if (event.storageArea !== localStorage || event.key !== AUTH_STORAGE_KEY) return
    if (event.newValue === null) {
      if (state.session !== null) clearSession(false)
      return
    }
    const next = parseStoredSession(event.newValue)
    if (!next) {
      if (state.session !== null) clearSession(false)
      return
    }
    if (next.refresh_token !== state.session?.refresh_token) {
      generation++
      refreshPending = undefined
      validationPending = undefined
      publish(next)
    } else if (next.access_token !== state.session.access_token) {
      publish(next, state.isLoading)
    }
  }
  const unsubscribe = subscribeToAuth(scheduleExpiry)
  scheduleExpiry()
  const interval = setInterval(check, 60_000)
  window.addEventListener("focus", check)
  window.addEventListener("pageshow", check)
  window.addEventListener("storage", handleStorage)
  document.addEventListener("visibilitychange", check)
  return () => {
    unsubscribe()
    clearTimeout(expiryTimer)
    clearInterval(interval)
    window.removeEventListener("focus", check)
    window.removeEventListener("pageshow", check)
    window.removeEventListener("storage", handleStorage)
    document.removeEventListener("visibilitychange", check)
  }
}

// A request may be replayed only once, and only in the session that started it.
export async function sendAuthenticatedRequest(
  path: string,
  options: RequestOptions = {}
): Promise<Response> {
  if (logoutPending) await logoutPending.catch(() => undefined)
  const attempt = generation
  const session = state.session
  if (!session) throw authError()
  let token = expired(session.access_token)
    ? await refresh()
    : session.access_token
  if (attempt !== generation) throw authError()
  let response = await sendRequest(path, options, token)
  if (attempt !== generation) throw authError()
  if (response.status === 401) {
    // Another request may already have refreshed this rejected token.
    token =
      state.session?.access_token !== token &&
      state.session &&
      !expired(state.session.access_token)
        ? state.session.access_token
        : await refresh()
    if (attempt !== generation) throw authError()
    response = await sendRequest(path, options, token)
    if (attempt !== generation) throw authError()
    if (response.status === 401) clearSession()
  }
  if (!response.ok) throw await parseError(response)
  return response
}
