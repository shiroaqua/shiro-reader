import type { TranslatableBlockHandle } from "../../contracts/augmentation"
import type { TranslationDisplayMode } from "../../contracts/engine"

export const XHTML_NS = "http://www.w3.org/1999/xhtml"
export const SHIRO_SYNTHETIC_ATTR = "data-shiro-synthetic"
export const SHIRO_BLOCK_ID_ATTR = "data-shiro-block-id"
export const SHIRO_ANCHOR_FOR_ATTR = "data-shiro-anchor-for"
export const SHIRO_HAS_TRANS_ATTR = "data-shiro-has-translation"
export const SHIRO_TRANS_MODE_ATTR = "data-shiro-trans-mode"

const CANDIDATE_BLOCK_TAGS = new Set([
  "p",
  "li",
  "blockquote",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "dd",
  "dt",
  "figcaption",
  "div",
])

const EXCLUDED_SUBTREE_TAGS = new Set([
  "pre",
  "code",
  "kbd",
  "samp",
  "svg",
  "math",
  "script",
  "style",
  "noscript",
  "template",
  "head",
  "audio",
  "video",
  "canvas",
  "iframe",
  "object",
  "embed",
])

const NON_TRANSLATABLE_INNER_TEXT_TAGS = new Set([
  "pre",
  "svg",
  "math",
  "script",
  "style",
  "noscript",
  "template",
])

export type XhtmlAttributes = Readonly<
  Record<string, string | null | undefined>
>

export interface InjectSyntheticTranslationOptions {
  readonly ready?: boolean | undefined
}

export function escapeAttrSelectorValue(value: string): string {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')
}

export function createXhtmlElement(
  doc: Document,
  tagName: string,
  attrs?: XhtmlAttributes | undefined,
  textContent?: string | undefined
): Element {
  const el = doc.createElementNS(XHTML_NS, tagName.toLowerCase())
  if (attrs) {
    for (const [name, value] of Object.entries(attrs)) {
      if (value !== undefined && value !== null) {
        el.setAttribute(name, value)
      }
    }
  }
  if (textContent !== undefined) {
    el.textContent = textContent
  }
  return el
}

export function isElementNode(node: Node | null): node is Element {
  return node !== null && node.nodeType === 1
}

export function isSyntheticElement(node: Node | null): boolean {
  return isElementNode(node) && node.hasAttribute(SHIRO_SYNTHETIC_ATTR)
}

export function findSyntheticAncestor(node: Node | null): Element | null {
  let nearestSynthetic: Element | null = null
  let current: Node | null = node

  while (current !== null) {
    if (isElementNode(current) && isSyntheticElement(current)) {
      if (current.hasAttribute(SHIRO_ANCHOR_FOR_ATTR)) {
        return current
      }
      nearestSynthetic ??= current
    }
    current = current.parentNode
  }

  return nearestSynthetic
}

export function findHostElementByBlockId(
  doc: Document,
  blockId: string
): Element | null {
  if (blockId.length === 0) return null
  const escaped = escapeAttrSelectorValue(blockId)
  return doc.querySelector(
    `[${SHIRO_BLOCK_ID_ATTR}="${escaped}"]:not([${SHIRO_SYNTHETIC_ATTR}])`
  )
}

export function findSyntheticTranslationForBlock(
  doc: Document,
  blockId: string
): Element | null {
  if (blockId.length === 0) return null
  const escaped = escapeAttrSelectorValue(blockId)
  return doc.querySelector(
    `[${SHIRO_SYNTHETIC_ATTR}="translation"][${SHIRO_ANCHOR_FOR_ATTR}="${escaped}"]`
  )
}

export function setDocumentTranslationMode(
  doc: Document,
  mode: TranslationDisplayMode
): void {
  doc.documentElement?.setAttribute(SHIRO_TRANS_MODE_ATTR, mode)
}

export function getDocumentTranslationMode(
  doc: Document
): TranslationDisplayMode {
  const raw = doc.documentElement?.getAttribute(SHIRO_TRANS_MODE_ATTR)
  if (raw === "bilingual" || raw === "translated_only") {
    return raw
  }
  return "original"
}

export function isElementDisplayNone(doc: Document, element: Element): boolean {
  const inlineStyle = (element as { readonly style?: CSSStyleDeclaration }).style
  if (inlineStyle && inlineStyle.display === "none") {
    return true
  }

  const rawStyleAttr = element.getAttribute("style")
  if (
    rawStyleAttr &&
    /(?:^|;)\s*display\s*:\s*none\s*(?:;|$)/i.test(rawStyleAttr)
  ) {
    return true
  }

  const computed = doc.defaultView?.getComputedStyle?.(element)
  return computed?.display === "none"
}

export function isHostBlockVisuallyHidden(
  doc: Document,
  hostElement: Element
): boolean {
  if (isElementDisplayNone(doc, hostElement)) {
    return true
  }
  return (
    getDocumentTranslationMode(doc) === "translated_only" &&
    hostElement.getAttribute(SHIRO_HAS_TRANS_ATTR) === "ready"
  )
}

export function normalizeBlockText(raw: string): string {
  return raw.replace(/\s+/g, " ").trim()
}

export function computeSegmentHash(normalizedText: string): string {
  const bytes = new TextEncoder().encode(normalizedText)
  let h1 = 0x811c9dc5
  let h2 = 0x811c9dc5 ^ 0x9e3779b9

  for (const byte of bytes) {
    h1 ^= byte
    h1 = Math.imul(h1, 0x01000193)
    h2 ^= byte
    h2 = Math.imul(h2, 0x01000193) ^ 0x27d4eb2d
  }

  const hex1 = (h1 >>> 0).toString(16).padStart(8, "0")
  const hex2 = (h2 >>> 0).toString(16).padStart(8, "0")
  return `${hex1}${hex2}`
}

function getLocalTagName(element: Element): string {
  return (element.localName || element.tagName).toLowerCase()
}

function hasDescendantCandidateBlock(element: Element): boolean {
  for (const child of Array.from(element.children)) {
    if (isSyntheticElement(child)) continue
    const tag = getLocalTagName(child)
    if (EXCLUDED_SUBTREE_TAGS.has(tag)) continue
    if (CANDIDATE_BLOCK_TAGS.has(tag)) return true
    if (hasDescendantCandidateBlock(child)) return true
  }
  return false
}

function extractTranslatableText(element: Element): string {
  let buffer = ""

  const walk = (node: Node): void => {
    if (node.nodeType === 3 || node.nodeType === 4) {
      buffer += node.nodeValue ?? ""
      return
    }
    if (node.nodeType !== 1) return

    const el = node as Element
    if (isSyntheticElement(el)) return
    const tag = getLocalTagName(el)
    if (NON_TRANSLATABLE_INNER_TEXT_TAGS.has(tag)) return

    for (const child of Array.from(el.childNodes)) {
      walk(child)
    }
  }

  for (const child of Array.from(element.childNodes)) {
    walk(child)
  }

  return normalizeBlockText(buffer)
}

function findNextSequenceNumber(root: Element, sectionIndex: number): number {
  const prefix = `b-${sectionIndex}-`
  let maxSeq = -1

  const existing = root.querySelectorAll(`[${SHIRO_BLOCK_ID_ATTR}]`)
  for (const el of Array.from(existing)) {
    if (isSyntheticElement(el)) continue
    const id = el.getAttribute(SHIRO_BLOCK_ID_ATTR)
    if (!id || !id.startsWith(prefix)) continue
    const tail = id.slice(prefix.length)
    if (!/^\d+$/.test(tail)) continue
    const parsed = Number.parseInt(tail, 10)
    if (parsed > maxSeq) {
      maxSeq = parsed
    }
  }

  return maxSeq + 1
}

export function indexTranslatableBlocks(
  doc: Document,
  sectionIndex: number
): TranslatableBlockHandle[] {
  const root = doc.body ?? doc.documentElement
  if (!root) return []

  const handles: TranslatableBlockHandle[] = []
  let nextSeq = findNextSequenceNumber(root, sectionIndex)

  const visit = (element: Element): void => {
    if (isSyntheticElement(element)) return
    const tag = getLocalTagName(element)
    if (EXCLUDED_SUBTREE_TAGS.has(tag)) return

    if (CANDIDATE_BLOCK_TAGS.has(tag)) {
      if (hasDescendantCandidateBlock(element)) {
        for (const child of Array.from(element.children)) {
          visit(child)
        }
        return
      }

      const sourceText = extractTranslatableText(element)
      if (sourceText.length === 0) return

      let blockId = element.getAttribute(SHIRO_BLOCK_ID_ATTR)
      if (!blockId) {
        blockId = `b-${sectionIndex}-${nextSeq}`
        nextSeq += 1
        element.setAttribute(SHIRO_BLOCK_ID_ATTR, blockId)
      }

      handles.push({
        blockId,
        sectionIndex,
        element,
        sourceText,
        segmentHash: computeSegmentHash(sourceText),
      })
      return
    }

    for (const child of Array.from(element.children)) {
      visit(child)
    }
  }

  for (const child of Array.from(root.children)) {
    visit(child)
  }

  return handles
}

function resolveSyntheticTagSpec(
  hostElement: Element,
  parent: Node
): { readonly tagName: string; readonly style: string | undefined } {
  const hostTag = getLocalTagName(hostElement)
  const parentTag =
    parent.nodeType === 1 ? getLocalTagName(parent as Element) : ""

  if (
    hostTag === "li" ||
    parentTag === "ul" ||
    parentTag === "ol" ||
    parentTag === "menu"
  ) {
    return {
      tagName: "li",
      style: "display: block; list-style: none;",
    }
  }

  if (hostTag === "dd" || hostTag === "dt" || parentTag === "dl") {
    return {
      tagName: "dd",
      style: undefined,
    }
  }

  return {
    tagName: "div",
    style: undefined,
  }
}

export function injectSyntheticTranslation(
  doc: Document,
  hostElement: Element,
  translatedText: string,
  options?: InjectSyntheticTranslationOptions | undefined
): Element {
  const blockId = hostElement.getAttribute(SHIRO_BLOCK_ID_ATTR)
  if (!blockId) {
    throw new Error(
      `Host element is missing ${SHIRO_BLOCK_ID_ATTR}; call indexTranslatableBlocks before injecting synthetic translation.`
    )
  }

  const parent = hostElement.parentNode
  if (!parent) {
    throw new Error(
      `Host element (${blockId}) is detached from the DOM and cannot receive a sibling synthetic node.`
    )
  }

  const { tagName, style } = resolveSyntheticTagSpec(hostElement, parent)
  const readyValue = options?.ready === false ? "streaming" : "ready"
  const existing = findSyntheticTranslationForBlock(doc, blockId)

  if (existing) {
    if (getLocalTagName(existing) === tagName) {
      existing.textContent = translatedText
      hostElement.setAttribute(SHIRO_HAS_TRANS_ATTR, readyValue)
      return existing
    }
    existing.parentNode?.removeChild(existing)
  }

  const syntheticEl = createXhtmlElement(
    doc,
    tagName,
    {
      [SHIRO_SYNTHETIC_ATTR]: "translation",
      [SHIRO_ANCHOR_FOR_ATTR]: blockId,
      style,
    },
    translatedText
  )

  parent.insertBefore(syntheticEl, hostElement.nextSibling)
  hostElement.setAttribute(SHIRO_HAS_TRANS_ATTR, readyValue)
  return syntheticEl
}

export function removeSyntheticTranslations(doc: Document): void {
  const syntheticNodes = doc.querySelectorAll(`[${SHIRO_SYNTHETIC_ATTR}]`)
  for (const node of Array.from(syntheticNodes)) {
    node.parentNode?.removeChild(node)
  }

  const hostNodes = doc.querySelectorAll(`[${SHIRO_HAS_TRANS_ATTR}]`)
  for (const host of Array.from(hostNodes)) {
    host.removeAttribute(SHIRO_HAS_TRANS_ATTR)
  }
}

export function safeIntersectsNode(range: Range, node: Node): boolean {
  const owner = range.startContainer.ownerDocument
  if (!owner || node.ownerDocument !== owner || !owner.contains(node)) {
    return false
  }

  try {
    if (typeof range.intersectsNode === "function") {
      return range.intersectsNode(node)
    }
  } catch {
    // Fall through to boundary comparison fallback.
  }

  try {
    const nodeRange = owner.createRange()
    nodeRange.selectNode(node)
    return (
      range.compareBoundaryPoints(Range.END_TO_START, nodeRange) < 0 &&
      range.compareBoundaryPoints(Range.START_TO_END, nodeRange) > 0
    )
  } catch {
    return false
  }
}
