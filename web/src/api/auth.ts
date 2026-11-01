import { ApiError, parseError, sendRequest, type RequestOptions } from "./http"

export const AUTH_STORAGE_KEY = "shiro-reader.auth"
type Session = { username: string; access_token: string; refresh_token: string }
type AuthState = { session: Session | null; isLoading: boolean }
let state: AuthState = { session: null, isLoading: true }
let generation = 0
let refreshPending: Promise<string> | undefined
let restorePending: Promise<void> | undefined
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

function expired(token: string): boolean {
  try {
    const payload = token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/")
    const { exp } = JSON.parse(atob(payload)) as { exp?: unknown }
    return (
      typeof exp !== "number" ||
      !Number.isFinite(exp) ||
      exp * 1000 <= Date.now()
    )
  } catch {
    return true
  }
}

function authError() {
  return new ApiError(401, "identity.authentication.invalid_token")
}

export function logout() {
  generation++
  refreshPending = undefined
  persist(null)
  publish(null)
}

export async function login(username: string, password: string): Promise<void> {
  const attempt = ++generation
  refreshPending = undefined
  const response = await sendRequest("auth/login", {
    method: "POST",
    body: { username: username, password },
  })
  if (!response.ok) throw await parseError(response)
  const tokens = (await response.json()) as Partial<Session>
  
  if (typeof tokens.access_token !== "string" || expired(tokens.access_token) ||
      typeof tokens.refresh_token !== "string" || expired(tokens.refresh_token))
    throw authError()

  if (generation !== attempt) 
    throw authError()
  
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
  if (!session) throw authError()
  const pending = (async () => {
    try {
      if (expired(session.refresh_token)) 
        throw authError()
      
      const response = await sendRequest("auth/refresh", {
        method: "POST",
        body: { token: session.refresh_token },
      })
      
      if (!response.ok) 
        throw await parseError(response)
      
      const tokens = (await response.json()) as { access_token?: unknown }
      if (typeof tokens.access_token !== "string" || expired(tokens.access_token))
        throw authError()
      
      if (generation !== attempt) 
        throw authError()
      
      const updated = { ...session, access_token: tokens.access_token }
      persist(updated)
      publish(updated)
      return updated.access_token
    } catch (error) {
      if (generation === attempt)
         logout()
      throw error
    }
  })()
  refreshPending = pending
  try {
    return await pending
  } finally {
    if (refreshPending === pending) 
      refreshPending = undefined
  }
}

export function restoreSession(): Promise<void> {
  if (restorePending) return restorePending
  const attempt = generation
  restorePending = (async () => {
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
        logout()
        return
      }
      const session = {
        username: stored.username,
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
      }
      publish(session, true)
      if (expired(session.access_token)) 
        await refresh()
      else 
        publish(session)

    } catch {
      if (generation === attempt) 
        logout()
    }
  })()
  return restorePending
}

// A request may be replayed only once, and only in the session that started it.
export async function sendAuthenticatedRequest(
  path: string,
  options: RequestOptions = {}
): Promise<Response> {
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
    if (response.status === 401) logout()
  }
  if (!response.ok) throw await parseError(response)
  return response
}
