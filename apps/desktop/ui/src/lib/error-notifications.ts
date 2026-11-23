import type { LocalTranslationKey, TranslationMessage } from "@/lib/i18n"

export type RustErrorEvent = {
  operationId: string
  errorId: string
  code: number
  key: string
  locale: "zh" | "en"
  message: string
  itemId: string | null
}

export type ErrorNotification = {
  id: number
  errorId?: string
  translation: TranslationMessage
  createdAt: number
}

const MAX_NOTIFICATIONS = 5
const MAX_SEEN_ERROR_IDS = 256
const AUTO_DISMISS_MS = 3000

let notifications: ErrorNotification[] = []
let nextId = 0
const seenErrorIds = new Set<string>()
const listeners = new Set<() => void>()
let dismissTimer: ReturnType<typeof setTimeout> | undefined
let scheduledTarget: { id: number; createdAt: number } | undefined

function scheduleOldestError() {
  const oldest = notifications[0]
  if (
    oldest?.id === scheduledTarget?.id &&
    oldest?.createdAt === scheduledTarget?.createdAt
  ) {
    return
  }

  clearTimeout(dismissTimer)
  dismissTimer = undefined
  scheduledTarget = oldest
    ? { id: oldest.id, createdAt: oldest.createdAt }
    : undefined
  if (oldest !== undefined) {
    const delay = Math.max(0, oldest.createdAt + AUTO_DISMISS_MS - Date.now())
    dismissTimer = setTimeout(() => dismissError(oldest.id), delay)
  }
}

function rememberErrorId(errorId: string): boolean {
  if (seenErrorIds.has(errorId)) return false
  if (seenErrorIds.size >= MAX_SEEN_ERROR_IDS) {
    const oldest = seenErrorIds.values().next().value
    if (oldest !== undefined) seenErrorIds.delete(oldest)
  }
  seenErrorIds.add(errorId)
  return true
}

export function markRustErrorHandled(errorId: string) {
  rememberErrorId(errorId)
}

export function notifyRustError(event: RustErrorEvent) {
  if (!rememberErrorId(event.errorId)) return
  enqueue({
    source: "rust",
    key: event.key,
    message: event.message,
    errorId: event.errorId,
  })
}

export function notifyLocalError(key: LocalTranslationKey) {
  enqueue({ source: "local", key })
}

function enqueue(translation: TranslationMessage) {
  const existing = translation.source === "local"
    ? notifications.find((item) =>
        item.translation.source === "local" &&
        item.translation.key === translation.key
      )
    : undefined
  notifications = [
    ...notifications.filter((item) => item !== existing),
    {
      id: existing?.id ?? ++nextId,
      errorId: translation.source === "rust" ? translation.errorId : undefined,
      translation,
      createdAt: Date.now(),
    },
  ].slice(-MAX_NOTIFICATIONS)
  scheduleOldestError()
  listeners.forEach((listener) => listener())
}

export function dismissError(id: number) {
  dismissErrors([id])
}

export function dismissErrors(ids: readonly number[]) {
  if (ids.length === 0) return
  const remove = new Set(ids)
  const filtered = notifications.filter((item) => !remove.has(item.id))
  if (filtered.length === notifications.length) return
  notifications = filtered
  scheduleOldestError()
  listeners.forEach((listener) => listener())
}

export function getErrorNotifications() {
  return notifications
}

export function subscribeToErrors(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function resetErrorNotifications() {
  clearTimeout(dismissTimer)
  dismissTimer = undefined
  scheduledTarget = undefined
  seenErrorIds.clear()
  nextId = 0
  if (notifications.length === 0) return
  notifications = []
  listeners.forEach((listener) => listener())
}
