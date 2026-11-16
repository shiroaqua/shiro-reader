import { ApiError, parseError, sendRequest, type RequestOptions } from "./http"

export const AUTH_STORAGE_KEY = "shiro-reader.auth"
// Axum serves HEAD for GET routes; this checks authentication without a book payload.
const SESSION_CHECK_PATH = "library/bookshelf/books"
const SESSION_CHECK_OPTIONS = { method: "HEAD", cache: "no-store" } as const
type Session = { username: string; access_token: string; refresh_token: string }
type AuthState = { session: Session | null; isLoading: boolean }
let state: AuthState = { session: null, isLoading: true }
let generation = 0
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

function expiresAt(token: string): number {
  try {
    const payload = token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/")
    const { exp } = JSON.parse(atob(payload)) as { exp?: unknown }
    return typeof exp === "number" && Number.isFinite(exp) ? exp * 1000 : 0
  } catch {
    return 0
  }
}

function expired(token: string): boolean {
  return expiresAt(token) <= Date.now()
}

function authError() {
  return new ApiError(401, {
    source: "local",
    key: "local.auth.session_expired",
  })
}

export function clearSession() {
  generation++
  refreshPending = undefined
  validationPending = undefined
  persist(null)
  publish(null)
}

export async function logout(): Promise<void> {
  if (logoutPending) return logoutPending
  const session = state.session
  if (!session) return
  // Retire in-flight requests so their failures or refreshes cannot change this session.
  const attempt = ++generation
  refreshPending = undefined
  validationPending = undefined
  const pending = (async () => {
    const response = await sendRequest("auth/logout", {
      method: "POST",
      body: { token: session.refresh_token },
    })
    if (generation !== attempt) throw authError()
    if (response.status !== 204) {
      throw new ApiError(response.status, {
        source: "local",
        key: "local.auth.logout_failed",
      })
    }

    // Bypass automatic refresh and invalidation: failed verification must keep the UI open.
    const [refreshResponse, accessResponse] = await Promise.all([
      sendRequest("auth/refresh", {
        method: "POST",
        body: { token: session.refresh_token },
      }),
      sendRequest(SESSION_CHECK_PATH, SESSION_CHECK_OPTIONS, session.access_token),
    ])
    if (generation !== attempt) throw authError()
    if (refreshResponse.status !== 401 || accessResponse.status !== 401) {
      throw new ApiError(0, {
        source: "local",
        key: "local.auth.logout_failed",
      })
    }
    clearSession()
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
  const tokens = (await response.json()) as Partial<Session>
  if (
    typeof tokens.access_token !== "string" || expired(tokens.access_token) ||
    typeof tokens.refresh_token !== "string" || expired(tokens.refresh_token)
  ) throw authError()
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
      if (expired(session.refresh_token)) throw authError()
      const response = await sendRequest("auth/refresh", {
        method: "POST",
        body: { token: session.refresh_token },
      })
      if (!response.ok) throw await parseError(response)
      const tokens = (await response.json()) as { access_token?: unknown }
      if (typeof tokens.access_token !== "string" || expired(tokens.access_token)) {
        throw authError()
      }
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
  if (expired(session.refresh_token)) {
    clearSession()
    return
  }
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
      const stored: unknown = JSON.parse(
        localStorage.getItem(AUTH_STORAGE_KEY) ?? "null"
      )
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
        clearSession()
        return
      }
      publish({
        username: stored.username,
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
      }, true)
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
    expiryTimer = setTimeout(check, Math.max(0, Math.min(expiry - Date.now(), 2_147_483_647)))
  }
  const unsubscribe = subscribeToAuth(scheduleExpiry)
  scheduleExpiry()
  const interval = setInterval(check, 60_000)
  window.addEventListener("focus", check)
  window.addEventListener("pageshow", check)
  document.addEventListener("visibilitychange", check)
  return () => {
    unsubscribe()
    clearTimeout(expiryTimer)
    clearInterval(interval)
    window.removeEventListener("focus", check)
    window.removeEventListener("pageshow", check)
    document.removeEventListener("visibilitychange", check)
  }
}

// A request may be replayed only once, and only in the session that started it.
export async function sendAuthenticatedRequest(
  path: string,
  options: RequestOptions = {}
): Promise<Response> {
  if (logoutPending) await logoutPending
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
