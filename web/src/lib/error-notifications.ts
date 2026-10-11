export type ErrorNotification = { id: number; message: string; createdAt: number }

let notifications: ErrorNotification[] = []
let nextId = 0
const listeners = new Set<() => void>()
let dismissTimer: ReturnType<typeof setTimeout> | undefined
let scheduledId: number | undefined

function scheduleOldestError() {
  const oldestId = notifications[0]?.id
  if (oldestId === scheduledId) return

  clearTimeout(dismissTimer)
  dismissTimer = undefined
  scheduledId = oldestId
  if (oldestId !== undefined) {
    dismissTimer = setTimeout(() => dismissError(oldestId), 3000)
  }
}

export function notifyError(message: string) {
  const id = ++nextId
  notifications = [...notifications, { id, message, createdAt: Date.now() }]
    .sort((a, b) => a.createdAt - b.createdAt || a.id - b.id)
  scheduleOldestError()
  listeners.forEach((listener) => listener())
}

export function dismissError(id: number) {
  if (!notifications.some((notification) => notification.id === id)) return
  notifications = notifications.filter((notification) => notification.id !== id)
  scheduleOldestError()
  listeners.forEach((listener) => listener())
}

export function getErrorNotifications() {
  return notifications
}

export function subscribeToErrors(listener: () => void) {
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}
