import type { TranslationMessage } from "@/lib/i18n"

export type ErrorNotification = {
  id: number
  translation: TranslationMessage
  createdAt: number
}

const MAX_NOTIFICATIONS = 5
const AUTO_DISMISS_MS = 3000

let notifications: ErrorNotification[] = []
let nextId = 0
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

export function notifyError(translation: TranslationMessage) {
  const now = Date.now()
  const existingIndex = notifications.findIndex(
    (item) =>
      item.translation.source === translation.source &&
      item.translation.key === translation.key
  )
  const next =
    existingIndex !== -1
      ? [
          ...notifications.slice(0, existingIndex),
          ...notifications.slice(existingIndex + 1),
          { id: notifications[existingIndex].id, translation, createdAt: now },
        ]
      : [...notifications, { id: ++nextId, translation, createdAt: now }]

  notifications = next
    .sort((a, b) => a.createdAt - b.createdAt || a.id - b.id)
    .slice(-MAX_NOTIFICATIONS)
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
