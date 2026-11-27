import type { EPUB, EpubDataTransformDetail } from "foliate-js/epub.js"

const MAX_TAP_DISPLACEMENT_PX = 6
const MAX_TAP_DURATION_MS = 300
const DEFAULT_BOUNDARY_EDGE_DWELL_MS = 150
const DEFAULT_BOUNDARY_WHEEL_THRESHOLD_PX = 80
const DEFAULT_BOUNDARY_TOUCH_THRESHOLD_PX = 56
const DEFAULT_BOUNDARY_SWITCH_COOLDOWN_MS = 450
const BOUNDARY_WHEEL_RESET_GAP_MS = 400

const MARKUP_MEDIA_TYPES = new Set([
  "application/xhtml+xml",
  "text/html",
  "image/svg+xml",
  "application/xml",
  "text/xml",
])

const URI_ATTRIBUTES = [
  "href",
  "src",
  "action",
  "formaction",
  "data",
  "poster",
] as const

const SMIL_ANIMATION_TAGS = new Set([
  "set",
  "animate",
  "animatecolor",
  "animatemotion",
  "animatetransform",
])

const INTERACTIVE_TAP_SELECTOR =
  'a, audio, video, summary, button, input, textarea, select, label, [role="button"], [role="link"]'

const OUTER_IGNORE_TAP_SELECTOR =
  'button, a, input, textarea, select, [role="button"], [data-reader-chrome]'

const FORWARDED_NAVIGATION_KEYS = new Set([
  "ArrowLeft",
  "ArrowRight",
  "PageUp",
  "PageDown",
  " ",
  "Home",
  "End",
  "Escape",
  "F11",
])

function normalizeMediaType(rawType: string): string {
  return rawType.split(";")[0]?.trim().toLowerCase() ?? ""
}

function isMarkupMediaType(mediaType: string): boolean {
  const normalized = normalizeMediaType(mediaType)
  return MARKUP_MEDIA_TYPES.has(normalized) || normalized.endsWith("+xml")
}

function decodeBasicNumericEntities(value: string): string {
  return value.replace(/&#(\d+|x[\da-fA-F]+);?/g, (_, raw: string) => {
    const code = raw.startsWith("x") || raw.startsWith("X")
      ? Number.parseInt(raw.slice(1), 16)
      : Number.parseInt(raw, 10)
    return Number.isFinite(code) && code > 0 && code <= 0x10ffff
      ? String.fromCodePoint(code)
      : ""
  })
}

function isDangerousUriValue(rawValue: string): boolean {
  const decoded = decodeBasicNumericEntities(rawValue)
  let stripped = ""
  for (let i = 0; i < decoded.length; i++) {
    const code = decoded.charCodeAt(i)
    if (code > 0x20) {
      stripped += decoded[i]
    }
  }
  const normalized = stripped.toLowerCase()
  return (
    normalized.startsWith("javascript:") ||
    normalized.startsWith("vbscript:") ||
    normalized.startsWith("data:text/html") ||
    normalized.startsWith("data:application/xhtml+xml") ||
    normalized.startsWith("data:image/svg+xml") ||
    normalized.startsWith("data:application/xml") ||
    normalized.startsWith("data:text/xml") ||
    /^data:[^,;]+\+xml(?:[;,]|$)/.test(normalized)
  )
}

function isDangerousAttributeTarget(rawAttrName: string): boolean {
  const lower = rawAttrName.trim().toLowerCase()
  if (lower.length === 0) return false
  if (lower.startsWith("on") || lower === "srcdoc" || lower === "xlink:href") {
    return true
  }
  const local = lower.includes(":") ? (lower.split(":").pop() ?? lower) : lower
  return (URI_ATTRIBUTES as readonly string[]).includes(local)
}

function stripDangerousProcessingInstructions(node: Node): void {
  const children = Array.from(node.childNodes)
  for (const child of children) {
    if (
      child.nodeType === Node.PROCESSING_INSTRUCTION_NODE &&
      (child as ProcessingInstruction).target.toLowerCase() === "xml-stylesheet"
    ) {
      child.parentNode?.removeChild(child)
    } else if (child.hasChildNodes()) {
      stripDangerousProcessingInstructions(child)
    }
  }
}

function stripDangerousContainer(root: Document | DocumentFragment): void {
  stripDangerousProcessingInstructions(root)

  const toRemove: Element[] = []
  const allElements = root.querySelectorAll("*")
  for (let i = 0; i < allElements.length; i++) {
    const el = allElements[i]
    if (!el) continue
    const localName = el.localName.toLowerCase()
    if (localName === "script" || localName === "base") {
      toRemove.push(el)
      continue
    }

    // Unconditionally strip <meta http-equiv="refresh">: prevents both silent external
    // iframe navigation and quote/newline regex bypasses for javascript: URIs.
    if (
      localName === "meta" &&
      el.getAttribute("http-equiv")?.trim().toLowerCase() === "refresh"
    ) {
      toRemove.push(el)
      continue
    }

    // Strip SVG SMIL elements that animate URI/event attributes or carry dangerous URI values.
    if (SMIL_ANIMATION_TAGS.has(localName)) {
      const attrName = el.getAttribute("attributeName") ?? ""
      const toVal = el.getAttribute("to") ?? ""
      const fromVal = el.getAttribute("from") ?? ""
      const valuesVal = el.getAttribute("values") ?? ""
      const hasDangerousValue =
        isDangerousUriValue(toVal) ||
        isDangerousUriValue(fromVal) ||
        valuesVal.split(";").some((part) => isDangerousUriValue(part))
      if (isDangerousAttributeTarget(attrName) || hasDangerousValue) {
        toRemove.push(el)
        continue
      }
    }

    const attrs = Array.from(el.attributes)
    for (const attr of attrs) {
      const lowerName = attr.name.toLowerCase()
      const lowerLocal = attr.localName.toLowerCase()
      if (lowerName.startsWith("on") || lowerName === "srcdoc") {
        el.removeAttributeNode(attr)
        continue
      }
      if (
        (URI_ATTRIBUTES as readonly string[]).includes(lowerName) ||
        (URI_ATTRIBUTES as readonly string[]).includes(lowerLocal) ||
        lowerName === "xlink:href"
      ) {
        if (isDangerousUriValue(attr.value)) {
          el.removeAttributeNode(attr)
        }
      }
    }

    if (
      localName === "template" &&
      "content" in el &&
      el.content instanceof DocumentFragment
    ) {
      stripDangerousContainer(el.content)
    }
  }

  for (const el of toRemove) {
    el.remove()
  }
}

function stripDangerousDomNodes(doc: Document): void {
  stripDangerousContainer(doc)
}

function serializeDoctype(doc: Document): string {
  const dt = doc.doctype
  if (!dt) return ""
  const publicPart = dt.publicId ? ` PUBLIC "${dt.publicId}"` : ""
  const systemPart = dt.systemId
    ? `${dt.publicId ? "" : " SYSTEM"} "${dt.systemId}"`
    : ""
  return `<!DOCTYPE ${dt.name}${publicPart}${systemPart}>\n`
}

export function sanitizeMarkupDocumentWithType(
  markup: string,
  mediaType: string
): { readonly markup: string; readonly mediaType: string } {
  const normalizedType = normalizeMediaType(mediaType)
  if (!isMarkupMediaType(normalizedType) || markup.length === 0) {
    return { markup, mediaType }
  }

  const parser = new DOMParser()
  let effectiveType:
    | "application/xhtml+xml"
    | "text/html"
    | "image/svg+xml"
    | "application/xml"
    | "text/xml" =
    normalizedType === "text/html"
      ? "text/html"
      : normalizedType === "image/svg+xml"
        ? "image/svg+xml"
        : normalizedType === "application/xhtml+xml"
          ? "application/xhtml+xml"
          : normalizedType === "text/xml"
            ? "text/xml"
            : "application/xml"
  let outputMediaType = normalizedType

  let doc = parser.parseFromString(markup, effectiveType)
  const hasParseError =
    doc.getElementsByTagName("parsererror").length > 0 ||
    !doc.documentElement

  if (
    hasParseError &&
    effectiveType !== "text/html" &&
    effectiveType !== "image/svg+xml"
  ) {
    effectiveType = "text/html"
    outputMediaType = "text/html"
    doc = parser.parseFromString(markup, "text/html")
  } else if (hasParseError && effectiveType === "image/svg+xml") {
    return {
      markup: '<svg xmlns="http://www.w3.org/2000/svg"/>',
      mediaType: "image/svg+xml",
    }
  }

  stripDangerousDomNodes(doc)

  if (effectiveType === "text/html") {
    const html = doc.documentElement?.outerHTML ?? ""
    return {
      markup: `${serializeDoctype(doc)}${html}`,
      mediaType: "text/html",
    }
  }

  return {
    markup: new XMLSerializer().serializeToString(doc),
    mediaType: outputMediaType,
  }
}

export function sanitizeMarkupDocument(markup: string, mediaType: string): string {
  return sanitizeMarkupDocumentWithType(markup, mediaType).markup
}

export function attachBookSanitizer(book: EPUB, signal?: AbortSignal): void {
  const target = book.transformTarget
  if (!target) return

  target.addEventListener(
    "data",
    (event: Event) => {
      const detail = (event as CustomEvent<EpubDataTransformDetail>).detail
      if (!detail) return

      if (typeof detail.type === "string" && !isMarkupMediaType(detail.type)) {
        return
      }

      const originalData = detail.data
      const originalType = detail.type
      const sanitizedPromise = Promise.all([
        Promise.resolve(originalData),
        Promise.resolve(originalType),
      ]).then(async ([resolvedData, resolvedType]) => {
        if (!isMarkupMediaType(resolvedType)) {
          return { data: resolvedData, mediaType: resolvedType }
        }
        const text =
          typeof resolvedData === "string"
            ? resolvedData
            : resolvedData instanceof Blob
              ? await resolvedData.text()
              : null
        if (text === null) {
          return { data: resolvedData, mediaType: resolvedType }
        }
        const result = sanitizeMarkupDocumentWithType(text, resolvedType)
        return { data: result.markup, mediaType: result.mediaType }
      })
      detail.data = sanitizedPromise.then((res) => res.data)
      detail.type = sanitizedPromise.then((res) => res.mediaType)
    },
    signal ? { signal } : undefined
  )
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0.5
  return Math.min(1, Math.max(0, value))
}

export function projectClientPointToNormalizedViewport(
  clientX: number,
  clientY: number,
  iframe: HTMLIFrameElement | null | undefined,
  containerRect: Pick<DOMRectReadOnly, "left" | "top" | "width" | "height">
): { readonly normX: number; readonly normY: number } {
  let hostX = clientX
  let hostY = clientY

  if (iframe) {
    const iframeRect = iframe.getBoundingClientRect()
    const baseWidth =
      iframe.offsetWidth > 0 ? iframe.offsetWidth : iframeRect.width
    const baseHeight =
      iframe.offsetHeight > 0 ? iframe.offsetHeight : iframeRect.height
    const scaleX = baseWidth > 0 ? iframeRect.width / baseWidth : 1
    const scaleY = baseHeight > 0 ? iframeRect.height / baseHeight : 1
    hostX = iframeRect.left + clientX * scaleX
    hostY = iframeRect.top + clientY * scaleY
  }

  const width = Math.max(1, containerRect.width)
  const height = Math.max(1, containerRect.height)

  return {
    normX: clamp01((hostX - containerRect.left) / width),
    normY: clamp01((hostY - containerRect.top) / height),
  }
}

function isEditableElement(target: EventTarget | null): boolean {
  if (!target || typeof target !== "object") return false
  const node = target as { tagName?: unknown; isContentEditable?: unknown }
  if (node.isContentEditable === true) return true
  if (typeof node.tagName !== "string") return false
  const tag = node.tagName.toLowerCase()
  return tag === "input" || tag === "textarea" || tag === "select"
}

function isInsideInteractiveSelector(
  target: EventTarget | null,
  selector: string
): boolean {
  if (!target || typeof target !== "object") return false
  const maybeEl = target as Partial<Element> & { parentElement?: Element | null }
  if (typeof maybeEl.closest === "function") {
    return maybeEl.closest(selector) !== null
  }
  return maybeEl.parentElement?.closest(selector) != null
}

function hasNonCollapsedSelection(doc: Document): boolean {
  const sel = doc.defaultView?.getSelection()
  return sel != null && !sel.isCollapsed && sel.toString().trim().length > 0
}

export interface ScrollBoundaryState {
  readonly atTop: boolean
  readonly atBottom: boolean
  readonly hasPrev: boolean
  readonly hasNext: boolean
}

export interface SectionLifecycleBridgeOptions {
  readonly container: HTMLElement
  readonly outerContainer?: HTMLElement | undefined
  readonly onViewportTap: (normX: number, normY: number) => void
  readonly onKeyCommand: (event: KeyboardEvent) => void
  readonly isBoundaryScrollEnabled?: (() => boolean) | undefined
  readonly getScrollBoundaryState?: (() => ScrollBoundaryState | null) | undefined
  readonly onBoundaryChapterSwitch?:
    | ((direction: "prev" | "next") => void)
    | undefined
  readonly boundaryEdgeDwellMs?: number | undefined
  readonly boundaryWheelThresholdPx?: number | undefined
  readonly boundaryTouchThresholdPx?: number | undefined
  readonly boundarySwitchCooldownMs?: number | undefined
  readonly now?: (() => number) | undefined
}

interface MountedSectionController {
  readonly doc: Document
  readonly controller: AbortController
}

export class SectionLifecycleBridge {
  private readonly container: HTMLElement
  private readonly tapContainer: HTMLElement
  private readonly onViewportTap: (normX: number, normY: number) => void
  private readonly onKeyCommand: (event: KeyboardEvent) => void
  private readonly isBoundaryScrollEnabled: () => boolean
  private readonly getScrollBoundaryState: () => ScrollBoundaryState | null
  private readonly onBoundaryChapterSwitch:
    | ((direction: "prev" | "next") => void)
    | undefined
  private readonly boundaryEdgeDwellMs: number
  private readonly boundaryWheelThresholdPx: number
  private readonly boundaryTouchThresholdPx: number
  private readonly boundarySwitchCooldownMs: number
  private readonly now: () => number

  private readonly sectionControllers = new Map<number, MountedSectionController>()
  private readonly outerController = new AbortController()
  private lastScrollMovedAt = -Infinity
  private lastBoundarySwitchAt = -Infinity
  private wheelOverscrollAcc = 0
  private wheelOverscrollDir: "prev" | "next" | null = null
  private lastWheelOverscrollAt = -Infinity
  private disposed = false

  constructor(options: SectionLifecycleBridgeOptions) {
    this.container = options.container
    this.tapContainer = options.outerContainer ?? options.container
    this.onViewportTap = options.onViewportTap
    this.onKeyCommand = options.onKeyCommand
    this.isBoundaryScrollEnabled = options.isBoundaryScrollEnabled ?? (() => false)
    this.getScrollBoundaryState = options.getScrollBoundaryState ?? (() => null)
    this.onBoundaryChapterSwitch = options.onBoundaryChapterSwitch
    this.boundaryEdgeDwellMs = Math.max(
      0,
      options.boundaryEdgeDwellMs ?? DEFAULT_BOUNDARY_EDGE_DWELL_MS
    )
    this.boundaryWheelThresholdPx = Math.max(
      1,
      options.boundaryWheelThresholdPx ?? DEFAULT_BOUNDARY_WHEEL_THRESHOLD_PX
    )
    this.boundaryTouchThresholdPx = Math.max(
      1,
      options.boundaryTouchThresholdPx ?? DEFAULT_BOUNDARY_TOUCH_THRESHOLD_PX
    )
    this.boundarySwitchCooldownMs = Math.max(
      0,
      options.boundarySwitchCooldownMs ?? DEFAULT_BOUNDARY_SWITCH_COOLDOWN_MS
    )
    this.now = options.now ?? (() => Date.now())

    this.attachOuterFallbackTapListener()
    this.attachBoundaryScrollListeners(
      this.container,
      this.outerController.signal,
      null
    )
  }

  notifyScrollActivity(): void {
    this.lastScrollMovedAt = this.now()
    this.wheelOverscrollAcc = 0
    this.wheelOverscrollDir = null
  }

  mountSection(sectionIndex: number, doc: Document): void {
    if (this.disposed) return

    const existing = this.sectionControllers.get(sectionIndex)
    if (existing && existing.doc === doc) {
      return
    }
    if (existing) {
      existing.controller.abort()
      this.sectionControllers.delete(sectionIndex)
    }

    const controller = new AbortController()
    const { signal } = controller
    this.sectionControllers.set(sectionIndex, { doc, controller })

    doc.addEventListener(
      "contextmenu",
      (event: Event) => {
        event.preventDefault()
      },
      { signal }
    )

    doc.addEventListener(
      "keydown",
      (event: KeyboardEvent) => {
        if (isEditableElement(event.target)) return
        const hasDisallowedModifiers = event.ctrlKey || event.metaKey || event.altKey
        const isCopyShortcut =
          (event.ctrlKey || event.metaKey) &&
          !event.altKey &&
          event.key.toLowerCase() === "c"
        const isPlainNavKey =
          FORWARDED_NAVIGATION_KEYS.has(event.key) &&
          !hasDisallowedModifiers &&
          (event.key === " " || !event.shiftKey)
        if (isCopyShortcut || isPlainNavKey) {
          if (
            isPlainNavKey &&
            (event.key === " " || event.key === "PageUp" || event.key === "PageDown")
          ) {
            event.preventDefault()
          }
          this.onKeyCommand(event)
        }
      },
      { signal }
    )

    let pointerState:
      | {
          readonly pointerId: number
          readonly startX: number
          readonly startY: number
          readonly downAt: number
          readonly hadSelection: boolean
        }
      | null = null

    doc.addEventListener(
      "pointerdown",
      (event: PointerEvent) => {
        if (event.button !== 0 || event.isPrimary === false) {
          pointerState = null
          return
        }
        pointerState = {
          pointerId: event.pointerId,
          startX: event.clientX,
          startY: event.clientY,
          downAt: this.now(),
          hadSelection: hasNonCollapsedSelection(doc),
        }
      },
      { signal }
    )

    doc.addEventListener(
      "pointerup",
      (event: PointerEvent) => {
        const current = pointerState
        pointerState = null
        if (!current || current.pointerId !== event.pointerId) return
        if (event.button !== 0) return

        const dx = event.clientX - current.startX
        const dy = event.clientY - current.startY
        const elapsed = this.now() - current.downAt
        if (
          Math.hypot(dx, dy) >= MAX_TAP_DISPLACEMENT_PX ||
          elapsed >= MAX_TAP_DURATION_MS
        ) {
          return
        }
        if (current.hadSelection || hasNonCollapsedSelection(doc)) {
          return
        }
        if (isInsideInteractiveSelector(event.target, INTERACTIVE_TAP_SELECTOR)) {
          return
        }

        const iframe =
          (doc.defaultView?.frameElement as HTMLIFrameElement | null | undefined) ??
          null
        const containerRect = this.container.getBoundingClientRect()
        const { normX, normY } = projectClientPointToNormalizedViewport(
          event.clientX,
          event.clientY,
          iframe,
          containerRect
        )
        this.onViewportTap(normX, normY)
      },
      { signal }
    )

    doc.addEventListener(
      "pointercancel",
      () => {
        pointerState = null
      },
      { signal }
    )

    this.attachBoundaryScrollListeners(doc, signal, doc)
  }

  unmountSection(sectionIndex: number): void {
    const entry = this.sectionControllers.get(sectionIndex)
    if (!entry) return
    entry.controller.abort()
    this.sectionControllers.delete(sectionIndex)
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.outerController.abort()
    for (const entry of this.sectionControllers.values()) {
      entry.controller.abort()
    }
    this.sectionControllers.clear()
  }

  private hasActiveSectionSelection(): boolean {
    for (const entry of this.sectionControllers.values()) {
      if (hasNonCollapsedSelection(entry.doc)) {
        return true
      }
    }
    return false
  }

  private attachOuterFallbackTapListener(): void {
    const { signal } = this.outerController
    let pointerState:
      | {
          readonly pointerId: number
          readonly startX: number
          readonly startY: number
          readonly downAt: number
          readonly hadSelection: boolean
        }
      | null = null

    this.tapContainer.addEventListener(
      "contextmenu",
      (event: Event) => {
        if (isInsideInteractiveSelector(event.target, OUTER_IGNORE_TAP_SELECTOR)) {
          return
        }
        event.preventDefault()
      },
      { signal }
    )

    this.tapContainer.addEventListener(
      "pointerdown",
      (event: PointerEvent) => {
        if (event.button !== 0 || event.isPrimary === false) {
          pointerState = null
          return
        }
        if (isInsideInteractiveSelector(event.target, OUTER_IGNORE_TAP_SELECTOR)) {
          pointerState = null
          return
        }
        pointerState = {
          pointerId: event.pointerId,
          startX: event.clientX,
          startY: event.clientY,
          downAt: this.now(),
          hadSelection: this.hasActiveSectionSelection(),
        }
      },
      { signal }
    )

    this.tapContainer.addEventListener(
      "pointerup",
      (event: PointerEvent) => {
        const current = pointerState
        pointerState = null
        if (!current || current.pointerId !== event.pointerId) return
        if (event.button !== 0) return

        const dx = event.clientX - current.startX
        const dy = event.clientY - current.startY
        const elapsed = this.now() - current.downAt
        if (
          Math.hypot(dx, dy) >= MAX_TAP_DISPLACEMENT_PX ||
          elapsed >= MAX_TAP_DURATION_MS
        ) {
          return
        }
        if (isInsideInteractiveSelector(event.target, OUTER_IGNORE_TAP_SELECTOR)) {
          return
        }

        if (current.hadSelection || this.hasActiveSectionSelection()) {
          return
        }

        const containerRect = this.container.getBoundingClientRect()
        const { normX, normY } = projectClientPointToNormalizedViewport(
          event.clientX,
          event.clientY,
          null,
          containerRect
        )
        this.onViewportTap(normX, normY)
      },
      { signal }
    )

    this.tapContainer.addEventListener(
      "pointercancel",
      () => {
        pointerState = null
      },
      { signal }
    )
  }

  private resetWheelOverscroll(): void {
    this.wheelOverscrollAcc = 0
    this.wheelOverscrollDir = null
  }

  private attachBoundaryScrollListeners(
    target: EventTarget,
    signal: AbortSignal,
    doc: Document | null
  ): void {
    target.addEventListener(
      "wheel",
      (rawEvent: Event) => {
        const event = rawEvent as WheelEvent
        if (!this.isBoundaryScrollEnabled()) {
          this.resetWheelOverscroll()
          return
        }
        if (event.ctrlKey || event.metaKey) return

        const scale =
          event.deltaMode === 1 ? 32 : event.deltaMode === 2 ? 240 : 1
        const dy = event.deltaY * scale
        if (!Number.isFinite(dy) || Math.abs(dy) < 1) return

        const state = this.getScrollBoundaryState()
        if (!state) {
          this.resetWheelOverscroll()
          return
        }

        const dir: "prev" | "next" | null =
          dy > 0 && state.atBottom && state.hasNext
            ? "next"
            : dy < 0 && state.atTop && state.hasPrev
              ? "prev"
              : null

        if (!dir) {
          this.resetWheelOverscroll()
          return
        }

        const now = this.now()
        if (
          now - this.lastBoundarySwitchAt < this.boundarySwitchCooldownMs ||
          now - this.lastScrollMovedAt < this.boundaryEdgeDwellMs
        ) {
          this.resetWheelOverscroll()
          return
        }

        if (
          this.wheelOverscrollDir !== dir ||
          now - this.lastWheelOverscrollAt > BOUNDARY_WHEEL_RESET_GAP_MS
        ) {
          this.wheelOverscrollAcc = 0
        }

        this.wheelOverscrollDir = dir
        this.lastWheelOverscrollAt = now
        this.wheelOverscrollAcc += Math.abs(dy)

        if (this.wheelOverscrollAcc >= this.boundaryWheelThresholdPx) {
          event.preventDefault()
          this.resetWheelOverscroll()
          this.lastBoundarySwitchAt = now
          this.onBoundaryChapterSwitch?.(dir)
        }
      },
      { signal, passive: false }
    )

    let touchGesture:
      | {
          readonly startX: number
          readonly startY: number
          readonly startedAtTop: boolean
          readonly startedAtBottom: boolean
          readonly hasPrev: boolean
          readonly hasNext: boolean
          pendingDir: "prev" | "next" | null
        }
      | null = null

    const hasSelection = () =>
      doc ? hasNonCollapsedSelection(doc) : this.hasActiveSectionSelection()

    target.addEventListener(
      "touchstart",
      (rawEvent: Event) => {
        const event = rawEvent as TouchEvent
        if (!this.isBoundaryScrollEnabled() || event.touches.length !== 1) {
          touchGesture = null
          return
        }
        if (hasSelection()) {
          touchGesture = null
          return
        }
        const state = this.getScrollBoundaryState()
        if (!state || (!state.atTop && !state.atBottom)) {
          touchGesture = null
          return
        }
        const touch = event.touches[0] ?? event.changedTouches[0]
        if (!touch) {
          touchGesture = null
          return
        }
        touchGesture = {
          startX: touch.screenX,
          startY: touch.screenY,
          startedAtTop: state.atTop,
          startedAtBottom: state.atBottom,
          hasPrev: state.hasPrev,
          hasNext: state.hasNext,
          pendingDir: null,
        }
      },
      { signal, passive: true }
    )

    target.addEventListener(
      "touchmove",
      (rawEvent: Event) => {
        const event = rawEvent as TouchEvent
        if (!touchGesture) return
        if (
          !this.isBoundaryScrollEnabled() ||
          event.touches.length !== 1 ||
          hasSelection()
        ) {
          touchGesture = null
          return
        }
        const touch = event.touches[0] ?? event.changedTouches[0]
        if (!touch) return

        const dx = touch.screenX - touchGesture.startX
        const dy = touch.screenY - touchGesture.startY
        if (Math.abs(dy) <= Math.abs(dx) * 1.2) {
          touchGesture.pendingDir = null
          return
        }
        if (
          dy <= -this.boundaryTouchThresholdPx &&
          touchGesture.startedAtBottom &&
          touchGesture.hasNext
        ) {
          touchGesture.pendingDir = "next"
        } else if (
          dy >= this.boundaryTouchThresholdPx &&
          touchGesture.startedAtTop &&
          touchGesture.hasPrev
        ) {
          touchGesture.pendingDir = "prev"
        } else {
          touchGesture.pendingDir = null
        }
      },
      { signal, passive: true }
    )

    target.addEventListener(
      "touchend",
      () => {
        const gesture = touchGesture
        touchGesture = null
        const dir = gesture?.pendingDir ?? null
        if (!dir || !this.isBoundaryScrollEnabled()) return

        const now = this.now()
        if (now - this.lastBoundarySwitchAt < this.boundarySwitchCooldownMs) {
          return
        }
        const state = this.getScrollBoundaryState()
        if (!state) return
        if (
          (dir === "next" && state.atBottom && state.hasNext) ||
          (dir === "prev" && state.atTop && state.hasPrev)
        ) {
          this.lastBoundarySwitchAt = now
          this.onBoundaryChapterSwitch?.(dir)
        }
      },
      { signal, passive: true }
    )

    target.addEventListener(
      "touchcancel",
      () => {
        touchGesture = null
      },
      { signal, passive: true }
    )
  }
}
