import {
  CanonicalCfiResolver,
  canonicalCfiResolver,
} from "./canonical-cfi"

export interface RelayoutTargetRenderer {
  render?(): void
  goTo?(target: {
    index: number
    anchor?: ((doc: Document) => Range | Element | number | null) | undefined
  }): Promise<void>
}

export interface RelayoutAnchorSnapshot {
  readonly sectionIndex: number
  readonly anchorCfi: string
}

export interface RelayoutCoordinatorOptions {
  readonly getRenderer: () => RelayoutTargetRenderer | null | undefined
  readonly captureAnchor: () => RelayoutAnchorSnapshot | null
  readonly hasActiveSelection?: (() => boolean) | undefined
  readonly resolver?: CanonicalCfiResolver | undefined
  readonly throttleMs?: number | undefined
  readonly scheduleFrame?: ((cb: () => void) => number) | undefined
  readonly cancelFrame?: ((handle: number) => void) | undefined
}

const DEFAULT_THROTTLE_MS = 180

export class RelayoutCoordinator {
  private readonly getRenderer: () => RelayoutTargetRenderer | null | undefined
  private readonly captureAnchor: () => RelayoutAnchorSnapshot | null
  private readonly hasActiveSelection: () => boolean
  private readonly resolver: CanonicalCfiResolver
  private readonly throttleMs: number
  private readonly scheduleFrame: (cb: () => void) => number
  private readonly cancelFrame: (handle: number) => void

  private throttleTimerId: ReturnType<typeof setTimeout> | null = null
  private frameHandle: number | null = null
  private pending = false
  private deferredDueToSelection = false
  private disposed = false

  constructor(options: RelayoutCoordinatorOptions) {
    this.getRenderer = options.getRenderer
    this.captureAnchor = options.captureAnchor
    this.hasActiveSelection = options.hasActiveSelection ?? (() => false)
    this.resolver = options.resolver ?? canonicalCfiResolver
    this.throttleMs = Math.max(0, options.throttleMs ?? DEFAULT_THROTTLE_MS)
    const hasNativeRaf =
      typeof globalThis.requestAnimationFrame === "function" &&
      typeof globalThis.cancelAnimationFrame === "function"
    this.scheduleFrame =
      options.scheduleFrame ??
      ((cb) =>
        hasNativeRaf
          ? globalThis.requestAnimationFrame(() => cb())
          : (globalThis.setTimeout(cb, 0) as unknown as number))
    this.cancelFrame =
      options.cancelFrame ??
      ((handle) => {
        if (hasNativeRaf) {
          globalThis.cancelAnimationFrame(handle)
        } else {
          globalThis.clearTimeout(
            handle as unknown as ReturnType<typeof setTimeout>
          )
        }
      })
  }

  scheduleRelayout(): void {
    if (this.disposed) return
    this.pending = true
    if (this.throttleTimerId !== null || this.frameHandle !== null) {
      return
    }

    this.throttleTimerId = globalThis.setTimeout(() => {
      this.throttleTimerId = null
      if (this.disposed || !this.pending) return
      let ranSynchronously = false
      const handle = this.scheduleFrame(() => {
        ranSynchronously = true
        this.frameHandle = null
        this.flush()
      })
      if (!ranSynchronously) {
        this.frameHandle = handle
      }
    }, this.throttleMs)
  }

  flush(): void {
    if (this.disposed || !this.pending) return
    this.pending = false
    if (this.throttleTimerId !== null) {
      globalThis.clearTimeout(this.throttleTimerId)
      this.throttleTimerId = null
    }
    if (this.frameHandle !== null) {
      this.cancelFrame(this.frameHandle)
      this.frameHandle = null
    }

    const renderer = this.getRenderer()
    if (!renderer || typeof renderer.render !== "function") {
      return
    }

    if (this.hasActiveSelection()) {
      this.deferredDueToSelection = true
      return
    }
    this.deferredDueToSelection = false

    const snapshot = this.captureAnchor()
    renderer.render()

    if (snapshot !== null && typeof renderer.goTo === "function") {
      const goToPromise = renderer.goTo({
        index: snapshot.sectionIndex,
        anchor: (doc: Document) =>
          this.resolver.resolveVisualAnchor(doc, snapshot.anchorCfi),
      })
      void Promise.resolve(goToPromise).catch(() => {
        // Ignore navigation cancellation if section changed mid-relayout.
      })
    }
  }

  notifySelectionCleared(): void {
    if (this.disposed || !this.deferredDueToSelection) return
    if (this.hasActiveSelection()) return
    this.deferredDueToSelection = false
    this.scheduleRelayout()
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.pending = false
    this.deferredDueToSelection = false

    if (this.throttleTimerId !== null) {
      globalThis.clearTimeout(this.throttleTimerId)
      this.throttleTimerId = null
    }
    if (this.frameHandle !== null) {
      this.cancelFrame(this.frameHandle)
      this.frameHandle = null
    }
  }
}
