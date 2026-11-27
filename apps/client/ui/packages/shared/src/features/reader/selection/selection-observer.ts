import type { BookFormat, BookLocator } from "../contracts/engine"
import type { SelectionContext } from "../contracts/selection"
import {
  CanonicalCfiResolver,
  canonicalCfiResolver,
} from "../engine/epub/canonical-cfi"
import {
  projectSelectionGeometry,
  type SafeViewportInsets,
} from "./geometry"

export interface ShiroAndroidReaderBridge {
  setSuppressSelectionMenu(suppress: boolean): void
  getSafeAreaInsetsJson?: () => string
  setReaderSystemUi?: (
    immersive: boolean,
    hideSystemBars: boolean,
    isDarkSurface: boolean,
    backgroundColorHex: string
  ) => void
  openExternalUrl?: (url: string) => boolean
}

declare global {
  interface Window {
    __SHIRO_ANDROID_READER__?: ShiroAndroidReaderBridge | undefined
  }
}

const DEFAULT_SELECTION_DEBOUNCE_MS = 140

function isEditableNode(target: EventTarget | null): boolean {
  if (!target || typeof target !== "object") return false
  const node = target as { tagName?: unknown; isContentEditable?: unknown }
  if (node.isContentEditable === true) return true
  if (typeof node.tagName !== "string") return false
  const tag = node.tagName.toLowerCase()
  return tag === "input" || tag === "textarea" || tag === "select"
}

export function setAndroidSelectionSuppression(suppress: boolean): void {
  if (typeof window === "undefined") return
  try {
    window.__SHIRO_ANDROID_READER__?.setSuppressSelectionMenu(suppress)
  } catch {
    // Ignore native bridge call failure in non-Android runtimes.
  }
}

export interface SelectionObserverOptions {
  readonly tabId: string
  readonly bookId: string
  readonly bookHash: string
  readonly format: BookFormat
  readonly getContainer: () => HTMLElement | null
  readonly getSectionHref: (sectionIndex: number) => string
  readonly resolver?: CanonicalCfiResolver | undefined
  readonly isAndroid?: (() => boolean) | undefined
  readonly getSafeInsets?: (() => SafeViewportInsets) | undefined
  readonly debounceMs?: number | undefined
  readonly onSelectionChange: (selection: SelectionContext | null) => void
}

interface AttachedSectionDoc {
  readonly sectionIndex: number
  readonly doc: Document
  readonly sectionBaseCfi: string
  readonly controller: AbortController
}

const activeAndroidObservers = new Set<SelectionObserver>()

function recomputeAndroidSelectionSuppression(): void {
  if (activeAndroidObservers.size === 0) {
    setAndroidSelectionSuppression(false)
    return
  }
  const hostActive =
    typeof document !== "undefined" ? document.activeElement : null
  if (isEditableNode(hostActive)) {
    setAndroidSelectionSuppression(false)
    return
  }
  for (const obs of activeAndroidObservers) {
    if (obs.hasFocusedEditableInSections()) {
      setAndroidSelectionSuppression(false)
      return
    }
  }
  setAndroidSelectionSuppression(true)
}

export class SelectionObserver {
  private readonly tabId: string
  private readonly bookId: string
  private readonly bookHash: string
  private readonly format: BookFormat
  private readonly getContainer: () => HTMLElement | null
  private readonly getSectionHref: (sectionIndex: number) => string
  private readonly resolver: CanonicalCfiResolver
  private readonly isAndroid: () => boolean
  private readonly getSafeInsets: () => SafeViewportInsets
  private readonly debounceMs: number
  private readonly onSelectionChange: (selection: SelectionContext | null) => void

  private readonly sections = new Map<number, AttachedSectionDoc>()
  private readonly hostController = new AbortController()
  private debounceTimer: ReturnType<typeof setTimeout> | null = null
  private pointerActive = false
  private currentSelection: SelectionContext | null = null
  private disposed = false

  constructor(options: SelectionObserverOptions) {
    this.tabId = options.tabId
    this.bookId = options.bookId
    this.bookHash = options.bookHash
    this.format = options.format
    this.getContainer = options.getContainer
    this.getSectionHref = options.getSectionHref
    this.resolver = options.resolver ?? canonicalCfiResolver
    this.isAndroid = options.isAndroid ?? (() => false)
    this.getSafeInsets = options.getSafeInsets ?? (() => ({}))
    this.debounceMs = Math.max(
      0,
      options.debounceMs ?? DEFAULT_SELECTION_DEBOUNCE_MS
    )
    this.onSelectionChange = options.onSelectionChange

    this.attachHostListeners()
    if (this.isAndroid()) {
      activeAndroidObservers.add(this)
    }
    recomputeAndroidSelectionSuppression()
  }

  hasFocusedEditableInSections(): boolean {
    for (const entry of this.sections.values()) {
      if (isEditableNode(entry.doc.activeElement)) {
        return true
      }
    }
    return false
  }

  getCurrentSelection(): SelectionContext | null {
    return this.currentSelection
  }

  flushSelectionNow(): SelectionContext | null {
    if (this.disposed) return null
    this.cancelDebounce()
    for (const entry of this.sections.values()) {
      const sel = entry.doc.defaultView?.getSelection()
      if (
        sel &&
        !sel.isCollapsed &&
        sel.rangeCount > 0 &&
        sel.toString().trim().length > 0
      ) {
        this.settleSelection(
          entry.sectionIndex,
          entry.doc,
          entry.sectionBaseCfi
        )
        return this.currentSelection
      }
    }
    return this.currentSelection
  }

  attachSection(
    sectionIndex: number,
    doc: Document,
    sectionBaseCfi: string
  ): void {
    if (this.disposed) return

    const existing = this.sections.get(sectionIndex)
    if (
      existing &&
      existing.doc === doc &&
      existing.sectionBaseCfi === sectionBaseCfi
    ) {
      return
    }
    if (existing) {
      existing.controller.abort()
      this.sections.delete(sectionIndex)
    }

    const controller = new AbortController()
    const { signal } = controller
    this.sections.set(sectionIndex, {
      sectionIndex,
      doc,
      sectionBaseCfi,
      controller,
    })

    const handleDragStart = () => {
      this.pointerActive = true
      this.cancelDebounce()
      if (this.currentSelection !== null) {
        this.emitSelection(null)
      }
    }

    const handleDragEnd = () => {
      this.pointerActive = false
      this.cancelDebounce()
      this.settleSelection(sectionIndex, doc, sectionBaseCfi)
    }

    doc.addEventListener("pointerdown", handleDragStart, { signal })
    doc.addEventListener("touchstart", handleDragStart, {
      signal,
      passive: true,
    })
    doc.addEventListener("pointerup", handleDragEnd, { signal })
    doc.addEventListener("touchend", handleDragEnd, { signal, passive: true })
    doc.addEventListener("pointercancel", handleDragEnd, { signal })
    doc.addEventListener("touchcancel", handleDragEnd, {
      signal,
      passive: true,
    })

    if (typeof doc.defaultView?.addEventListener === "function") {
      doc.defaultView.addEventListener(
        "scroll",
        () => {
          this.cancelDebounce()
          this.emitSelection(null)
        },
        { signal, passive: true }
      )
    }

    doc.addEventListener(
      "selectionchange",
      () => {
        if (this.disposed || this.pointerActive) return
        const sel = doc.defaultView?.getSelection()
        if (
          !sel ||
          sel.isCollapsed ||
          sel.rangeCount === 0 ||
          sel.toString().trim().length === 0
        ) {
          this.cancelDebounce()
          this.emitSelection(null)
          return
        }
        this.scheduleDebounce(() => {
          this.settleSelection(sectionIndex, doc, sectionBaseCfi)
        })
      },
      { signal }
    )

    doc.addEventListener(
      "focusin",
      (event) => {
        if (!this.isAndroid() || this.disposed) return
        if (isEditableNode(event.target)) {
          setAndroidSelectionSuppression(false)
        } else {
          recomputeAndroidSelectionSuppression()
        }
      },
      { signal }
    )

    doc.addEventListener(
      "focusout",
      () => {
        if (!this.isAndroid() || this.disposed) return
        queueMicrotask(() => {
          if (this.disposed) return
          recomputeAndroidSelectionSuppression()
        })
      },
      { signal }
    )
  }

  detachSection(sectionIndex: number): void {
    const entry = this.sections.get(sectionIndex)
    if (!entry) return
    entry.controller.abort()
    this.sections.delete(sectionIndex)
    if (this.currentSelection?.sectionIndex === sectionIndex) {
      this.emitSelection(null)
    }
    if (this.isAndroid() && !this.disposed) {
      recomputeAndroidSelectionSuppression()
    }
  }

  handleRelocate(): void {
    this.pointerActive = false
    this.cancelDebounce()
    this.emitSelection(null)
  }

  clearSelection(): void {
    this.pointerActive = false
    this.cancelDebounce()
    for (const entry of this.sections.values()) {
      entry.doc.defaultView?.getSelection()?.removeAllRanges()
    }
    this.emitSelection(null)
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.cancelDebounce()
    this.hostController.abort()
    for (const entry of this.sections.values()) {
      entry.controller.abort()
    }
    this.sections.clear()
    this.emitSelection(null)
    activeAndroidObservers.delete(this)
    recomputeAndroidSelectionSuppression()
  }

  private attachHostListeners(): void {
    if (typeof window === "undefined") return
    const { signal } = this.hostController

    window.addEventListener(
      "resize",
      () => {
        this.cancelDebounce()
        if (this.currentSelection !== null) {
          this.emitSelection(null)
        }
      },
      { signal }
    )

    window.addEventListener(
      "scroll",
      (event) => {
        const target = event.target
        const container = this.getContainer()
        const affectsViewport =
          target === window ||
          target === document ||
          target === document.documentElement ||
          target === document.body ||
          (container !== null &&
            target instanceof Node &&
            (target === container ||
              target.contains(container) ||
              container.contains(target)))
        if (!affectsViewport) {
          return
        }
        this.cancelDebounce()
        if (this.currentSelection !== null) {
          this.emitSelection(null)
        }
      },
      { signal, capture: true }
    )

    const handleHostPointerEnd = () => {
      if (!this.pointerActive || this.disposed) return
      this.pointerActive = false
      this.cancelDebounce()
      for (const entry of this.sections.values()) {
        const sel = entry.doc.defaultView?.getSelection()
        if (
          sel &&
          !sel.isCollapsed &&
          sel.rangeCount > 0 &&
          sel.toString().trim().length > 0
        ) {
          this.settleSelection(
            entry.sectionIndex,
            entry.doc,
            entry.sectionBaseCfi
          )
          return
        }
      }
      this.emitSelection(null)
    }

    window.addEventListener("pointerup", handleHostPointerEnd, { signal })
    window.addEventListener("pointercancel", handleHostPointerEnd, { signal })

    if (typeof document !== "undefined") {
      document.addEventListener(
        "focusin",
        (event) => {
          if (!this.isAndroid() || this.disposed) return
          if (isEditableNode(event.target)) {
            setAndroidSelectionSuppression(false)
          } else {
            recomputeAndroidSelectionSuppression()
          }
        },
        { signal }
      )

      document.addEventListener(
        "focusout",
        () => {
          if (!this.isAndroid() || this.disposed) return
          queueMicrotask(() => {
            if (this.disposed) return
            recomputeAndroidSelectionSuppression()
          })
        },
        { signal }
      )
    }
  }

  private scheduleDebounce(cb: () => void): void {
    this.cancelDebounce()
    this.debounceTimer = setTimeout(() => {
      this.debounceTimer = null
      if (!this.disposed) {
        cb()
      }
    }, this.debounceMs)
  }

  private cancelDebounce(): void {
    if (this.debounceTimer !== null) {
      clearTimeout(this.debounceTimer)
      this.debounceTimer = null
    }
  }

  private settleSelection(
    sectionIndex: number,
    doc: Document,
    sectionBaseCfi: string
  ): void {
    if (this.disposed) return

    const sel = doc.defaultView?.getSelection()
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
      this.emitSelection(null)
      return
    }

    const text = sel.toString()
    if (text.trim().length === 0) {
      this.emitSelection(null)
      return
    }

    const container = this.getContainer()
    if (!container) {
      this.emitSelection(null)
      return
    }

    const range = sel.getRangeAt(0)
    const iframe =
      (doc.defaultView?.frameElement as HTMLIFrameElement | null | undefined) ??
      null
    const safeInsets = this.getSafeInsets()
    const anchor = projectSelectionGeometry(
      range,
      iframe,
      container.getBoundingClientRect(),
      {
        isAndroid: this.isAndroid(),
        safeTopPx: safeInsets.top,
        safeBottomPx: safeInsets.bottom,
        viewportHeightPx:
          typeof window !== "undefined" ? window.innerHeight : undefined,
      }
    )

    if (!anchor) {
      this.emitSelection(null)
      return
    }

    const canonicalCfi =
      this.format === "EPUB"
        ? this.resolver.rangeToCanonicalCfi(doc, sectionBaseCfi, range)
        : null
    const inspection = this.resolver.inspectSelectionOrigin(range)

    const locator: BookLocator | null =
      this.format === "EPUB" && canonicalCfi
        ? {
            kind: "epub",
            spine_index: sectionIndex,
            href: this.getSectionHref(sectionIndex),
            fragment: null,
            cfi: canonicalCfi,
          }
        : null

    this.emitSelection({
      tabId: this.tabId,
      bookId: this.bookId,
      bookHash: this.bookHash,
      format: this.format,
      sectionIndex,
      locator,
      textOrigin: inspection.textOrigin,
      text,
      anchorBlockOriginalText: inspection.anchorBlockOriginalText,
      anchor,
      clearSelection: () => this.clearSelection(),
    })
  }

  private emitSelection(next: SelectionContext | null): void {
    if (next === null && this.currentSelection === null) return
    this.currentSelection = next
    this.onSelectionChange(next)
  }
}
