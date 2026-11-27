import type {
  SectionAugmenter,
  TranslatableBlockHandle,
} from "../../contracts/augmentation"
import type { EphemeralReaderViewState } from "../../contracts/engine"
import {
  getDocumentTranslationMode,
  indexTranslatableBlocks,
  isElementDisplayNone,
  isHostBlockVisuallyHidden,
  isSyntheticElement,
  safeIntersectsNode,
  setDocumentTranslationMode,
  SHIRO_ANCHOR_FOR_ATTR,
  SHIRO_SYNTHETIC_ATTR,
} from "./xhtml-dom"

export interface AugmentationPipelineOptions {
  readonly lookaheadCount?: number | undefined
  readonly requestRelayout: () => void
  readonly getViewportContainer?: (() => HTMLElement | null) | undefined
}

function hasNonZeroBox(el: Element): boolean {
  if (typeof el.getBoundingClientRect !== "function") return false
  const rect = el.getBoundingClientRect()
  return rect.width > 0 || rect.height > 0
}

function isNodeEffectivelyHidden(doc: Document, node: Node | null): boolean {
  if (!node) return false
  const mode = getDocumentTranslationMode(doc)
  let current: Node | null = node
  while (current !== null) {
    if (current.nodeType === 1) {
      const el = current as Element
      if (isElementDisplayNone(doc, el)) return true
      if (isSyntheticElement(el) && mode === "original") return true
      if (!isSyntheticElement(el) && isHostBlockVisuallyHidden(doc, el)) {
        return true
      }
    }
    current = current.parentNode
  }
  return false
}

function resolveRangeEndNode(range: Range): Node | null {
  const { endContainer, endOffset } = range
  if (endContainer.nodeType === 1 && endOffset > 0) {
    const child = endContainer.childNodes[endOffset - 1]
    if (child) return child
  }
  return endContainer
}

function isElementInHostViewport(
  el: Element,
  iframe: HTMLIFrameElement,
  containerRect: DOMRectReadOnly
): boolean {
  const rect = el.getBoundingClientRect()
  if (rect.width <= 0 && rect.height <= 0) return false
  const iframeRect = iframe.getBoundingClientRect()
  const baseWidth = iframe.offsetWidth > 0 ? iframe.offsetWidth : iframeRect.width
  const baseHeight =
    iframe.offsetHeight > 0 ? iframe.offsetHeight : iframeRect.height
  const scaleX = baseWidth > 0 ? iframeRect.width / baseWidth : 1
  const scaleY = baseHeight > 0 ? iframeRect.height / baseHeight : 1
  const left = iframeRect.left + rect.left * scaleX
  const right = left + rect.width * scaleX
  const top = iframeRect.top + rect.top * scaleY
  const bottom = top + rect.height * scaleY
  return (
    right > containerRect.left &&
    left < containerRect.right &&
    bottom > containerRect.top &&
    top < containerRect.bottom
  )
}

interface MountedSectionState {
  readonly doc: Document
  readonly blocks: readonly TranslatableBlockHandle[]
}

const DEFAULT_LOOKAHEAD_COUNT = 10

export class AugmentationPipeline {
  private readonly lookaheadCount: number
  private readonly requestRelayout: () => void
  private readonly getViewportContainer: (() => HTMLElement | null) | undefined
  private readonly augmenters: SectionAugmenter[] = []
  private readonly sections = new Map<number, MountedSectionState>()
  private viewState: EphemeralReaderViewState = {
    translationMode: "original",
    targetLanguage: null,
  }
  private activeWindowSectionIndex: number | null = null
  private activeWindowSignature: string | null = null
  private activeAbortController: AbortController | null = null
  private disposed = false

  constructor(options: AugmentationPipelineOptions) {
    this.lookaheadCount = Math.max(
      0,
      options.lookaheadCount ?? DEFAULT_LOOKAHEAD_COUNT
    )
    this.requestRelayout = options.requestRelayout
    this.getViewportContainer = options.getViewportContainer
  }

  registerAugmenter(augmenter: SectionAugmenter): () => void {
    if (this.disposed) return () => {}
    if (!this.augmenters.includes(augmenter)) {
      this.augmenters.push(augmenter)
      for (const [sectionIndex, state] of this.sections.entries()) {
        augmenter.onSectionMount({
          sectionIndex,
          doc: state.doc,
          mode: this.viewState,
        })
      }
    }
    return () => {
      this.unregisterAugmenter(augmenter.id)
    }
  }

  unregisterAugmenter(augmenterId: string): void {
    const idx = this.augmenters.findIndex((item) => item.id === augmenterId)
    if (idx >= 0) {
      this.augmenters.splice(idx, 1)
    }
  }

  getViewState(): EphemeralReaderViewState {
    return this.viewState
  }

  getIndexedBlocks(sectionIndex: number): readonly TranslatableBlockHandle[] {
    return this.sections.get(sectionIndex)?.blocks ?? []
  }

  mountSection(
    sectionIndex: number,
    doc: Document
  ): readonly TranslatableBlockHandle[] {
    if (this.disposed) return []

    const existing = this.sections.get(sectionIndex)
    if (existing && existing.doc === doc) {
      return existing.blocks
    }

    if (existing) {
      this.unmountSection(sectionIndex)
    }

    setDocumentTranslationMode(doc, this.viewState.translationMode)
    const blocks = indexTranslatableBlocks(doc, sectionIndex)
    this.sections.set(sectionIndex, { doc, blocks })

    for (const augmenter of [...this.augmenters]) {
      augmenter.onSectionMount({
        sectionIndex,
        doc,
        mode: this.viewState,
      })
    }

    return blocks
  }

  handleRelocate(
    sectionIndex: number,
    doc: Document,
    visibleRange: Range | undefined
  ): void {
    if (this.disposed) return

    const blocks = this.mountSection(sectionIndex, doc)
    if (!visibleRange || blocks.length === 0) return

    const syntheticByBlockId = new Map<string, Element>()
    const syntheticNodes = doc.querySelectorAll(
      `[${SHIRO_SYNTHETIC_ATTR}="translation"][${SHIRO_ANCHOR_FOR_ATTR}]`
    )
    for (const node of Array.from(syntheticNodes)) {
      const anchorFor = node.getAttribute(SHIRO_ANCHOR_FOR_ATTR)
      if (anchorFor && !syntheticByBlockId.has(anchorFor)) {
        syntheticByBlockId.set(anchorFor, node)
      }
    }

    const visibleBlocks: TranslatableBlockHandle[] = []
    let lastVisibleIdx = -1

    // Guard against foliate-js paginator.js getVisibleRange() returning a stretched range
    // on Page 1 / Scroll-top-0 when display:none elements report (0, 0) client rects.
    const rangeEndHidden = isNodeEffectivelyHidden(
      doc,
      resolveRangeEndNode(visibleRange)
    )
    const iframe =
      (doc.defaultView?.frameElement as HTMLIFrameElement | null | undefined) ??
      null
    const container = this.getViewportContainer?.() ?? null
    const containerRect =
      container && typeof container.getBoundingClientRect === "function"
        ? container.getBoundingClientRect()
        : null
    const hasContainerBounds =
      iframe !== null &&
      containerRect !== null &&
      containerRect.width > 0 &&
      containerRect.height > 0

    let hasAnyLayoutBox = false
    for (let i = 0; i < blocks.length; i += 1) {
      const block = blocks[i]
      if (!block) continue
      const paired = syntheticByBlockId.get(block.blockId) ?? null
      if (hasNonZeroBox(block.element) || (paired !== null && hasNonZeroBox(paired))) {
        hasAnyLayoutBox = true
        break
      }
    }

    let effectiveRange = visibleRange
    if (rangeEndHidden && !hasAnyLayoutBox) {
      // Without layout boxes (e.g. headless DOM tests), collapse a hidden-tail range
      // to its start boundary so off-screen trailing blocks are not flooded as visible.
      const collapsed = doc.createRange()
      collapsed.setStart(visibleRange.startContainer, visibleRange.startOffset)
      collapsed.collapse(true)
      effectiveRange = collapsed
    }

    const mode = getDocumentTranslationMode(doc)
    const isVisualCandidateVisible = (el: Element): boolean => {
      if (isNodeEffectivelyHidden(doc, el)) return false
      if (hasAnyLayoutBox) {
        if (!hasNonZeroBox(el)) return false
        if (hasContainerBounds && iframe && containerRect) {
          return isElementInHostViewport(el, iframe, containerRect)
        }
        if (rangeEndHidden) {
          const rect = el.getBoundingClientRect()
          const viewW = doc.defaultView?.innerWidth ?? 0
          const viewH = doc.defaultView?.innerHeight ?? 0
          if (viewW > 0 && viewH > 0) {
            return (
              rect.right > 0 &&
              rect.left < viewW &&
              rect.bottom > 0 &&
              rect.top < viewH
            )
          }
        }
      }
      return safeIntersectsNode(effectiveRange, el)
    }

    for (let i = 0; i < blocks.length; i += 1) {
      const block = blocks[i]
      if (!block) continue

      const pairedSynthetic = syntheticByBlockId.get(block.blockId) ?? null

      const hostVisible =
        !isHostBlockVisuallyHidden(doc, block.element) &&
        isVisualCandidateVisible(block.element)
      const syntheticVisible =
        mode !== "original" &&
        pairedSynthetic !== null &&
        isVisualCandidateVisible(pairedSynthetic)

      if (hostVisible || syntheticVisible) {
        visibleBlocks.push(block)
        lastVisibleIdx = i
      }
    }

    let lookaheadStart: number
    if (lastVisibleIdx >= 0) {
      lookaheadStart = lastVisibleIdx + 1
    } else {
      const lookaheadRefRange = (() => {
        if (!rangeEndHidden) return effectiveRange
        const collapsed = doc.createRange()
        collapsed.setStart(
          visibleRange.startContainer,
          visibleRange.startOffset
        )
        collapsed.collapse(true)
        return collapsed
      })()
      const firstAfterIdx = blocks.findIndex((block) => {
        try {
          const nodeRange = doc.createRange()
          nodeRange.selectNode(block.element)
          return (
            lookaheadRefRange.compareBoundaryPoints(
              Range.START_TO_END,
              nodeRange
            ) <= 0
          )
        } catch {
          return false
        }
      })
      lookaheadStart = firstAfterIdx >= 0 ? firstAfterIdx : blocks.length
    }
    const lookaheadBlocks = blocks.slice(
      lookaheadStart,
      lookaheadStart + this.lookaheadCount
    )

    const signature = `${sectionIndex}|${visibleBlocks
      .map((b) => b.blockId)
      .join(",")}|${lookaheadBlocks.map((b) => b.blockId).join(",")}`

    if (
      this.activeWindowSectionIndex === sectionIndex &&
      this.activeWindowSignature === signature &&
      this.activeAbortController !== null &&
      !this.activeAbortController.signal.aborted
    ) {
      return
    }

    this.abortActiveWindow()

    const controller = new AbortController()
    this.activeAbortController = controller
    this.activeWindowSectionIndex = sectionIndex
    this.activeWindowSignature = signature

    for (const augmenter of [...this.augmenters]) {
      augmenter.onVisibleWindowChange({
        sectionIndex,
        doc,
        visibleBlocks,
        lookaheadBlocks,
        signal: controller.signal,
        requestRelayout: this.requestRelayout,
      })
    }
  }

  updateViewState(state: EphemeralReaderViewState): void {
    if (this.disposed) return
    this.viewState = state

    for (const [sectionIndex, sectionState] of this.sections.entries()) {
      setDocumentTranslationMode(sectionState.doc, state.translationMode)
      for (const augmenter of [...this.augmenters]) {
        augmenter.onViewStateChange({
          sectionIndex,
          doc: sectionState.doc,
          state,
          requestRelayout: this.requestRelayout,
        })
      }
    }
  }

  unmountSection(sectionIndex: number): void {
    if (!this.sections.delete(sectionIndex)) return

    if (this.activeWindowSectionIndex === sectionIndex) {
      this.abortActiveWindow()
    }

    for (const augmenter of [...this.augmenters]) {
      augmenter.onSectionUnmount(sectionIndex)
    }
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.abortActiveWindow()

    const mountedIndices = Array.from(this.sections.keys())
    this.sections.clear()

    for (const sectionIndex of mountedIndices) {
      for (const augmenter of [...this.augmenters]) {
        augmenter.onSectionUnmount(sectionIndex)
      }
    }
    this.augmenters.length = 0
  }

  private abortActiveWindow(): void {
    if (this.activeAbortController && !this.activeAbortController.signal.aborted) {
      this.activeAbortController.abort()
    }
    this.activeAbortController = null
    this.activeWindowSectionIndex = null
    this.activeWindowSignature = null
  }
}
