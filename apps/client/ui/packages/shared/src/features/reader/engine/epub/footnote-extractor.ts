import type { EPUB } from "foliate-js/epub.js"
import type {
  FootnoteAnchorRect,
  FootnoteKind,
  FootnotePayload,
  NavigationLocator,
} from "../../contracts/engine"
import { sanitizeMarkupDocumentWithType } from "./section-bridge"

const OPS_NS = "http://www.idpf.org/2007/ops"

const REF_TYPES = new Set(["noteref", "biblioref", "glossref"])
const REF_ROLES = new Set(["doc-noteref", "doc-biblioref", "doc-glossref"])
const INLINE_ANCESTOR_TAGS = new Set([
  "a",
  "span",
  "sup",
  "sub",
  "em",
  "strong",
  "i",
  "b",
  "small",
  "big",
  "abbr",
  "cite",
  "code",
])

function readTokenSet(raw: string | null | undefined): Set<string> {
  if (!raw) return new Set()
  return new Set(
    raw
      .split(/\s+/)
      .map((item) => item.trim().toLowerCase())
      .filter((item) => item.length > 0)
  )
}

function getEpubTypes(el: Element | null | undefined): Set<string> {
  if (!el) return new Set()
  const nsVal =
    typeof el.getAttributeNS === "function"
      ? el.getAttributeNS(OPS_NS, "type")
      : null
  const literalVal = el.getAttribute("epub:type")
  const tokens = readTokenSet(nsVal)
  for (const token of readTokenSet(literalVal)) {
    tokens.add(token)
  }
  return tokens
}

function getRoles(el: Element | null | undefined): Set<string> {
  if (!el) return new Set()
  return readTokenSet(el.getAttribute("role"))
}

function isSuperscriptAnchor(anchorEl: Element): boolean {
  const localName = anchorEl.localName?.toLowerCase() ?? ""
  if (localName === "sup") return true
  if (anchorEl.closest?.("sup") !== null) return true
  if (
    anchorEl.children.length === 1 &&
    anchorEl.children[0]?.localName?.toLowerCase() === "sup"
  ) {
    return true
  }
  const computed = anchorEl.ownerDocument?.defaultView?.getComputedStyle?.(
    anchorEl
  )
  const align = computed?.verticalAlign ?? ""
  return (
    align === "super" ||
    align === "top" ||
    align === "text-top" ||
    /^\d/.test(align)
  )
}

export function isFootnoteReference(
  anchorEl: Element | null | undefined,
  resolvedHref: string
): boolean {
  if (!anchorEl) return false
  const hashIdx = resolvedHref.indexOf("#")
  if (hashIdx < 0 || hashIdx >= resolvedHref.length - 1) {
    return false
  }

  const types = getEpubTypes(anchorEl)
  const roles = getRoles(anchorEl)

  if (types.has("backlink") || roles.has("doc-backlink")) {
    return false
  }

  for (const t of REF_TYPES) {
    if (types.has(t)) return true
  }
  for (const r of REF_ROLES) {
    if (roles.has(r)) return true
  }

  return isSuperscriptAnchor(anchorEl)
}

function resolveFootnoteKind(el: Element): FootnoteKind {
  const types = getEpubTypes(el)
  const roles = getRoles(el)
  if (roles.has("doc-biblioentry") || types.has("biblioentry")) {
    return "biblioentry"
  }
  if (roles.has("definition") || types.has("glossdef")) {
    return "definition"
  }
  if (
    roles.has("doc-endnote") ||
    types.has("endnote") ||
    types.has("rearnote")
  ) {
    return "endnote"
  }
  if (roles.has("doc-footnote") || types.has("footnote")) {
    return "footnote"
  }
  return "note"
}

function resolveFootnoteBlockContainer(
  doc: Document,
  targetEl: Element
): Element | null {
  let current: Element = targetEl
  while (
    INLINE_ANCESTOR_TAGS.has(current.localName?.toLowerCase() ?? "") &&
    current.parentElement &&
    current.parentElement !== doc.body
  ) {
    current = current.parentElement
  }
  if (current === doc.body) {
    const next = targetEl.nextElementSibling
    if (next && !INLINE_ANCESTOR_TAGS.has(next.localName?.toLowerCase() ?? "")) {
      return next
    }
    return targetEl
  }
  return current
}

function stripBacklinksAndResolveLinks(
  container: Element,
  resolveHref?: ((href: string) => string) | undefined
): void {
  const candidates = Array.from(container.querySelectorAll("*"))
  for (const el of candidates) {
    const types = getEpubTypes(el)
    const roles = getRoles(el)
    if (types.has("backlink") || roles.has("doc-backlink")) {
      el.remove()
      continue
    }
    if (el.localName?.toLowerCase() === "a") {
      const rawHref = el.getAttribute("href")
      if (rawHref && resolveHref) {
        try {
          el.setAttribute("href", resolveHref(rawHref))
        } catch {
          // Keep original href if resolver throws.
        }
      }
    }
  }
}

export function computeAnchorRectInViewport(
  anchorEl: Element,
  viewportContainer: HTMLElement
): FootnoteAnchorRect | null {
  const anchorRect = anchorEl.getBoundingClientRect?.()
  const viewportRect = viewportContainer.getBoundingClientRect?.()
  if (!anchorRect || !viewportRect) return null

  const frameEl = anchorEl.ownerDocument?.defaultView?.frameElement as
    | (HTMLElement & { readonly offsetWidth: number; readonly offsetHeight: number })
    | null
  if (frameEl && typeof frameEl.getBoundingClientRect === "function") {
    const frameRect = frameEl.getBoundingClientRect()
    const baseWidth =
      frameEl.offsetWidth > 0 ? frameEl.offsetWidth : frameRect.width
    const baseHeight =
      frameEl.offsetHeight > 0 ? frameEl.offsetHeight : frameRect.height
    const scaleX = baseWidth > 0 ? frameRect.width / baseWidth : 1
    const scaleY = baseHeight > 0 ? frameRect.height / baseHeight : 1
    return {
      left: frameRect.left + anchorRect.left * scaleX - viewportRect.left,
      top: frameRect.top + anchorRect.top * scaleY - viewportRect.top,
      width: Math.max(1, anchorRect.width * scaleX),
      height: Math.max(1, anchorRect.height * scaleY),
    }
  }

  return {
    left: anchorRect.left - viewportRect.left,
    top: anchorRect.top - viewportRect.top,
    width: Math.max(1, anchorRect.width),
    height: Math.max(1, anchorRect.height),
  }
}

function findTargetElement(
  doc: Document,
  anchorFn: ((doc: Document) => Element | number | null) | undefined,
  fragment: string
): Element | null {
  const fromResolver = anchorFn?.(doc)
  if (
    fromResolver &&
    typeof fromResolver === "object" &&
    "nodeType" in fromResolver &&
    fromResolver.nodeType === 1
  ) {
    return fromResolver
  }
  if (!fragment) return null
  const decoded = (() => {
    try {
      return decodeURIComponent(fragment)
    } catch {
      return fragment
    }
  })()
  const byId = doc.getElementById(decoded) ?? doc.getElementById(fragment)
  if (byId) return byId
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return doc.querySelector(`[name="${CSS.escape(decoded)}"]`)
  }
  return null
}

export async function extractFootnotePayload(params: {
  readonly book: Pick<EPUB, "sections" | "resolveHref">
  readonly resolvedHref: string
  readonly anchorEl: Element
  readonly viewportContainer: HTMLElement
}): Promise<FootnotePayload | null> {
  const { book, resolvedHref, anchorEl, viewportContainer } = params
  const resolved = book.resolveHref?.(resolvedHref) ?? null
  if (
    !resolved ||
    !Number.isFinite(resolved.index) ||
    resolved.index < 0 ||
    resolved.index >= book.sections.length
  ) {
    return null
  }

  const section = book.sections[resolved.index]
  if (!section || typeof section.createDocument !== "function") {
    return null
  }

  let targetDoc: Document
  try {
    targetDoc = await section.createDocument()
  } catch {
    return null
  }

  const hashIdx = resolvedHref.indexOf("#")
  const pathPart = hashIdx >= 0 ? resolvedHref.slice(0, hashIdx) : resolvedHref
  const fragment = hashIdx >= 0 ? resolvedHref.slice(hashIdx + 1) : ""

  const targetEl = findTargetElement(targetDoc, resolved.anchor, fragment)
  if (!targetEl) return null

  const containerEl = resolveFootnoteBlockContainer(targetDoc, targetEl)
  if (!containerEl) return null

  const kind = resolveFootnoteKind(containerEl)
  const cloned = containerEl.cloneNode(true) as Element
  if (
    "style" in cloned &&
    typeof (cloned as { readonly style?: CSSStyleDeclaration }).style
      ?.removeProperty === "function"
  ) {
    ;(cloned as HTMLElement).style.removeProperty("display")
    ;(cloned as HTMLElement).style.removeProperty("visibility")
  }
  stripBacklinksAndResolveLinks(cloned, section.resolveHref)

  const sanitizedHtml = sanitizeMarkupDocumentWithType(
    `<div data-shiro-footnote-content="">${cloned.outerHTML}</div>`,
    "text/html"
  ).markup
  const parsedClean = new DOMParser().parseFromString(sanitizedHtml, "text/html")
  const wrapper = parsedClean.querySelector("[data-shiro-footnote-content]")
  const html = wrapper?.innerHTML ?? ""
  const text = (wrapper?.textContent ?? cloned.textContent ?? "")
    .replace(/\s+/g, " ")
    .trim()

  if (!text && !wrapper?.querySelector("img, svg")) {
    return null
  }

  const sectionHref =
    pathPart || (typeof section.id === "string" ? section.id : resolvedHref)
  const locator: NavigationLocator = {
    kind: "epub",
    spine_index: resolved.index,
    href: sectionHref,
    target: fragment ? { kind: "fragment", fragment } : { kind: "start" },
  }

  return {
    href: resolvedHref,
    locator,
    kind,
    html,
    text,
    anchorRect: computeAnchorRectInViewport(anchorEl, viewportContainer),
  }
}
