import * as CFI from "foliate-js/epubcfi.js"
import type { SelectionTextOrigin } from "../../contracts/selection"
import {
  findHostElementByBlockId,
  findSyntheticAncestor,
  getDocumentTranslationMode,
  isElementDisplayNone,
  isElementNode,
  isHostBlockVisuallyHidden,
  isSyntheticElement,
  normalizeBlockText,
  safeIntersectsNode,
  SHIRO_ANCHOR_FOR_ATTR,
  SHIRO_BLOCK_ID_ATTR,
  SHIRO_SYNTHETIC_ATTR,
  findSyntheticTranslationForBlock,
} from "./xhtml-dom"

export const shiroCfiFilter: CFI.CfiNodeFilter = (node: Node): number => {
  if (isSyntheticElement(node)) {
    return NodeFilter.FILTER_REJECT
  }
  return NodeFilter.FILTER_ACCEPT
}

const VALID_RANGE_SIDE_PREFIX_CHARS = new Set(["/", ":", "[", "@", "~", "!"])

function hasControlChar(value: string): boolean {
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i)
    if (code <= 0x1f || code === 0x7f) {
      return true
    }
  }
  return false
}

/**
 * Mirrors Rust `parse_cfi_structure` to determine whether a CFI string is a
 * syntactically valid canonical EPUB range CFI (`epubcfi(parent,start,end)`).
 */
export function isCanonicalRangeCfi(cfi: string): boolean {
  if (cfi !== cfi.trim() || hasControlChar(cfi)) {
    return false
  }
  if (!cfi.startsWith("epubcfi(") || !cfi.endsWith(")")) {
    return false
  }
  const body = cfi.slice("epubcfi(".length, -1)
  if (!body || !body.startsWith("/")) {
    return false
  }

  let inBracket = false
  let escaped = false
  let prevNonBracket: string | null = null
  const commas: number[] = []

  for (let i = 0; i < body.length; i++) {
    const ch = body[i]!
    if (escaped) {
      escaped = false
      continue
    }
    if (ch === "^") {
      escaped = true
      continue
    }
    if (ch === "[") {
      if (inBracket) return false
      inBracket = true
      prevNonBracket = null
      continue
    }
    if (ch === "]") {
      if (!inBracket) return false
      inBracket = false
      prevNonBracket = null
      continue
    }
    if (!inBracket) {
      if (/\s/.test(ch)) return false
      if (ch === "-" && prevNonBracket === "/") return false
      if (ch === ",") {
        commas.push(i)
      }
      prevNonBracket = ch
    }
  }

  if (escaped || inBracket || commas.length !== 2) {
    return false
  }

  const c1 = commas[0]!
  const c2 = commas[1]!
  const parent = body.slice(0, c1)
  const start = body.slice(c1 + 1, c2)
  const end = body.slice(c2 + 1)

  if (!parent.startsWith("/") || (start.length === 0 && end.length === 0)) {
    return false
  }
  if (start.length > 0 && !VALID_RANGE_SIDE_PREFIX_CHARS.has(start[0]!)) {
    return false
  }
  if (end.length > 0 && !VALID_RANGE_SIDE_PREFIX_CHARS.has(end[0]!)) {
    return false
  }
  return true
}

export interface SelectionOriginInspection {
  readonly textOrigin: SelectionTextOrigin
  readonly anchorBlockOriginalText: string | null
}

interface RangeBoundaryPoint {
  readonly container: Node
  readonly offset: number
}

function isTextOrCdataNode(node: Node): node is Text {
  return node.nodeType === 3 || node.nodeType === 4
}

function findEdgeTextNode(root: Node, edge: "start" | "end"): Text | null {
  const children = Array.from(root.childNodes)
  if (edge === "end") {
    children.reverse()
  }

  for (const child of children) {
    if (isSyntheticElement(child)) continue
    if (isTextOrCdataNode(child)) {
      return child
    }
    if (child.nodeType === 1) {
      const nested = findEdgeTextNode(child, edge)
      if (nested !== null) return nested
    }
  }

  return null
}

function findFallbackNonSyntheticHost(
  syntheticEl: Element,
  edge: "start" | "end"
): Element | null {
  let prev = syntheticEl.previousElementSibling
  while (prev) {
    if (!isSyntheticElement(prev)) return prev
    prev = prev.previousElementSibling
  }

  if (edge === "end") {
    let next = syntheticEl.nextElementSibling
    while (next) {
      if (!isSyntheticElement(next)) return next
      next = next.nextElementSibling
    }
  }

  return syntheticEl.parentElement
}

function resolveHostElementForSynthetic(
  doc: Document,
  syntheticEl: Element,
  edge: "start" | "end"
): Element | null {
  const anchorFor = syntheticEl.getAttribute(SHIRO_ANCHOR_FOR_ATTR)
  if (anchorFor) {
    const host = findHostElementByBlockId(doc, anchorFor)
    if (host) return host
  }
  return findFallbackNonSyntheticHost(syntheticEl, edge)
}

function snapPointToHostEdge(
  doc: Document,
  syntheticEl: Element,
  edge: "start" | "end"
): RangeBoundaryPoint | null {
  const hostEl = resolveHostElementForSynthetic(doc, syntheticEl, edge)
  if (!hostEl) return null

  const textNode = findEdgeTextNode(hostEl, edge)
  if (textNode !== null) {
    return {
      container: textNode,
      offset: edge === "start" ? 0 : (textNode.nodeValue?.length ?? 0),
    }
  }

  return {
    container: hostEl,
    offset: edge === "start" ? 0 : hostEl.childNodes.length,
  }
}

function detectSyntheticEndpoint(
  container: Node,
  offset: number,
  edge: "start" | "end"
): Element | null {
  const ancestor = findSyntheticAncestor(container)
  if (ancestor !== null) return ancestor

  if (container.nodeType === 1) {
    const childIndex = edge === "start" ? offset : offset - 1
    if (childIndex >= 0 && childIndex < container.childNodes.length) {
      const child = container.childNodes.item(childIndex)
      if (isElementNode(child) && isSyntheticElement(child)) {
        return child
      }
    }
  }

  return null
}

function normalizeRootBoundary(
  doc: Document,
  point: RangeBoundaryPoint,
  edge: "start" | "end"
): RangeBoundaryPoint {
  if (point.container !== doc && point.container !== doc.documentElement) {
    return point
  }

  const root = doc.body ?? doc.documentElement
  if (!root) return point

  const edgeText = findEdgeTextNode(root, edge)
  if (edgeText !== null) {
    return {
      container: edgeText,
      offset: edge === "start" ? 0 : (edgeText.nodeValue?.length ?? 0),
    }
  }

  if (doc.body) {
    return {
      container: doc.body,
      offset: edge === "start" ? 0 : doc.body.childNodes.length,
    }
  }

  const firstChildEl =
    edge === "start"
      ? root.firstElementChild
      : root.lastElementChild
  if (firstChildEl && !isSyntheticElement(firstChildEl)) {
    return {
      container: firstChildEl,
      offset: edge === "start" ? 0 : firstChildEl.childNodes.length,
    }
  }

  return point
}

function normalizeElementBoundary(
  point: RangeBoundaryPoint,
  edge: "start" | "end"
): RangeBoundaryPoint {
  if (point.container.nodeType !== 1) {
    return point
  }
  const children = point.container.childNodes
  if (children.length === 0) {
    return point
  }

  if (edge === "start") {
    for (let i = Math.max(0, point.offset); i < children.length; i++) {
      const child = children.item(i)
      if (!child || isSyntheticElement(child)) continue
      if (isTextOrCdataNode(child)) {
        return { container: child, offset: 0 }
      }
      if (child.nodeType === 1) {
        const edgeText = findEdgeTextNode(child, "start")
        if (edgeText !== null) {
          return { container: edgeText, offset: 0 }
        }
        return { container: child, offset: 0 }
      }
    }
    return point
  }

  for (
    let i = Math.min(point.offset - 1, children.length - 1);
    i >= 0;
    i--
  ) {
    const child = children.item(i)
    if (!child || isSyntheticElement(child)) continue
    if (isTextOrCdataNode(child)) {
      return { container: child, offset: child.nodeValue?.length ?? 0 }
    }
    if (child.nodeType === 1) {
      const edgeText = findEdgeTextNode(child, "end")
      if (edgeText !== null) {
        return { container: edgeText, offset: edgeText.nodeValue?.length ?? 0 }
      }
      return { container: child, offset: child.childNodes.length }
    }
  }

  return point
}

function isNodeVisuallyHiddenForSelection(
  doc: Document,
  node: Node | null
): boolean {
  if (!node) return false
  const mode = getDocumentTranslationMode(doc)
  let current: Node | null = node
  while (current !== null) {
    if (isElementNode(current)) {
      if (isElementDisplayNone(doc, current)) return true
      if (isSyntheticElement(current) && mode === "original") return true
      if (!isSyntheticElement(current) && isHostBlockVisuallyHidden(doc, current)) {
        return true
      }
    }
    current = current.parentNode
  }
  return false
}

function clampTrailingZeroOffsetRange(doc: Document, range: Range): Range {
  if (range.collapsed || range.endOffset !== 0) {
    return range
  }

  const root = doc.body ?? doc.documentElement
  if (!root || !root.contains(range.endContainer)) {
    return range
  }

  const startPoint: RangeBoundaryPoint = {
    container: range.startContainer,
    offset: range.startOffset,
  }

  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  walker.currentNode = range.endContainer

  let fallbackWhitespacePoint: RangeBoundaryPoint | null = null
  let prevText = walker.previousNode() as Text | null
  while (prevText !== null) {
    if (isNodeVisuallyHiddenForSelection(doc, prevText)) {
      prevText = walker.previousNode() as Text | null
      continue
    }
    const len = prevText.nodeValue?.length ?? 0
    if (len > 0) {
      const candidate: RangeBoundaryPoint = {
        container: prevText,
        offset: len,
      }
      if (comparePoints(doc, candidate, startPoint) < 0) {
        break
      }
      if (
        normalizeBlockText(prevText.nodeValue ?? "").length > 0 ||
        prevText === range.startContainer
      ) {
        const clamped = doc.createRange()
        clamped.setStart(range.startContainer, range.startOffset)
        clamped.setEnd(candidate.container, candidate.offset)
        return clamped
      }
      if (fallbackWhitespacePoint === null) {
        fallbackWhitespacePoint = candidate
      }
    }
    prevText = walker.previousNode() as Text | null
  }

  if (fallbackWhitespacePoint !== null) {
    const clamped = doc.createRange()
    clamped.setStart(range.startContainer, range.startOffset)
    clamped.setEnd(
      fallbackWhitespacePoint.container,
      fallbackWhitespacePoint.offset
    )
    return clamped
  }

  // Fallback for non-text blocks (e.g. an image element immediately before next block).
  let cursor: Node | null = range.endContainer
  let prevSibling: Node | null = null
  while (cursor && cursor !== root) {
    let candidate = cursor.previousSibling
    while (candidate && isNodeVisuallyHiddenForSelection(doc, candidate)) {
      candidate = candidate.previousSibling
    }
    if (candidate) {
      prevSibling = candidate
      break
    }
    cursor = cursor.parentNode
  }
  if (prevSibling) {
    const candidate: RangeBoundaryPoint = {
      container: prevSibling,
      offset: prevSibling.childNodes.length,
    }
    if (comparePoints(doc, candidate, startPoint) >= 0) {
      const clamped = doc.createRange()
      clamped.setStart(range.startContainer, range.startOffset)
      clamped.setEnd(candidate.container, candidate.offset)
      return clamped
    }
  }

  return range
}

function comparePoints(
  doc: Document,
  a: RangeBoundaryPoint,
  b: RangeBoundaryPoint
): number {
  const ra = doc.createRange()
  ra.setStart(a.container, a.offset)
  ra.collapse(true)

  const rb = doc.createRange()
  rb.setStart(b.container, b.offset)
  rb.collapse(true)

  return ra.compareBoundaryPoints(Range.START_TO_START, rb)
}

function joinSectionCfi(sectionBaseCfi: string, localCfi: string): string {
  const trimmedBase = sectionBaseCfi.trim()
  if (trimmedBase.length === 0) return localCfi

  const match = CFI.isCFI.exec(trimmedBase)
  const inner = (match?.[1] ?? trimmedBase).replace(/!+$/, "")
  if (inner.length === 0) return localCfi

  return CFI.joinIndir(inner, localCfi)
}

function findEnclosingHostBlock(node: Node): Element | null {
  let current: Node | null = node
  while (current !== null) {
    if (
      isElementNode(current) &&
      !isSyntheticElement(current) &&
      current.hasAttribute(SHIRO_BLOCK_ID_ATTR)
    ) {
      return current
    }
    current = current.parentNode
  }
  return null
}

function collectIntersectingSyntheticElements(
  doc: Document,
  range: Range
): Element[] {
  const candidates = doc.querySelectorAll(`[${SHIRO_SYNTHETIC_ATTR}]`)
  const hits: Element[] = []
  for (const el of Array.from(candidates)) {
    if (safeIntersectsNode(range, el)) {
      hits.push(el)
    }
  }
  return hits
}

function hasVisibleOriginalTextBetweenSynthetics(
  doc: Document,
  range: Range
): boolean {
  const root = doc.body ?? doc.documentElement
  if (!root) return false

  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node: Node): number {
      if (findSyntheticAncestor(node) !== null) {
        return NodeFilter.FILTER_REJECT
      }
      if (normalizeBlockText(node.nodeValue ?? "").length === 0) {
        return NodeFilter.FILTER_SKIP
      }
      return NodeFilter.FILTER_ACCEPT
    },
  })

  let current = walker.nextNode()
  while (current !== null) {
    if (safeIntersectsNode(range, current)) {
      const host = findEnclosingHostBlock(current)
      if (!host || !isHostBlockVisuallyHidden(doc, host)) {
        return true
      }
    }
    current = walker.nextNode()
  }

  return false
}

export class CanonicalCfiResolver {
  rangeToCanonicalCfi(
    doc: Document,
    sectionBaseCfi: string,
    range: Range
  ): string | null {
    if (!doc.documentElement) return null
    if (
      !doc.contains(range.startContainer) ||
      !doc.contains(range.endContainer)
    ) {
      return null
    }

    const normalizedRange = clampTrailingZeroOffsetRange(doc, range)

    const startSynthetic = detectSyntheticEndpoint(
      normalizedRange.startContainer,
      normalizedRange.startOffset,
      "start"
    )
    const endSynthetic = detectSyntheticEndpoint(
      normalizedRange.endContainer,
      normalizedRange.endOffset,
      "end"
    )

    let cleanStart: RangeBoundaryPoint
    let cleanEnd: RangeBoundaryPoint

    if (
      startSynthetic !== null &&
      endSynthetic !== null &&
      startSynthetic === endSynthetic
    ) {
      const hostPoint = snapPointToHostEdge(doc, startSynthetic, "start")
      if (!hostPoint) return null
      cleanStart = hostPoint
      cleanEnd = hostPoint
    } else {
      if (startSynthetic !== null) {
        const hostPoint = snapPointToHostEdge(doc, startSynthetic, "start")
        if (!hostPoint) return null
        cleanStart = hostPoint
      } else {
        cleanStart = {
          container: normalizedRange.startContainer,
          offset: normalizedRange.startOffset,
        }
      }

      if (endSynthetic !== null) {
        const hostPoint = snapPointToHostEdge(doc, endSynthetic, "end")
        if (!hostPoint) return null
        cleanEnd = hostPoint
      } else {
        cleanEnd = {
          container: normalizedRange.endContainer,
          offset: normalizedRange.endOffset,
        }
      }
    }

    cleanStart = normalizeRootBoundary(doc, cleanStart, "start")
    cleanEnd = normalizeRootBoundary(doc, cleanEnd, "end")

    const wasCollapsed =
      cleanStart.container === cleanEnd.container &&
      cleanStart.offset === cleanEnd.offset
    cleanStart = normalizeElementBoundary(cleanStart, "start")
    cleanEnd = wasCollapsed
      ? cleanStart
      : normalizeElementBoundary(cleanEnd, "end")

    if (
      cleanStart.container === doc ||
      cleanStart.container === doc.documentElement ||
      cleanEnd.container === doc ||
      cleanEnd.container === doc.documentElement
    ) {
      return null
    }

    if (comparePoints(doc, cleanStart, cleanEnd) > 0) {
      cleanEnd = cleanStart
    }

    const cleanRange = doc.createRange()
    cleanRange.setStart(cleanStart.container, cleanStart.offset)
    cleanRange.setEnd(cleanEnd.container, cleanEnd.offset)

    try {
      const localCfi = CFI.fromRange(cleanRange, shiroCfiFilter)
      if (!localCfi || localCfi.includes("/-1")) {
        return null
      }
      return joinSectionCfi(sectionBaseCfi, localCfi)
    } catch {
      return null
    }
  }

  canonicalCfiToRange(doc: Document, cfi: string): Range | null {
    const trimmed = cfi.trim()
    if (trimmed.length === 0 || !doc.documentElement) return null
    if (!CFI.isCFI.test(trimmed)) return null

    try {
      const parsed = CFI.parse(trimmed)
      const segments = Array.isArray(parsed) ? parsed : parsed.parent
      if (segments.length > 1) {
        segments.shift()
      } else if (segments.length === 1 && segments[0]?.[0]?.index === 6) {
        return null
      }

      const startHead = CFI.collapse(parsed)[0]
      const endHead = CFI.collapse(parsed, true)[0]
      if (!startHead || startHead.length === 0 || !endHead || endHead.length === 0) {
        return null
      }

      const range = CFI.toRange(doc, parsed, shiroCfiFilter)
      if (range.startContainer === doc || range.endContainer === doc) {
        return null
      }
      return range
    } catch {
      return null
    }
  }

  resolveVisualTarget(
    doc: Document,
    target: Range | Element | number | null
  ): Range | Element | number | null {
    if (target === null || typeof target === "number") return target

    const startNode: Node | null =
      typeof target === "object" && "nodeType" in target
        ? isElementNode(target)
          ? target
          : null
        : (target.startContainer ?? null)
    if (startNode === null) return target

    const hostEl = findEnclosingHostBlock(startNode)
    if (!hostEl) return target

    if (isHostBlockVisuallyHidden(doc, hostEl)) {
      const blockId = hostEl.getAttribute(SHIRO_BLOCK_ID_ATTR)
      if (blockId) {
        const syntheticEl = findSyntheticTranslationForBlock(doc, blockId)
        if (syntheticEl && !isElementDisplayNone(doc, syntheticEl)) {
          return syntheticEl
        }
      }
    }

    return target
  }

  resolveVisualAnchor(doc: Document, cfi: string): Range | Element | null {
    const range = this.canonicalCfiToRange(doc, cfi)
    if (!range) return null
    const resolved = this.resolveVisualTarget(doc, range)
    return typeof resolved === "number" ? range : resolved
  }

  inspectSelectionOrigin(range: Range): SelectionOriginInspection {
    const doc = range.startContainer.ownerDocument
    if (!doc) {
      return { textOrigin: "original", anchorBlockOriginalText: null }
    }

    const normalizedRange = clampTrailingZeroOffsetRange(doc, range)

    const startSynthetic = detectSyntheticEndpoint(
      normalizedRange.startContainer,
      normalizedRange.startOffset,
      "start"
    )
    const endSynthetic = detectSyntheticEndpoint(
      normalizedRange.endContainer,
      normalizedRange.endOffset,
      "end"
    )

    const getHostOriginalText = (syntheticEl: Element | null): string | null => {
      if (!syntheticEl) return null
      const host = resolveHostElementForSynthetic(doc, syntheticEl, "start")
      if (!host) return null
      const normalized = normalizeBlockText(host.textContent ?? "")
      return normalized.length > 0 ? normalized : null
    }

    if (startSynthetic !== null && endSynthetic !== null) {
      if (startSynthetic === endSynthetic) {
        return {
          textOrigin: "translation",
          anchorBlockOriginalText: getHostOriginalText(startSynthetic),
        }
      }

      const hasOriginal = hasVisibleOriginalTextBetweenSynthetics(
        doc,
        normalizedRange
      )
      return {
        textOrigin: hasOriginal ? "mixed" : "translation",
        anchorBlockOriginalText: getHostOriginalText(startSynthetic),
      }
    }

    if (startSynthetic !== null || endSynthetic !== null) {
      return {
        textOrigin: "mixed",
        anchorBlockOriginalText: getHostOriginalText(
          startSynthetic ?? endSynthetic
        ),
      }
    }

    const mode = getDocumentTranslationMode(doc)
    if (mode !== "original") {
      const visibleSyntheticHit = collectIntersectingSyntheticElements(
        doc,
        normalizedRange
      ).find((el) => !isElementDisplayNone(doc, el))

      if (visibleSyntheticHit) {
        return {
          textOrigin: "mixed",
          anchorBlockOriginalText: getHostOriginalText(visibleSyntheticHit),
        }
      }
    }

    return {
      textOrigin: "original",
      anchorBlockOriginalText: null,
    }
  }
}

export const canonicalCfiResolver = new CanonicalCfiResolver()
