type MediaQueryListener = (event: MediaQueryListEvent) => void

const queryMatches = new Map<string, boolean>()
const queryListeners = new Map<string, Set<MediaQueryListener>>()

let orientationListeners = new Set<() => void>()
let currentOrientationType: OrientationType | null = null

const TOUCH_QUERY = "(hover: none) and (pointer: coarse)"
const PORTRAIT_QUERY = "(orientation: portrait)"

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

function createScreenOrientationStub(): ScreenOrientation {
  return {
    get type(): OrientationType {
      return currentOrientationType ?? "landscape-primary"
    },
    angle: 0,
    onchange: null,
    lock: async () => {},
    unlock: () => {},
    addEventListener(type: string, listener: EventListenerOrEventListenerObject) {
      if (type === "change" && typeof listener === "function") {
        orientationListeners.add(listener as () => void)
      }
    },
    removeEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject
    ) {
      if (type === "change" && typeof listener === "function") {
        orientationListeners.delete(listener as () => void)
      }
    },
    dispatchEvent() {
      return true
    },
  } as unknown as ScreenOrientation
}

function applyScreenOrientationProperty(enabled: boolean) {
  Object.defineProperty(window.screen, "orientation", {
    configurable: true,
    value: enabled ? createScreenOrientationStub() : undefined,
  })
}

export function installMediaMocks() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => createMediaQueryList(query),
  })
  currentOrientationType = "landscape-primary"
  applyScreenOrientationProperty(true)
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

export function setScreenOrientation(
  type: "portrait-primary" | "landscape-primary" | null
) {
  if (type === null) {
    currentOrientationType = null
    applyScreenOrientationProperty(false)
    return
  }

  currentOrientationType = type
  applyScreenOrientationProperty(true)
  setMediaQueryMatch(PORTRAIT_QUERY, type.startsWith("portrait"))
  for (const listener of orientationListeners) {
    listener()
  }
  window.dispatchEvent(new Event("orientationchange"))
}

export function resetMediaMocks() {
  queryMatches.clear()
  queryListeners.clear()
  orientationListeners = new Set()
  currentOrientationType = "landscape-primary"
  applyScreenOrientationProperty(true)
}
