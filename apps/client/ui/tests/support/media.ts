type MediaQueryListener = (event: MediaQueryListEvent) => void

const queryMatches = new Map<string, boolean>()
const queryListeners = new Map<string, Set<MediaQueryListener>>()

const TOUCH_QUERY = "(hover: none) and (pointer: coarse)"

function createMediaQueryList(query: string): MediaQueryList {
  const addListener = (listener: MediaQueryListener) => {
    const listeners = queryListeners.get(query) ?? new Set()
    listeners.add(listener)
    queryListeners.set(query, listeners)
  }

  const removeListener = (listener: MediaQueryListener) => {
    queryListeners.get(query)?.delete(listener)
  }

  return {
    get matches() {
      return queryMatches.get(query) ?? false
    },
    media: query,
    onchange: null,
    addEventListener(_type: string, listener: EventListenerOrEventListenerObject) {
      if (typeof listener === "function") {
        addListener(listener as MediaQueryListener)
      }
    },
    removeEventListener(
      _type: string,
      listener: EventListenerOrEventListenerObject
    ) {
      if (typeof listener === "function") {
        removeListener(listener as MediaQueryListener)
      }
    },
    addListener(listener: MediaQueryListener) {
      addListener(listener)
    },
    removeListener(listener: MediaQueryListener) {
      removeListener(listener)
    },
    dispatchEvent() {
      return true
    },
  } as MediaQueryList
}

export function installMediaMocks() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => createMediaQueryList(query),
  })
}

export function setMediaQueryMatch(query: string, matches: boolean) {
  queryMatches.set(query, matches)
  const event = { matches, media: query } as MediaQueryListEvent
  for (const listener of queryListeners.get(query) ?? []) {
    listener(event)
  }
}

export function setTouchDevice(enabled: boolean) {
  setMediaQueryMatch(TOUCH_QUERY, enabled)
}

export function resetMediaMocks() {
  queryMatches.clear()
  queryListeners.clear()
}
