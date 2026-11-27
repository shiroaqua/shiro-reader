import type { EPUB, EpubSection } from "foliate-js/epub.js"
import type { CompiledReaderStyle } from "../../contracts/settings"

export interface ContinuousScrollRelocateDetail {
  readonly sectionIndex: number
  readonly sectionFraction: number
  readonly bookFraction: number
  readonly range: Range | null
}

export interface ContinuousScrollRendererOptions {
  readonly container: HTMLElement
  readonly book: EPUB
  readonly initialStyle: CompiledReaderStyle
  readonly onSectionLoad: (sectionIndex: number, doc: Document) => void
  readonly onSectionUnmount: (sectionIndex: number) => void
  readonly onRelocate: (detail: ContinuousScrollRelocateDetail) => void
  readonly onExternalLink: (url: string) => void
  readonly onInternalLink: (href: string) => void
  readonly hasSectionSelection?: ((sectionIndex: number) => boolean) | undefined
  readonly loadSectionSrc?:
    | ((section: EpubSection, index: number) => Promise<string | null>)
    | undefined
  readonly mountIframeDoc?:
    | ((
        iframe: HTMLIFrameElement,
        src: string,
        index: number
      ) => Promise<Document | null>)
    | undefined
  readonly relocateDebounceMs?: number | undefined
  readonly maxMountedSections?: number | undefined
}

interface MountedContinuousSlot {
  readonly index: number
  readonly section: EpubSection
  readonly wrapper: HTMLElement
  readonly iframe: HTMLIFrameElement
  readonly doc: Document
  readonly styleEl: HTMLStyleElement
  readonly controller: AbortController
  readonly resizeObserver: ResizeObserver | null
  measuredHeight: number
  verticalPaddingPx: number
  totalSlotHeight: number
}

const DEFAULT_RELOCATE_DEBOUNCE_MS = 120
const DEFAULT_MAX_MOUNTED_SECTIONS = 5
const PRELOAD_MARGIN_MIN_PX = 400

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.min(1, Math.max(0, value))
}

function parseBlockMarginPx(rawMargin: string): number {
  const parsed = Number.parseFloat(rawMargin)
  if (!Number.isFinite(parsed) || parsed < 0) return 20
  return Math.max(8, Math.round(parsed * 0.6))
}

function setStylesImportant(
  el: HTMLElement,
  styles: Readonly<Record<string, string>>
): void {
  const { style } = el
  for (const [k, v] of Object.entries(styles)) {
    style.setProperty(k, v, "important")
  }
}

function defaultLoadSectionSrc(section: EpubSection): Promise<string | null> {
  if (typeof section.load !== "function") {
    return Promise.resolve(null)
  }
  return section.load().then((src) => (typeof src === "string" ? src : null))
}

function defaultMountIframeDoc(
  iframe: HTMLIFrameElement,
  src: string,
  _index: number,
  signal: AbortSignal
): Promise<Document | null> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve(null)
      return
    }
    const onAbort = () => {
      resolve(null)
    }
    const onLoad = () => {
      signal.removeEventListener("abort", onAbort)
      resolve(iframe.contentDocument)
    }
    signal.addEventListener("abort", onAbort, { once: true })
    iframe.addEventListener("load", onLoad, { once: true })
    iframe.src = src
  })
}

function findFirstNonEmptyRect(target: Range | Element): DOMRect | null {
  const rects = target.getClientRects?.()
  if (rects && rects.length > 0) {
    for (const rect of Array.from(rects)) {
      if (rect.width > 0 || rect.height > 0) {
        return rect
      }
    }
    return rects[0] ?? null
  }
  if ("getBoundingClientRect" in target) {
    return target.getBoundingClientRect()
  }
  return null
}

function findVisibleStartRange(doc: Document, localOffset: number): Range | null {
  const body = doc.body
  if (!body) return null

  const walker = doc.createTreeWalker(
    body,
    NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT,
    {
      acceptNode(node) {
        if (node.nodeType === Node.ELEMENT_NODE) {
          const tag = (node as Element).localName?.toLowerCase()
          if (tag === "script" || tag === "style") {
            return NodeFilter.FILTER_REJECT
          }
          return NodeFilter.FILTER_SKIP
        }
        if (!node.nodeValue || node.nodeValue.trim().length === 0) {
          return NodeFilter.FILTER_SKIP
        }
        return NodeFilter.FILTER_ACCEPT
      },
    }
  )

  let firstText: Node | null = null
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!firstText) firstText = node
    try {
      const probe = doc.createRange()
      probe.selectNodeContents(node)
      const rect = probe.getBoundingClientRect()
      if (rect && rect.height > 0 && rect.bottom >= localOffset) {
        const collapsed = doc.createRange()
        collapsed.setStart(node, 0)
        collapsed.collapse(true)
        return collapsed
      }
    } catch {
      // Ignore invalid node range probes.
    }
  }

  try {
    const fallback = doc.createRange()
    if (firstText) {
      fallback.setStart(firstText, 0)
    } else {
      fallback.setStart(body, 0)
    }
    fallback.collapse(true)
    return fallback
  } catch {
    return null
  }
}

export class ContinuousScrollRenderer {
  readonly scrolled = true

  private readonly container: HTMLElement
  private readonly book: EPUB
  private readonly onSectionLoad: (sectionIndex: number, doc: Document) => void
  private readonly onSectionUnmount: (sectionIndex: number) => void
  private readonly onRelocate: (detail: ContinuousScrollRelocateDetail) => void
  private readonly onExternalLink: (url: string) => void
  private readonly onInternalLink: (href: string) => void
  private readonly hasSectionSelection: ((sectionIndex: number) => boolean) | undefined
  private readonly loadSectionSrc: (
    section: EpubSection,
    index: number
  ) => Promise<string | null>
  private readonly mountIframeDocFn: (
    iframe: HTMLIFrameElement,
    src: string,
    index: number,
    signal: AbortSignal
  ) => Promise<Document | null>
  private readonly relocateDebounceMs: number
  private readonly maxMountedSections: number

  private readonly scroller: HTMLDivElement
  private readonly stack: HTMLDivElement
  private readonly rootController = new AbortController()
  private readonly sectionWeights: readonly number[]
  private readonly sectionPrefixWeights: readonly number[]
  private readonly totalLinearWeight: number
  private readonly slots: MountedContinuousSlot[] = []
  private readonly loadingIndices = new Set<number>()

  private currentStyle: CompiledReaderStyle
  private relocateTimer: ReturnType<typeof setTimeout> | null = null
  private ensureInFlight = false
  private ensureQueued = false
  private navigationEpoch = 0
  private destroyed = false

  constructor(options: ContinuousScrollRendererOptions) {
    this.container = options.container
    this.book = options.book
    this.currentStyle = options.initialStyle
    this.onSectionLoad = options.onSectionLoad
    this.onSectionUnmount = options.onSectionUnmount
    this.onRelocate = options.onRelocate
    this.onExternalLink = options.onExternalLink
    this.onInternalLink = options.onInternalLink
    this.hasSectionSelection = options.hasSectionSelection
    this.loadSectionSrc = options.loadSectionSrc ?? defaultLoadSectionSrc
    this.mountIframeDocFn = options.mountIframeDoc
      ? (iframe, src, index) => options.mountIframeDoc!(iframe, src, index)
      : defaultMountIframeDoc
    this.relocateDebounceMs = Math.max(
      0,
      options.relocateDebounceMs ?? DEFAULT_RELOCATE_DEBOUNCE_MS
    )
    this.maxMountedSections = Math.max(
      3,
      options.maxMountedSections ?? DEFAULT_MAX_MOUNTED_SECTIONS
    )

    const weights: number[] = []
    const prefix: number[] = [0]
    let total = 0
    for (const section of this.book.sections) {
      const w = section.linear !== "no" ? Math.max(1, section.size || 1) : 0
      weights.push(w)
      total += w
      prefix.push(total)
    }
    this.sectionWeights = weights
    this.sectionPrefixWeights = prefix
    this.totalLinearWeight = total

    this.scroller = document.createElement("div")
    this.scroller.setAttribute("data-shiro-continuous-scroll", "")
    Object.assign(this.scroller.style, {
      width: "100%",
      height: "100%",
      overflowY: "auto",
      overflowX: "hidden",
      overflowAnchor: "none",
      position: "relative",
      overscrollBehavior: "contain",
      boxSizing: "border-box",
    })

    this.stack = document.createElement("div")
    this.stack.setAttribute("data-shiro-continuous-stack", "")
    Object.assign(this.stack.style, {
      display: "flex",
      flexDirection: "column",
      width: "100%",
      minHeight: "100%",
    })

    this.scroller.appendChild(this.stack)
    this.container.appendChild(this.scroller)

    this.scroller.addEventListener(
      "scroll",
      () => {
        if (this.destroyed) return
        this.scheduleRelocate()
        void this.ensureAdjacentLoaded()
      },
      { signal: this.rootController.signal, passive: true }
    )
  }

  get scrollElement(): HTMLElement {
    return this.scroller
  }

  render(): void {
    if (this.destroyed) return
    for (const slot of this.slots) {
      this.syncSlotHeight(slot)
    }
  }

  setStyles(styles: string | [string, string]): void {
    const nextCss = Array.isArray(styles) ? styles[1] : styles
    this.currentStyle = {
      ...this.currentStyle,
      chapterCss: nextCss,
    }
    for (const slot of this.slots) {
      slot.styleEl.textContent = nextCss
      this.syncSlotHeight(slot)
    }
  }

  applyLayout(style: CompiledReaderStyle): void {
    this.currentStyle = style
    for (const slot of this.slots) {
      this.applyDocStyles(slot)
      this.syncSlotHeight(slot)
    }
  }

  getContents(): Array<{
    index?: number | undefined
    doc: Document
    overlayer?: unknown
  }> {
    return this.slots.map((slot) => ({
      index: slot.index,
      doc: slot.doc,
    }))
  }

  async goTo(target: {
    index: number
    anchor?: ((doc: Document) => Range | Element | number | null) | undefined
  }): Promise<void> {
    if (this.destroyed) return
    const sectionCount = this.book.sections.length
    if (sectionCount === 0) return

    const targetIndex = Math.min(sectionCount - 1, Math.max(0, target.index))
    const existing = this.slots.find((slot) => slot.index === targetIndex)
    if (existing) {
      this.scrollToSlotAnchor(existing, target.anchor)
      this.emitRelocationNow()
      void this.ensureAdjacentLoaded()
      return
    }

    const epoch = ++this.navigationEpoch
    this.clearAllSlots()
    this.loadingIndices.clear()
    this.scroller.scrollTop = 0

    const mounted = await this.mountSlot(targetIndex, "append", epoch)
    if (!mounted || this.destroyed || this.navigationEpoch !== epoch) {
      return
    }

    this.scrollToSlotAnchor(mounted, target.anchor)
    this.emitRelocationNow()
    await this.ensureAdjacentLoaded()
  }

  async goToFraction(fraction: number): Promise<void> {
    if (this.destroyed) return
    const clamped = clamp01(fraction)
    const { index, sectionFraction } = this.fractionToSectionTarget(clamped)
    await this.goTo({
      index,
      anchor: () => sectionFraction,
    })
  }

  async scrollByPage(direction: "prev" | "next"): Promise<void> {
    if (this.destroyed) return
    const viewportHeight = this.getViewportHeight()
    const step = Math.max(80, viewportHeight - 48)
    const maxScroll = Math.max(0, this.getTotalStackHeight() - viewportHeight)
    const nextTop =
      direction === "next"
        ? Math.min(maxScroll, this.scroller.scrollTop + step)
        : Math.max(0, this.scroller.scrollTop - step)

    const animated = this.currentStyle.paginatorAttributes.animated
    if (animated && typeof this.scroller.scrollTo === "function") {
      this.scroller.scrollTo({ top: nextTop, behavior: "smooth" })
    } else {
      this.scroller.scrollTop = nextTop
    }
    this.scheduleRelocate()
    await this.ensureAdjacentLoaded()
  }

  destroy(): void {
    if (this.destroyed) return
    this.destroyed = true
    this.navigationEpoch += 1
    if (this.relocateTimer !== null) {
      clearTimeout(this.relocateTimer)
      this.relocateTimer = null
    }
    this.rootController.abort()
    this.clearAllSlots()
    this.loadingIndices.clear()
    this.scroller.remove()
  }

  private adjacentLinearIndex(
    fromIndex: number,
    direction: -1 | 1
  ): number | null {
    const sections = this.book.sections
    for (
      let index = fromIndex + direction;
      index >= 0 && index < sections.length;
      index += direction
    ) {
      if (sections[index]?.linear !== "no") {
        return index
      }
    }
    return null
  }

  private fractionToSectionTarget(bookFraction: number): {
    readonly index: number
    readonly sectionFraction: number
  } {
    const sections = this.book.sections
    if (sections.length === 0) {
      return { index: 0, sectionFraction: 0 }
    }
    if (this.totalLinearWeight <= 0) {
      const idx = Math.min(
        sections.length - 1,
        Math.max(0, Math.floor(bookFraction * (sections.length - 1)))
      )
      return { index: idx, sectionFraction: 0 }
    }

    const firstLinear = this.adjacentLinearIndex(-1, 1) ?? 0
    const lastLinear = this.adjacentLinearIndex(sections.length, -1) ?? sections.length - 1

    if (bookFraction <= 0) return { index: firstLinear, sectionFraction: 0 }
    if (bookFraction >= 1) return { index: lastLinear, sectionFraction: 1 }

    const targetWeight = bookFraction * this.totalLinearWeight
    for (let i = 0; i < sections.length; i++) {
      const weight = this.sectionWeights[i] ?? 0
      if (weight <= 0) continue
      const startW = this.sectionPrefixWeights[i] ?? 0
      const endW = startW + weight
      if (targetWeight <= endW || i === lastLinear) {
        return {
          index: i,
          sectionFraction: clamp01((targetWeight - startW) / weight),
        }
      }
    }
    return { index: lastLinear, sectionFraction: 1 }
  }

  private computeBookFraction(
    sectionIndex: number,
    sectionFraction: number
  ): number {
    if (this.totalLinearWeight <= 0) {
      const maxIdx = Math.max(1, this.book.sections.length - 1)
      return clamp01((sectionIndex + sectionFraction) / maxIdx)
    }

    const weight = this.sectionWeights[sectionIndex] ?? 0
    if (weight > 0) {
      const before = this.sectionPrefixWeights[sectionIndex] ?? 0
      return clamp01(
        (before + clamp01(sectionFraction) * weight) / this.totalLinearWeight
      )
    }

    const before = this.sectionPrefixWeights[sectionIndex] ?? 0
    return clamp01(before / this.totalLinearWeight)
  }

  private getViewportHeight(): number {
    return this.scroller.clientHeight > 0 ? this.scroller.clientHeight : 600
  }

  private getSlotBounds(slotIdx: number): {
    readonly top: number
    readonly height: number
  } {
    const slot = this.slots[slotIdx]
    if (!slot) return { top: 0, height: 0 }
    if (slot.wrapper.offsetHeight > 0) {
      return {
        top: slot.wrapper.offsetTop,
        height: slot.wrapper.offsetHeight,
      }
    }
    let top = 0
    for (let i = 0; i < slotIdx; i++) {
      const prev = this.slots[i]!
      top +=
        prev.wrapper.offsetHeight > 0
          ? prev.wrapper.offsetHeight
          : prev.totalSlotHeight
    }
    return {
      top,
      height: slot.totalSlotHeight,
    }
  }

  private getTotalStackHeight(): number {
    if (this.scroller.scrollHeight > 0) {
      return this.scroller.scrollHeight
    }
    let sum = 0
    for (const slot of this.slots) {
      sum +=
        slot.wrapper.offsetHeight > 0
          ? slot.wrapper.offsetHeight
          : slot.totalSlotHeight
    }
    return sum
  }

  private applyDocStyles(slot: MountedContinuousSlot): void {
    const doc = slot.doc
    const attrs = this.currentStyle.paginatorAttributes
    const verticalPaddingPx = parseBlockMarginPx(attrs.margin)
    slot.verticalPaddingPx = verticalPaddingPx
    slot.wrapper.style.paddingBlock = `${verticalPaddingPx}px`

    const gap = attrs.gap || "6%"
    const maxInline = attrs.maxInlineSize || "760px"

    if (doc.documentElement) {
      setStylesImportant(doc.documentElement, {
        "box-sizing": "border-box",
        padding: `0 ${gap}`,
        "column-width": "auto",
        height: "auto",
        width: "auto",
        overflow: "visible",
      })
    }
    if (doc.body) {
      setStylesImportant(doc.body, {
        "max-width": maxInline,
        margin: "0 auto",
        background: "none",
      })
      for (const mediaEl of Array.from(
        doc.body.querySelectorAll("img, svg, video")
      )) {
        if (mediaEl instanceof HTMLElement) {
          setStylesImportant(mediaEl, {
            "max-width": "100%",
            height: "auto",
            "object-fit": "contain",
            "box-sizing": "border-box",
          })
        }
      }
    }
    slot.styleEl.textContent = this.currentStyle.chapterCss
  }

  private syncSlotHeight(slot: MountedContinuousSlot): number {
    const doc = slot.doc
    const measured = Math.max(
      1,
      doc.documentElement?.scrollHeight ?? 0,
      doc.body?.scrollHeight ?? 0,
      Math.ceil(doc.body?.getBoundingClientRect?.().height ?? 0)
    )
    const idx = this.slots.indexOf(slot)
    const prevTotal =
      slot.wrapper.offsetHeight > 0
        ? slot.wrapper.offsetHeight
        : slot.totalSlotHeight
    const prevBounds = idx >= 0 ? this.getSlotBounds(idx) : { top: 0, height: prevTotal }
    const isAboveViewport =
      idx >= 0 &&
      prevTotal > 0 &&
      prevBounds.top + prevBounds.height <= this.scroller.scrollTop + 2

    slot.iframe.style.height = `${measured}px`
    slot.measuredHeight = measured
    slot.totalSlotHeight = measured + slot.verticalPaddingPx * 2

    const nextTotal =
      slot.wrapper.offsetHeight > 0
        ? slot.wrapper.offsetHeight
        : slot.totalSlotHeight
    if (isAboveViewport && nextTotal !== prevTotal) {
      this.scroller.scrollTop = Math.max(
        0,
        this.scroller.scrollTop + (nextTotal - prevTotal)
      )
    }
    return nextTotal
  }

  private attachDocLinkListener(
    doc: Document,
    section: EpubSection,
    signal: AbortSignal
  ): void {
    doc.addEventListener(
      "click",
      (event: Event) => {
        const target = event.target as Element | null
        const anchor = target?.closest?.("a[href]") as HTMLAnchorElement | null
        if (!anchor) return
        const rawHref = anchor.getAttribute("href")
        if (!rawHref) return
        event.preventDefault()

        const resolvedHref = section.resolveHref?.(rawHref) ?? rawHref
        const isExternal =
          this.book.isExternal?.(resolvedHref) ??
          /^(?!blob:)[a-z][a-z0-9+.-]*:/i.test(resolvedHref)
        if (isExternal) {
          this.onExternalLink(resolvedHref)
        } else {
          this.onInternalLink(resolvedHref)
        }
      },
      { signal }
    )
  }

  private async mountSlot(
    sectionIndex: number,
    position: "append" | "prepend",
    epoch: number
  ): Promise<MountedContinuousSlot | null> {
    const section = this.book.sections[sectionIndex]
    if (!section) return null
    if (
      this.slots.some((s) => s.index === sectionIndex) ||
      this.loadingIndices.has(sectionIndex)
    ) {
      return this.slots.find((s) => s.index === sectionIndex) ?? null
    }

    this.loadingIndices.add(sectionIndex)
    const slotController = new AbortController()
    const wrapper = document.createElement("section")
    wrapper.setAttribute("data-shiro-section-index", String(sectionIndex))
    const verticalPaddingPx = parseBlockMarginPx(
      this.currentStyle.paginatorAttributes.margin
    )
    Object.assign(wrapper.style, {
      width: "100%",
      flex: "0 0 auto",
      boxSizing: "border-box",
      paddingBlock: "0px",
    })

    const iframe = document.createElement("iframe")
    iframe.setAttribute("sandbox", "allow-same-origin allow-scripts")
    iframe.setAttribute("scrolling", "no")
    Object.assign(iframe.style, {
      border: "0",
      display: "block",
      width: "100%",
      height: "0px",
      overflow: "hidden",
    })
    wrapper.appendChild(iframe)

    if (position === "prepend" && this.stack.firstChild) {
      this.stack.insertBefore(wrapper, this.stack.firstChild)
    } else {
      this.stack.appendChild(wrapper)
    }

    try {
      const src = await this.loadSectionSrc(section, sectionIndex)
      if (
        !src ||
        this.destroyed ||
        this.navigationEpoch !== epoch ||
        slotController.signal.aborted
      ) {
        wrapper.remove()
        return null
      }

      const doc = await this.mountIframeDocFn(
        iframe,
        src,
        sectionIndex,
        slotController.signal
      )
      if (
        !doc ||
        this.destroyed ||
        this.navigationEpoch !== epoch ||
        slotController.signal.aborted
      ) {
        wrapper.remove()
        section.unload?.()
        return null
      }

      const styleEl = doc.createElement("style")
      styleEl.setAttribute("data-shiro-continuous-style", "")
      const styleParent = doc.head ?? doc.documentElement
      styleParent?.appendChild(styleEl)

      if (!doc.documentElement.lang && this.book.metadata?.language) {
        const lang = Array.isArray(this.book.metadata.language)
          ? this.book.metadata.language[0]
          : this.book.metadata.language
        if (typeof lang === "string" && lang.trim()) {
          doc.documentElement.lang = lang.trim()
        }
      }

      const slot: MountedContinuousSlot = {
        index: sectionIndex,
        section,
        wrapper,
        iframe,
        doc,
        styleEl,
        controller: slotController,
        resizeObserver: null,
        measuredHeight: 0,
        verticalPaddingPx,
        totalSlotHeight: 0,
      }

      if (position === "prepend") {
        this.slots.unshift(slot)
      } else {
        this.slots.push(slot)
      }

      this.applyDocStyles(slot)

      const measured = Math.max(
        1,
        doc.documentElement?.scrollHeight ?? 0,
        doc.body?.scrollHeight ?? 0,
        Math.ceil(doc.body?.getBoundingClientRect?.().height ?? 0)
      )
      iframe.style.height = `${measured}px`
      slot.measuredHeight = measured
      slot.totalSlotHeight = measured + slot.verticalPaddingPx * 2

      if (position === "prepend") {
        const addedHeight =
          wrapper.offsetHeight > 0 ? wrapper.offsetHeight : slot.totalSlotHeight
        this.scroller.scrollTop = Math.max(
          0,
          this.scroller.scrollTop + addedHeight
        )
      }

      let resizeObserver: ResizeObserver | null = null
      if (typeof ResizeObserver !== "undefined" && doc.body) {
        resizeObserver = new ResizeObserver(() => {
          if (!this.destroyed && this.slots.includes(slot)) {
            this.syncSlotHeight(slot)
          }
        })
        resizeObserver.observe(doc.body)
      }
      ;(slot as { resizeObserver: ResizeObserver | null }).resizeObserver =
        resizeObserver

      void doc.fonts?.ready
        ?.then(() => {
          if (!this.destroyed && this.slots.includes(slot)) {
            this.syncSlotHeight(slot)
          }
        })
        .catch(() => {})

      this.attachDocLinkListener(doc, section, slotController.signal)
      this.onSectionLoad(sectionIndex, doc)
      return slot
    } finally {
      this.loadingIndices.delete(sectionIndex)
    }
  }

  private removeSlotAt(slotArrayIndex: number): void {
    const slot = this.slots[slotArrayIndex]
    if (!slot) return
    this.slots.splice(slotArrayIndex, 1)
    slot.resizeObserver?.disconnect()
    slot.controller.abort()
    this.onSectionUnmount(slot.index)
    slot.wrapper.remove()
    try {
      slot.section.unload?.()
    } catch {
      // Ignore section unload errors.
    }
  }

  private clearAllSlots(): void {
    while (this.slots.length > 0) {
      this.removeSlotAt(0)
    }
  }

  private findActiveSlotIndex(): number {
    if (this.slots.length === 0) return -1
    const viewportHeight = this.getViewportHeight()
    const refY =
      this.scroller.scrollTop +
      Math.min(48, Math.max(16, viewportHeight * 0.15))

    for (let i = 0; i < this.slots.length; i++) {
      const bounds = this.getSlotBounds(i)
      if (refY <= bounds.top + bounds.height) {
        return i
      }
    }
    return this.slots.length - 1
  }

  private pruneDistantSlots(preloadMargin: number): void {
    if (this.slots.length <= this.maxMountedSections) return

    const viewportTop = this.scroller.scrollTop
    const viewportBottom = viewportTop + this.getViewportHeight()
    const minKeep = viewportTop - preloadMargin
    const maxKeep = viewportBottom + preloadMargin

    while (this.slots.length > this.maxMountedSections) {
      const activeIdx = this.findActiveSlotIndex()
      const first = this.slots[0]
      const last = this.slots[this.slots.length - 1]
      if (!first || !last) break

      const firstBounds = this.getSlotBounds(0)
      const lastBounds = this.getSlotBounds(this.slots.length - 1)

      const canPruneFirst =
        activeIdx >= 2 &&
        firstBounds.top + firstBounds.height < minKeep &&
        !this.hasSectionSelection?.(first.index)
      const canPruneLast =
        this.slots.length - 1 - activeIdx >= 2 &&
        lastBounds.top > maxKeep &&
        !this.hasSectionSelection?.(last.index)

      if (!canPruneFirst && !canPruneLast) {
        break
      }

      if (canPruneFirst && (!canPruneLast || activeIdx >= this.slots.length - 1 - activeIdx)) {
        const removedHeight =
          first.wrapper.offsetHeight > 0
            ? first.wrapper.offsetHeight
            : first.totalSlotHeight
        this.removeSlotAt(0)
        this.scroller.scrollTop = Math.max(
          0,
          this.scroller.scrollTop - removedHeight
        )
      } else if (canPruneLast) {
        this.removeSlotAt(this.slots.length - 1)
      } else {
        break
      }
    }
  }

  private async ensureAdjacentLoaded(): Promise<void> {
    if (this.destroyed || this.slots.length === 0) return
    if (this.ensureInFlight) {
      this.ensureQueued = true
      return
    }
    this.ensureInFlight = true
    try {
      const epoch = this.navigationEpoch
      const viewportHeight = this.getViewportHeight()
      const preloadMargin = Math.max(PRELOAD_MARGIN_MIN_PX, viewportHeight * 1.5)

      let progressed = true
      while (
        progressed &&
        !this.destroyed &&
        this.navigationEpoch === epoch &&
        this.slots.length > 0 &&
        this.slots.length < this.maxMountedSections + 2
      ) {
        progressed = false
        const firstSlot = this.slots[0]!
        const lastSlot = this.slots[this.slots.length - 1]!
        const totalHeight = this.getTotalStackHeight()
        const bottomGap =
          totalHeight - (this.scroller.scrollTop + viewportHeight)
        const topGap = this.scroller.scrollTop

        if (bottomGap <= preloadMargin) {
          const nextIdx = this.adjacentLinearIndex(lastSlot.index, 1)
          if (
            nextIdx !== null &&
            !this.slots.some((s) => s.index === nextIdx) &&
            !this.loadingIndices.has(nextIdx)
          ) {
            const mounted = await this.mountSlot(nextIdx, "append", epoch)
            if (mounted) {
              progressed = true
              continue
            }
          }
        }

        if (topGap <= preloadMargin) {
          const prevIdx = this.adjacentLinearIndex(firstSlot.index, -1)
          if (
            prevIdx !== null &&
            !this.slots.some((s) => s.index === prevIdx) &&
            !this.loadingIndices.has(prevIdx)
          ) {
            const mounted = await this.mountSlot(prevIdx, "prepend", epoch)
            if (mounted) {
              progressed = true
            }
          }
        }
      }

      if (!this.destroyed && this.navigationEpoch === epoch) {
        this.pruneDistantSlots(preloadMargin)
      }
    } finally {
      this.ensureInFlight = false
      if (this.ensureQueued && !this.destroyed) {
        this.ensureQueued = false
        void this.ensureAdjacentLoaded()
      }
    }
  }

  private scrollToSlotAnchor(
    slot: MountedContinuousSlot,
    anchor?: ((doc: Document) => Range | Element | number | null) | undefined
  ): void {
    const idx = this.slots.indexOf(slot)
    if (idx < 0) return
    const bounds = this.getSlotBounds(idx)
    const viewportHeight = this.getViewportHeight()
    const resolved = typeof anchor === "function" ? anchor(slot.doc) : undefined

    if (typeof resolved === "number" && Number.isFinite(resolved)) {
      const frac = clamp01(resolved)
      const scrollable = Math.max(0, bounds.height - viewportHeight)
      this.scroller.scrollTop = Math.max(0, bounds.top + frac * scrollable)
      return
    }

    if (resolved && typeof resolved === "object") {
      const rect = findFirstNonEmptyRect(resolved)
      if (rect) {
        this.scroller.scrollTop = Math.max(
          0,
          bounds.top + slot.verticalPaddingPx + rect.top - 16
        )
        return
      }
    }

    this.scroller.scrollTop = Math.max(0, bounds.top)
  }

  private scheduleRelocate(): void {
    if (this.relocateTimer !== null) {
      clearTimeout(this.relocateTimer)
    }
    this.relocateTimer = setTimeout(() => {
      this.relocateTimer = null
      if (!this.destroyed) {
        this.emitRelocationNow()
      }
    }, this.relocateDebounceMs)
  }

  private emitRelocationNow(): void {
    if (this.destroyed || this.slots.length === 0) return
    const activeSlotIdx = this.findActiveSlotIndex()
    if (activeSlotIdx < 0) return
    const slot = this.slots[activeSlotIdx]!
    const bounds = this.getSlotBounds(activeSlotIdx)
    const viewportHeight = this.getViewportHeight()
    const totalHeight = this.getTotalStackHeight()

    const localOffset = Math.max(0, this.scroller.scrollTop - bounds.top)
    const hasNextLinear =
      this.adjacentLinearIndex(slot.index, 1) !== null
    const atVeryBottom =
      !hasNextLinear &&
      totalHeight > 0 &&
      this.scroller.scrollTop + viewportHeight >= totalHeight - 2

    const sectionFraction = atVeryBottom
      ? 1
      : clamp01(localOffset / Math.max(1, bounds.height))

    const range = findVisibleStartRange(
      slot.doc,
      Math.max(0, localOffset - slot.verticalPaddingPx)
    )
    const bookFraction = this.computeBookFraction(slot.index, sectionFraction)

    this.onRelocate({
      sectionIndex: slot.index,
      sectionFraction,
      bookFraction,
      range,
    })
  }
}
