import * as CFI from "foliate-js/epubcfi.js"
import type { EPUB, EpubSection, EpubTocEntry } from "foliate-js/epub.js"
import type {
  FoliateExternalLinkDetail,
  FoliateLinkDetail,
  FoliateLoadDetail,
  FoliateRelocateDetail,
  FoliateRendererElement,
  View as FoliateView,
} from "foliate-js/view.js"
import { readerBridge, type ReaderBridge } from "@shiro/ui-shared/api/reader-bridge"
import { isMobilePlatform } from "@shiro/ui-shared/hooks/use-mobile"
import type { SectionAugmenter } from "../../contracts/augmentation"
import type {
  BookLocator,
  BookNavigationData,
  EngineCapabilities,
  EphemeralReaderViewState,
  OpenDocumentParams,
  ReaderBookMetadata,
  ReaderDocumentInstance,
  ReaderEngineAdapter,
  ReaderLinearUnit,
  ReaderPageStats,
  ReaderTocNode,
  RelocationSnapshot,
} from "../../contracts/engine"
import { TOC_FLAG_EXTERNAL_LINK } from "../../contracts/engine"
import type {
  CompiledFontFace,
  CompiledReaderStyle,
  ReaderFlowMode,
} from "../../contracts/settings"
import { loadSharedCustomFontBuffer } from "../../settings/reader-settings-store"
import { escapeCssString } from "../../settings/style-compiler"
import { AugmentationPipeline } from "./augmentation-pipeline"
import {
  CanonicalCfiResolver,
  canonicalCfiResolver,
} from "./canonical-cfi"
import {
  ContinuousScrollRenderer,
  type ContinuousScrollRendererOptions,
} from "./continuous-scroll-renderer"
import { EpubIpcLoader } from "./epub-ipc-loader"
import { RelayoutCoordinator } from "./relayout-coordinator"
import {
  SectionLifecycleBridge,
  attachBookSanitizer,
} from "./section-bridge"
import { selectionActionRegistry } from "../../selection/action-registry"
import { SelectionObserver } from "../../selection/selection-observer"

export interface FoliateModules {
  readonly View: typeof FoliateView
  readonly EPUB: typeof EPUB
}

export interface FoliateEpubEngineDeps {
  readonly bridge?: ReaderBridge | undefined
  readonly resolver?: CanonicalCfiResolver | undefined
  readonly loadModules?: (() => Promise<FoliateModules>) | undefined
  readonly createView?: ((ViewCtor: typeof FoliateView) => FoliateView) | undefined
  readonly createContinuousRenderer?:
    | ((options: ContinuousScrollRendererOptions) => ContinuousScrollRenderer)
    | undefined
  readonly augmenters?: readonly SectionAugmenter[] | undefined
  readonly relayoutThrottleMs?: number | undefined
  readonly now?: (() => number) | undefined
}

interface RelocateRendererDetail {
  readonly index?: number | undefined
  readonly fraction?: number | undefined
}

async function loadDefaultFoliateModules(): Promise<FoliateModules> {
  const [viewMod, epubMod] = await Promise.all([
    import("foliate-js/view.js"),
    import("foliate-js/epub.js"),
  ])
  return {
    View: viewMod.View,
    EPUB: epubMod.EPUB,
  }
}

function defaultCreateView(ViewCtor: typeof FoliateView): FoliateView {
  if (
    typeof customElements !== "undefined" &&
    !customElements.get("foliate-view")
  ) {
    customElements.define("foliate-view", ViewCtor)
  }
  return document.createElement("foliate-view") as FoliateView
}

function createAbortError(): Error {
  if (typeof DOMException !== "undefined") {
    return new DOMException("The operation was aborted.", "AbortError")
  }
  const err = new Error("The operation was aborted.")
  err.name = "AbortError"
  return err
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.min(1, Math.max(0, value))
}

function resolveLanguageMapValue(
  raw: string | Record<string, string> | undefined
): string | null {
  if (typeof raw === "string") {
    const trimmed = raw.trim()
    return trimmed.length > 0 ? trimmed : null
  }
  if (raw && typeof raw === "object") {
    for (const value of Object.values(raw)) {
      if (typeof value === "string" && value.trim().length > 0) {
        return value.trim()
      }
    }
  }
  return null
}

function extractSingleAuthorName(
  entry:
    | string
    | Record<string, string>
    | { name?: string | Record<string, string> | undefined }
    | undefined
): string | null {
  if (typeof entry === "string") {
    return entry.trim() || null
  }
  if (!entry || typeof entry !== "object") {
    return null
  }
  if ("name" in entry && entry.name !== undefined) {
    return resolveLanguageMapValue(entry.name)
  }
  return resolveLanguageMapValue(entry as Record<string, string>)
}

function extractBookMetadata(
  book: Pick<EPUB, "metadata" | "dir" | "rendition">,
  fallbackTitle: string
): ReaderBookMetadata {
  const rawMeta = book.metadata
  const title = resolveLanguageMapValue(rawMeta?.title) ?? fallbackTitle

  const rawAuthor = rawMeta?.author
  const author: string | null = Array.isArray(rawAuthor)
    ? (() => {
        const names = rawAuthor
          .map((entry) => extractSingleAuthorName(entry))
          .filter((name): name is string => name !== null)
        return names.length > 0 ? names.join(", ") : null
      })()
    : extractSingleAuthorName(rawAuthor)

  let language: string | null = null
  const rawLang = rawMeta?.language
  if (typeof rawLang === "string") {
    language = rawLang.trim() || null
  } else if (Array.isArray(rawLang)) {
    const first = rawLang.find(
      (item) => typeof item === "string" && item.trim().length > 0
    )
    language = first?.trim() ?? null
  }

  const direction = book.dir === "rtl" ? "rtl" : "ltr"
  const isFixedLayout = book.rendition?.layout === "pre-paginated"

  return {
    title,
    author,
    language,
    direction,
    isFixedLayout,
  }
}

function buildCapabilities(isFixedLayout: boolean): EngineCapabilities {
  return {
    supportsReflowTypography: !isFixedLayout,
    supportsPublisherStyleOverride: !isFixedLayout,
    supportsFlowSwitch: !isFixedLayout,
    supportsColumnSwitch: !isFixedLayout,
    supportsTextSelection: true,
    supportsToc: true,
    supportsFixedZoom: isFixedLayout,
    supportsFullTextTranslation: !isFixedLayout,
    supportsBilingualInterleave: !isFixedLayout,
  }
}

/**
 * Exact 1:1 mirror of Rust `BookNavigation::active_toc_index` linear scan.
 * Handles non-monotonic TOC items, external links (`unit_index === null`),
 * and non-linear spine sections without assuming monotonicity.
 */
export function computeActiveTocIndex(
  navigation: BookNavigationData,
  unitIndex: number,
  unitOffset: number
): number | null {
  const currentUnit = navigation.linear_units[unitIndex]
  if (!currentUnit) return null

  const clampedOffset = clamp01(unitOffset)
  let bestIndex: number | null = null
  let bestUnit = -1
  let bestOffset = -1

  for (let index = 0; index < navigation.toc.length; index++) {
    const node = navigation.toc[index]
    if (!node || node.unit_index === null) continue
    const unit = navigation.linear_units[node.unit_index]
    if (!unit) continue

    if (currentUnit.is_linear) {
      if (!unit.is_linear) continue
    } else if (node.unit_index !== unitIndex) {
      continue
    }

    const nodeUnit = node.unit_index
    const nodeOffset = node.unit_offset
    const leTarget =
      nodeUnit < unitIndex ||
      (nodeUnit === unitIndex && nodeOffset <= clampedOffset)
    if (!leTarget) continue

    const geBest =
      bestIndex === null ||
      nodeUnit > bestUnit ||
      (nodeUnit === bestUnit && nodeOffset >= bestOffset)
    if (geBest) {
      bestIndex = index
      bestUnit = nodeUnit
      bestOffset = nodeOffset
    }
  }

  return bestIndex
}

export function buildLinearUnitsFromSections(
  sections: EPUB["sections"]
): {
  readonly linearUnits: readonly ReaderLinearUnit[]
  readonly totalWeight: number
} {
  const linearUnits: ReaderLinearUnit[] = []
  let cumulativeWeight = 0

  for (let index = 0; index < sections.length; index++) {
    const section = sections[index]
    const isLinear = section ? section.linear !== "no" : false
    const rawSize =
      section && typeof section.size === "number" && Number.isFinite(section.size)
        ? section.size
        : 0
    const weight = isLinear ? Math.max(1, Math.round(rawSize)) : 0
    linearUnits.push({
      index,
      is_linear: isLinear,
      weight,
      cumulative_weight: cumulativeWeight,
      display_label: null,
    })
    cumulativeWeight += weight
  }

  return {
    linearUnits,
    totalWeight: cumulativeWeight,
  }
}

export function flattenFoliateToc(
  toc: readonly EpubTocEntry[],
  book: Pick<EPUB, "sections" | "resolveHref" | "isExternal">
): BookNavigationData {
  const { linearUnits, totalWeight } = buildLinearUnitsFromSections(book.sections)
  const nodes: Array<{
    parent: number | null
    depth: number
    subtree_end: number
    title: string
    locator: BookLocator | null
    unit_index: number | null
    unit_offset: number
    start_progression: number | null
    flags: number
    external_url?: string | null | undefined
  }> = []

  const visit = (
    entries: readonly EpubTocEntry[],
    parentIndex: number | null,
    depth: number
  ) => {
    for (const entry of entries) {
      const myIndex = nodes.length
      const title = entry.label?.trim() || "(Untitled)"
      const rawHref = entry.href?.trim() ?? ""
      const isExternal =
        rawHref.length > 0 &&
        (book.isExternal?.(rawHref) ??
          /^(?!blob:)[a-z][a-z0-9+.-]*:/i.test(rawHref))

      let locator: BookLocator | null = null
      let unitIndex: number | null = null
      let startProgression: number | null = null
      let flags = 0
      let externalUrl: string | undefined

      if (isExternal) {
        flags = TOC_FLAG_EXTERNAL_LINK
        externalUrl = rawHref
      } else if (rawHref.length > 0) {
        const hashIdx = rawHref.indexOf("#")
        const pathPart = hashIdx >= 0 ? rawHref.slice(0, hashIdx) : rawHref
        const fragment =
          hashIdx >= 0 ? rawHref.slice(hashIdx + 1) || null : null
        const resolved = book.resolveHref?.(rawHref) ?? null
        unitIndex =
          resolved &&
          Number.isFinite(resolved.index) &&
          resolved.index >= 0 &&
          resolved.index < book.sections.length
            ? resolved.index
            : null

        const sectionId =
          unitIndex !== null ? book.sections[unitIndex]?.id : undefined
        const normalizedHref =
          pathPart || (typeof sectionId === "string" ? sectionId : rawHref)

        locator = {
          kind: "epub",
          spine_index: unitIndex,
          href: normalizedHref,
          fragment,
          cfi: null,
        }

        const unit =
          unitIndex !== null ? linearUnits[unitIndex] : undefined
        if (unit && unit.is_linear && totalWeight > 0) {
          startProgression = Math.max(0, unit.cumulative_weight / totalWeight)
        }
      }

      nodes.push({
        parent: parentIndex,
        depth,
        subtree_end: myIndex + 1,
        title,
        locator,
        unit_index: unitIndex,
        unit_offset: 0,
        start_progression: startProgression,
        flags,
        ...(externalUrl ? { external_url: externalUrl } : {}),
      })

      if (entry.subitems && entry.subitems.length > 0) {
        visit(entry.subitems, myIndex, depth + 1)
      }
      nodes[myIndex]!.subtree_end = nodes.length
    }
  }

  visit(toc, null, 0)

  const readonlyNodes: ReaderTocNode[] = nodes.map((node) => ({
    parent: node.parent,
    depth: node.depth,
    subtree_end: node.subtree_end,
    title: node.title,
    locator: node.locator,
    unit_index: node.unit_index,
    unit_offset: node.unit_offset,
    start_progression: node.start_progression,
    flags: node.flags,
    ...(node.external_url ? { external_url: node.external_url } : {}),
  }))

  return {
    toc: readonlyNodes,
    linear_units: linearUnits,
    total_weight: totalWeight,
  }
}

export function computePageStats(
  renderer: FoliateRendererElement | null | undefined,
  flow: ReaderFlowMode,
  isFixedLayout: boolean
): ReaderPageStats | null {
  if (isFixedLayout || flow === "scrolled" || !renderer) {
    return null
  }
  const pages = renderer.pages
  if (typeof pages !== "number" || !Number.isFinite(pages) || pages <= 0) {
    return null
  }
  const total = Math.max(1, Math.round(pages) - 2)
  const rawPage =
    typeof renderer.page === "number" && Number.isFinite(renderer.page)
      ? Math.round(renderer.page)
      : 1
  const current = Math.min(total, Math.max(1, rawPage))
  return { current, total }
}

function setAttrIfChanged(el: HTMLElement, name: string, value: string): void {
  if (el.getAttribute(name) !== value) {
    el.setAttribute(name, value)
  }
}

function applyPaginatorAttributes(
  renderer: FoliateRendererElement,
  style: CompiledReaderStyle,
  isFixedLayout: boolean
): void {
  if (isFixedLayout) return
  const attrs = style.paginatorAttributes
  setAttrIfChanged(renderer, "flow", attrs.flow)
  setAttrIfChanged(renderer, "margin", attrs.margin)
  setAttrIfChanged(renderer, "gap", attrs.gap)
  setAttrIfChanged(renderer, "max-inline-size", attrs.maxInlineSize)
  setAttrIfChanged(renderer, "max-column-count", attrs.maxColumnCount)

  if (attrs.animated) {
    if (!renderer.hasAttribute("animated")) {
      renderer.setAttribute("animated", "")
    }
  } else if (renderer.hasAttribute("animated")) {
    renderer.removeAttribute("animated")
  }
}

function findDocFromRenderer(
  renderer: Pick<FoliateRendererElement, "getContents"> | null | undefined,
  sectionIndex: number
): Document | null {
  const contents = renderer?.getContents?.()
  if (!contents || contents.length === 0) return null
  const exact = contents.find((item) => item.index === sectionIndex)
  if (exact) return exact.doc
  return contents.length === 1 ? (contents[0]?.doc ?? null) : null
}

function safeResolveCfiIndex(
  view: FoliateView,
  cfi: string,
  sectionCount: number
): number | null {
  try {
    const resolved = view.resolveCFI(cfi)
    if (
      resolved &&
      typeof resolved.index === "number" &&
      Number.isFinite(resolved.index) &&
      resolved.index >= 0 &&
      resolved.index < sectionCount
    ) {
      return resolved.index
    }
  } catch {
    // Fall back to direct CFI parsing if view.resolveCFI fails on unexpected input.
  }

  try {
    const parsed = CFI.parse(cfi)
    const first = Array.isArray(parsed) ? parsed[0] : parsed.parent[0]
    const index = CFI.fake.toIndex(first)
    return Number.isFinite(index) && index >= 0 && index < sectionCount
      ? index
      : null
  } catch {
    return null
  }
}

function resolveSectionBaseCfi(
  book: EPUB,
  sectionIndex: number
): string {
  const sectionCfi = book.sections[sectionIndex]?.cfi
  if (typeof sectionCfi === "string" && sectionCfi.length > 0) {
    return sectionCfi
  }
  return CFI.fake.fromIndex(sectionIndex)
}

function findAdjacentLinearIndex(
  sections: readonly EpubSection[],
  fromIndex: number,
  direction: -1 | 1
): number | null {
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

function findFirstLinearIndex(sections: readonly EpubSection[]): number {
  const first = sections.findIndex((section) => section.linear !== "no")
  return first >= 0 ? first : 0
}

const PRESERVED_BACKGROUND_SELECTOR =
  "img, picture, video, canvas, svg, svg *, hr, [data-shiro-synthetic], [data-shiro-synthetic] *"

function clearElementInlineBackground(el: Element | null | undefined): void {
  if (!el) return
  const styled = el as Element & { style?: CSSStyleDeclaration }
  const style = styled.style
  if (!style || typeof style.removeProperty !== "function") return
  style.removeProperty("background")
  style.removeProperty("background-color")
  style.removeProperty("background-image")
}

function clearPublisherInlineBackgrounds(
  doc: Document,
  options?: { readonly preserveBodyBackground?: boolean }
): void {
  clearElementInlineBackground(doc.documentElement)
  if (!options?.preserveBodyBackground) {
    clearElementInlineBackground(doc.body)
  }
  const styledDescendants = doc.body?.querySelectorAll("[style]")
  if (!styledDescendants) return
  for (const el of Array.from(styledDescendants)) {
    if (
      typeof el.matches === "function" &&
      el.matches(PRESERVED_BACKGROUND_SELECTOR)
    ) {
      continue
    }
    clearElementInlineBackground(el)
  }
}

function hydrateBackendExternalTocUrls(
  backendToc: readonly ReaderTocNode[],
  foliateToc: readonly EpubTocEntry[],
  book: EPUB
): readonly ReaderTocNode[] {
  const needsHydration = backendToc.some(
    (node) =>
      (node.flags & TOC_FLAG_EXTERNAL_LINK) !== 0 && !node.external_url
  )
  if (!needsHydration || foliateToc.length === 0) {
    return backendToc
  }

  const externalEntries: Array<{ readonly label: string; readonly href: string }> = []
  const collect = (entries: readonly EpubTocEntry[]) => {
    for (const entry of entries) {
      const rawHref = entry.href?.trim() ?? ""
      const isExternal =
        rawHref.length > 0 &&
        (book.isExternal?.(rawHref) ??
          /^(?!blob:)[a-z][a-z0-9+.-]*:/i.test(rawHref))
      if (isExternal) {
        externalEntries.push({
          label: (entry.label ?? "").trim().toLowerCase(),
          href: rawHref,
        })
      }
      if (entry.subitems && entry.subitems.length > 0) {
        collect(entry.subitems)
      }
    }
  }
  collect(foliateToc)
  if (externalEntries.length === 0) {
    return backendToc
  }

  let cursor = 0
  return backendToc.map((node) => {
    if ((node.flags & TOC_FLAG_EXTERNAL_LINK) === 0 || node.external_url) {
      return node
    }
    const normalizedTitle = node.title.trim().toLowerCase()
    let matchedIdx = -1
    for (let i = cursor; i < externalEntries.length; i++) {
      if (externalEntries[i]!.label === normalizedTitle) {
        matchedIdx = i
        break
      }
    }
    if (matchedIdx < 0 && cursor < externalEntries.length) {
      matchedIdx = cursor
    }
    if (matchedIdx < 0) return node
    const matched = externalEntries[matchedIdx]!
    cursor = matchedIdx + 1
    return { ...node, external_url: matched.href }
  })
}

export class FoliateEpubEngine implements ReaderEngineAdapter {
  readonly format = "EPUB" as const
  private readonly bridge: ReaderBridge
  private readonly resolver: CanonicalCfiResolver
  private readonly loadModules: () => Promise<FoliateModules>
  private readonly createView: (ViewCtor: typeof FoliateView) => FoliateView
  private readonly createContinuousRenderer: (
    options: ContinuousScrollRendererOptions
  ) => ContinuousScrollRenderer
  private readonly augmenters: readonly SectionAugmenter[]
  private readonly relayoutThrottleMs: number | undefined
  private readonly now: (() => number) | undefined

  constructor(deps: FoliateEpubEngineDeps = {}) {
    this.bridge = deps.bridge ?? readerBridge
    this.resolver = deps.resolver ?? canonicalCfiResolver
    this.loadModules = deps.loadModules ?? loadDefaultFoliateModules
    this.createView = deps.createView ?? defaultCreateView
    this.createContinuousRenderer =
      deps.createContinuousRenderer ??
      ((options) => new ContinuousScrollRenderer(options))
    this.augmenters = deps.augmenters ?? []
    this.relayoutThrottleMs = deps.relayoutThrottleMs
    this.now = deps.now
  }

  async open(params: OpenDocumentParams): Promise<ReaderDocumentInstance> {
    const manageSessionLifecycleInFrontend =
      params.manageSessionLifecycleInFrontend ?? true
    const abortSignal = params.signal

    const checkNotAborted = () => {
      if (abortSignal?.aborted) {
        throw createAbortError()
      }
    }

    let loader: EpubIpcLoader | null = null
    let handedOffSessionId: string | null = null
    let destroyed = false
    let book: EPUB | null = null
    let view: FoliateView | null = null
    let sectionBridge: SectionLifecycleBridge | null = null
    let selectionObserver: SelectionObserver | null = null
    let augmentationPipeline: AugmentationPipeline | null = null
    let relayoutCoordinator: RelayoutCoordinator | null = null
    let continuousRenderer: ContinuousScrollRenderer | null = null
    const sectionSelectionControllers = new Map<number, AbortController>()
    const abortSectionSelectionListener = (sectionIndex: number): void => {
      const existing = sectionSelectionControllers.get(sectionIndex)
      if (existing) {
        existing.abort()
        sectionSelectionControllers.delete(sectionIndex)
      }
    }
    const abortAllSectionSelectionListeners = (): void => {
      for (const controller of sectionSelectionControllers.values()) {
        controller.abort()
      }
      sectionSelectionControllers.clear()
    }
    const lifecycleController = new AbortController()
    const onExternalAbort = () => {
      lifecycleController.abort()
    }
    abortSignal?.addEventListener("abort", onExternalAbort, { once: true })

    const shouldForceCloseUntracked = (targetLoader: EpubIpcLoader): boolean =>
      !manageSessionLifecycleInFrontend &&
      targetLoader.sessionId !== handedOffSessionId &&
      targetLoader.sessionId !== params.reattachSessionId

    const cleanupPartialOpen = async (): Promise<void> => {
      abortSignal?.removeEventListener("abort", onExternalAbort)
      lifecycleController.abort()
      abortAllSectionSelectionListeners()
      selectionObserver?.dispose()
      sectionBridge?.dispose()
      augmentationPipeline?.dispose()
      relayoutCoordinator?.dispose()
      try {
        continuousRenderer?.destroy()
      } catch {
        // Ignore continuous scroll renderer destroy error during cleanup.
      }
      continuousRenderer = null
      try {
        view?.close()
      } catch {
        // Ignore view close error during cancellation cleanup.
      }
      view?.remove()
      try {
        book?.destroy?.()
      } catch {
        // Ignore book destroy error during cancellation cleanup.
      }
      if (loader) {
        const forceCloseUntracked = shouldForceCloseUntracked(loader)
        await loader
          .close(
            forceCloseUntracked ? false : !manageSessionLifecycleInFrontend,
            { forceCloseUntracked }
          )
          .catch(() => {})
      }
    }

    let loaderPromise: Promise<EpubIpcLoader> | null = null

    try {
      checkNotAborted()

      loaderPromise = EpubIpcLoader.open({
        bookId: params.bookId,
        reattachSessionId: params.reattachSessionId,
        manageSessionLifecycleInFrontend,
        bridge: this.bridge,
        signal: lifecycleController.signal,
        onSessionChange: (nextSessionId) => {
          if (abortSignal?.aborted || destroyed) return
          handedOffSessionId = nextSessionId
          params.events.onSessionChange?.(nextSessionId)
        },
      })

      const navigationPromise =
        params.preloadedNavigation !== null
          ? Promise.resolve(params.preloadedNavigation)
          : this.bridge.bookNavigation(params.bookId).catch(() => null)

      const [modules, openedLoader, backendNav] = await Promise.all([
        this.loadModules(),
        loaderPromise,
        navigationPromise,
      ])
      loader = openedLoader
      if (abortSignal?.aborted) {
        throw createAbortError()
      }

      if (params.events.onSessionChange) {
        handedOffSessionId = openedLoader.sessionId
        params.events.onSessionChange(openedLoader.sessionId)
      }

      const epubBook = new modules.EPUB(openedLoader)
      book = await epubBook.init()
      checkNotAborted()

      attachBookSanitizer(book, lifecycleController.signal)

      const metadata = extractBookMetadata(book, params.fallbackTitle)
      const isFixedLayout = metadata.isFixedLayout
      const capabilities = buildCapabilities(isFixedLayout)

      const navigation: BookNavigationData = (() => {
        if (backendNav && backendNav.toc.length > 0) {
          const hydratedToc = hydrateBackendExternalTocUrls(
            backendNav.toc,
            book.toc ?? [],
            book
          )
          if (backendNav.linear_units.length > 0) {
            return hydratedToc === backendNav.toc
              ? backendNav
              : { ...backendNav, toc: hydratedToc }
          }
          const { linearUnits, totalWeight } = buildLinearUnitsFromSections(
            book.sections
          )
          return {
            toc: hydratedToc,
            linear_units: linearUnits,
            total_weight: totalWeight,
          }
        }
        return flattenFoliateToc(book.toc ?? [], book)
      })()

      const mountedDocs = new Map<number, Document>()
      const docInjectedFonts = new WeakMap<
        Document,
        Map<string, { readonly fontFace: FontFace; readonly rawFamily: string }>
      >()
      const docPendingFonts = new WeakMap<Document, Set<string>>()
      let currentStyle: CompiledReaderStyle = params.initialStyle
      let lastAppliedChapterCss: string | null = null
      let lastRelocateSnapshot: {
        readonly sectionIndex: number
        readonly cfi: string | null
        readonly range: Range | undefined
      } | null = null
      let lastEmittedRelocation: RelocationSnapshot | null = null
      let lastRendererFraction: {
        readonly index: number
        readonly fraction: number
      } | null = null
      let currentSectionIndex = 0
      let boundarySwitching = false
      let modeSwitchEpoch = 0

      const createdView = this.createView(modules.View)
      view = createdView
      createdView.style.display = "block"
      createdView.style.width = "100%"
      createdView.style.height = "100%"
      params.container.appendChild(createdView)

      await createdView.open(book)
      checkNotAborted()

      const activeView = createdView
      const activeBook = book
      const activeLoader = openedLoader

      const ensureDocCustomFont = async (
        doc: Document,
        face: CompiledFontFace
      ): Promise<void> => {
        const docView = doc.defaultView as
          | (Window & { readonly FontFace?: typeof FontFace | undefined })
          | null
        const FontFaceCtor =
          docView?.FontFace ??
          (typeof FontFace !== "undefined" ? FontFace : undefined)
        if (!FontFaceCtor || !doc.fonts?.add) return

        const isDocStillMounted = () => {
          for (const mounted of mountedDocs.values()) {
            if (mounted === doc) return true
          }
          return false
        }

        let applied = docInjectedFonts.get(doc)
        if (!applied) {
          applied = new Map()
          docInjectedFonts.set(doc, applied)
        }
        if (applied.has(face.fontId)) return

        let pending = docPendingFonts.get(doc)
        if (!pending) {
          pending = new Set()
          docPendingFonts.set(doc, pending)
        }
        if (pending.has(face.fontId)) return
        pending.add(face.fontId)

        try {
          const buffer = await loadSharedCustomFontBuffer(
            face.fontId,
            face.url,
            this.bridge
          )
          if (
            destroyed ||
            lifecycleController.signal.aborted ||
            !isDocStillMounted() ||
            !currentStyle.usesFontFaceFallback ||
            currentStyle.customFontFace?.fontId !== face.fontId
          ) {
            return
          }
          const cssFamily = `"${escapeCssString(face.family)}"`
          const fontFace = new FontFaceCtor(cssFamily, buffer, {
            weight: "100 900",
            style: "normal",
          })
          await fontFace.load()
          if (
            destroyed ||
            lifecycleController.signal.aborted ||
            !isDocStillMounted() ||
            !currentStyle.usesFontFaceFallback ||
            currentStyle.customFontFace?.fontId !== face.fontId
          ) {
            return
          }
          // Replace any previously injected FontFace that shares the same raw family name.
          for (const [prevId, prevEntry] of [...applied.entries()]) {
            if (prevId !== face.fontId && prevEntry.rawFamily === face.family) {
              doc.fonts.delete?.(prevEntry.fontFace)
              applied.delete(prevId)
            }
          }
          doc.fonts.add(fontFace)
          applied.set(face.fontId, { fontFace, rawFamily: face.family })
          relayout.scheduleRelayout()
        } catch {
          // Non-fatal: keep reading with fallback system/publisher font.
        } finally {
          pending.delete(face.fontId)
        }
      }

      const createCollapsedStartRange = (
        doc: Document,
        range: Range
      ): Range => {
        try {
          const collapsed = doc.createRange()
          collapsed.setStart(range.startContainer, range.startOffset)
          collapsed.collapse(true)
          return collapsed
        } catch {
          return range
        }
      }

      const relayout = new RelayoutCoordinator({
        getRenderer: () => continuousRenderer ?? activeView.renderer,
        resolver: this.resolver,
        throttleMs: this.relayoutThrottleMs,
        hasActiveSelection: () => {
          if (selectionObserver?.getCurrentSelection() !== null) {
            return true
          }
          for (const doc of mountedDocs.values()) {
            const sel = doc.defaultView?.getSelection?.()
            if (sel && !sel.isCollapsed && sel.rangeCount > 0) {
              return true
            }
          }
          return false
        },
        captureAnchor: () => {
          if (!lastRelocateSnapshot) return null
          const doc = mountedDocs.get(lastRelocateSnapshot.sectionIndex)
          if (doc && lastRelocateSnapshot.range) {
            const cfi = this.resolver.rangeToCanonicalCfi(
              doc,
              resolveSectionBaseCfi(activeBook, lastRelocateSnapshot.sectionIndex),
              createCollapsedStartRange(doc, lastRelocateSnapshot.range)
            )
            if (cfi) {
              return {
                sectionIndex: lastRelocateSnapshot.sectionIndex,
                anchorCfi: cfi,
              }
            }
          }
          if (lastRelocateSnapshot.cfi) {
            return {
              sectionIndex: lastRelocateSnapshot.sectionIndex,
              anchorCfi: lastRelocateSnapshot.cfi,
            }
          }
          return null
        },
      })
      relayoutCoordinator = relayout

      const pipeline = new AugmentationPipeline({
        requestRelayout: () => relayout.scheduleRelayout(),
        getViewportContainer: () => params.container,
      })
      for (const augmenter of this.augmenters) {
        pipeline.registerAugmenter(augmenter)
      }
      if (params.augmenters) {
        for (const augmenter of params.augmenters) {
          pipeline.registerAugmenter(augmenter)
        }
      }
      augmentationPipeline = pipeline
      if (params.initialViewState) {
        pipeline.updateViewState(params.initialViewState)
      }

      const observer = new SelectionObserver({
        tabId: params.tabId,
        bookId: params.bookId,
        bookHash: params.bookHash,
        format: "EPUB",
        getContainer: () => params.container,
        getSectionHref: (idx) => {
          const id = activeBook.sections[idx]?.id
          return typeof id === "string" ? id : ""
        },
        resolver: this.resolver,
        isAndroid: () => isMobilePlatform(),
        getSafeInsets: () => {
          if (typeof document === "undefined" || typeof getComputedStyle === "undefined") {
            return {}
          }
          const style = getComputedStyle(document.documentElement)
          return {
            top: Number.parseFloat(style.getPropertyValue("--shiro-safe-top")) || 0,
            bottom:
              Number.parseFloat(style.getPropertyValue("--shiro-safe-bottom")) || 0,
            left:
              Number.parseFloat(style.getPropertyValue("--shiro-safe-left")) || 0,
            right:
              Number.parseFloat(style.getPropertyValue("--shiro-safe-right")) || 0,
          }
        },
        onSelectionChange: (sel) => {
          if (sel === null) {
            relayout.notifySelectionCleared()
          }
          params.events.onSelectionChange(sel)
        },
      })
      selectionObserver = observer

      const bridge = new SectionLifecycleBridge({
        container: params.container,
        outerContainer: params.outerContainer,
        onViewportTap: (normX, normY) => params.events.onViewportTap(normX, normY),
        onKeyCommand: (event) => {
          if (
            (event.ctrlKey || event.metaKey) &&
            !event.altKey &&
            event.key.toLowerCase() === "c"
          ) {
            const activeSel = observer.flushSelectionNow()
            if (activeSel) {
              event.preventDefault()
              if (params.events.onCopySelection) {
                void params.events.onCopySelection(activeSel).catch(() => {})
              } else {
                const copyPlugin = selectionActionRegistry.getById("builtin.copy")
                if (copyPlugin) {
                  void copyPlugin
                    .execute(activeSel)
                    .then((res) => {
                      if (res.outcome === "dismiss") {
                        activeSel.clearSelection()
                      }
                    })
                    .catch(() => {})
                }
              }
            }
          }
          params.events.onKeyCommand(event)
        },
        isBoundaryScrollEnabled: () =>
          !isFixedLayout &&
          currentStyle.paginatorAttributes.flow === "scrolled" &&
          !currentStyle.seamlessScroll &&
          continuousRenderer === null,
        getScrollBoundaryState: () => {
          const renderer = activeView.renderer
          if (
            !renderer ||
            renderer.scrolled !== true ||
            typeof renderer.start !== "number" ||
            typeof renderer.viewSize !== "number" ||
            !Number.isFinite(renderer.start) ||
            !Number.isFinite(renderer.viewSize) ||
            renderer.viewSize <= 0
          ) {
            return null
          }
          const end =
            typeof renderer.end === "number" && Number.isFinite(renderer.end)
              ? renderer.end
              : renderer.start +
                (typeof renderer.size === "number" &&
                Number.isFinite(renderer.size)
                  ? renderer.size
                  : renderer.viewSize)
          return {
            atTop: renderer.start <= 2,
            atBottom: renderer.viewSize - end <= 2,
            hasPrev:
              findAdjacentLinearIndex(
                activeBook.sections,
                currentSectionIndex,
                -1
              ) !== null,
            hasNext:
              findAdjacentLinearIndex(
                activeBook.sections,
                currentSectionIndex,
                1
              ) !== null,
          }
        },
        onBoundaryChapterSwitch: (direction) => {
          if (destroyed || boundarySwitching || continuousRenderer !== null) {
            return
          }
          const renderer = activeView.renderer
          if (!renderer?.goTo) return
          const targetIndex = findAdjacentLinearIndex(
            activeBook.sections,
            currentSectionIndex,
            direction === "next" ? 1 : -1
          )
          if (targetIndex === null) return
          boundarySwitching = true
          void (async () => {
            try {
              await renderer.goTo?.({
                index: targetIndex,
                anchor: direction === "prev" ? () => 1 : () => 0,
              })
            } catch {
              // Ignore navigation cancellation during boundary switch.
            } finally {
              boundarySwitching = false
            }
          })()
        },
        now: this.now,
      })
      sectionBridge = bridge

      const attachMountedSection = (index: number, doc: Document): void => {
        if (!isFixedLayout) {
          clearPublisherInlineBackgrounds(doc, {
            preserveBodyBackground: continuousRenderer !== null,
          })
        }
        mountedDocs.set(index, doc)
        abortSectionSelectionListener(index)
        const sectionSelectionController = new AbortController()
        sectionSelectionControllers.set(index, sectionSelectionController)
        doc.addEventListener(
          "selectionchange",
          () => {
            if (destroyed || mountedDocs.get(index) !== doc) return
            const sel = doc.defaultView?.getSelection?.()
            if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
              relayout.notifySelectionCleared()
            }
          },
          { signal: sectionSelectionController.signal }
        )
        bridge.mountSection(index, doc)
        observer.attachSection(
          index,
          doc,
          resolveSectionBaseCfi(activeBook, index)
        )
        pipeline.mountSection(index, doc)

        if (currentStyle.customFontFace && currentStyle.usesFontFaceFallback) {
          void ensureDocCustomFont(doc, currentStyle.customFontFace)
        }
      }

      const detachMountedSection = (index: number): void => {
        mountedDocs.delete(index)
        abortSectionSelectionListener(index)
        observer.detachSection(index)
        bridge.unmountSection(index)
        pipeline.unmountSection(index)
      }

      const detachAllMountedSections = (): void => {
        for (const index of [...mountedDocs.keys()]) {
          detachMountedSection(index)
        }
      }

      const emitRelocationSnapshot = (opts: {
        readonly sectionIndex: number
        readonly sectionTotal: number
        readonly bookFraction: number
        readonly unitOffset: number
        readonly range: Range | null | undefined
        readonly pageStats: ReaderPageStats | null
      }): void => {
        const {
          sectionIndex,
          sectionTotal,
          bookFraction,
          unitOffset,
          range,
          pageStats,
        } = opts
        currentSectionIndex = sectionIndex
        observer.handleRelocate()

        const doc =
          mountedDocs.get(sectionIndex) ??
          findDocFromRenderer(
            continuousRenderer ?? activeView.renderer,
            sectionIndex
          )
        const sectionBaseCfi = resolveSectionBaseCfi(activeBook, sectionIndex)

        let canonicalCfi: string | null = null
        if (doc && range) {
          canonicalCfi = this.resolver.rangeToCanonicalCfi(
            doc,
            sectionBaseCfi,
            createCollapsedStartRange(doc, range)
          )
        }
        if (!canonicalCfi) {
          canonicalCfi = sectionBaseCfi
        }

        const sectionId = activeBook.sections[sectionIndex]?.id
        const locator: BookLocator = {
          kind: "epub",
          spine_index: sectionIndex,
          href: typeof sectionId === "string" ? sectionId : "",
          fragment: null,
          cfi: canonicalCfi,
        }

        lastRelocateSnapshot = {
          sectionIndex,
          cfi: canonicalCfi,
          range: range ?? undefined,
        }

        const activeTocIndex = computeActiveTocIndex(
          navigation,
          sectionIndex,
          unitOffset
        )

        if (doc) {
          pipeline.handleRelocate(sectionIndex, doc, range ?? undefined)
        }

        const emittedSnapshot: RelocationSnapshot = {
          locator,
          fraction: clamp01(bookFraction),
          sectionIndex,
          sectionTotal,
          activeTocIndex,
          pageStats,
        }
        lastEmittedRelocation = emittedSnapshot
        params.events.onRelocate(emittedSnapshot)
      }

      const navigateToLocatorWithRenderer = async (
        targetRenderer:
          | Pick<FoliateRendererElement, "goTo">
          | null
          | undefined,
        locator: BookLocator
      ): Promise<boolean> => {
        if (destroyed || locator.kind !== "epub" || !targetRenderer?.goTo) {
          return false
        }
        const sectionCount = activeBook.sections.length

        if (locator.cfi) {
          const explicitIndex =
            locator.spine_index !== null &&
            locator.spine_index >= 0 &&
            locator.spine_index < sectionCount
              ? locator.spine_index
              : null
          const sectionIndex =
            explicitIndex ??
            safeResolveCfiIndex(activeView, locator.cfi, sectionCount)
          if (sectionIndex !== null) {
            const targetCfi = locator.cfi
            await targetRenderer.goTo({
              index: sectionIndex,
              anchor: (doc: Document) =>
                this.resolver.resolveVisualAnchor(doc, targetCfi),
            })
            return true
          }
        }

        if (locator.href) {
          const targetHref = locator.fragment
            ? `${locator.href}#${locator.fragment}`
            : locator.href
          const resolved = activeBook.resolveHref?.(targetHref) ?? null
          if (
            resolved &&
            Number.isFinite(resolved.index) &&
            resolved.index >= 0 &&
            resolved.index < sectionCount
          ) {
            const resolvedAnchor = resolved.anchor
            await targetRenderer.goTo({
              index: resolved.index,
              anchor: resolvedAnchor
                ? (doc: Document) =>
                    this.resolver.resolveVisualTarget(doc, resolvedAnchor(doc))
                : undefined,
            })
            return true
          }
        }

        if (
          locator.spine_index !== null &&
          locator.spine_index >= 0 &&
          locator.spine_index < sectionCount
        ) {
          await targetRenderer.goTo({ index: locator.spine_index })
          return true
        }
        return false
      }

      const enterContinuousScrollMode = async (
        targetLocator: BookLocator | null
      ): Promise<void> => {
        const epoch = ++modeSwitchEpoch
        if (destroyed || continuousRenderer) return

        detachAllMountedSections()
        activeView.style.display = "none"

        const renderer = this.createContinuousRenderer({
          container: params.container,
          book: activeBook,
          initialStyle: currentStyle,
          onSectionLoad: (sectionIndex, doc) => {
            if (!destroyed && continuousRenderer === renderer) {
              attachMountedSection(sectionIndex, doc)
            }
          },
          onSectionUnmount: (sectionIndex) => {
            detachMountedSection(sectionIndex)
          },
          onRelocate: (detail) => {
            if (destroyed || continuousRenderer !== renderer) return
            emitRelocationSnapshot({
              sectionIndex: detail.sectionIndex,
              sectionTotal: activeBook.sections.length,
              bookFraction: detail.bookFraction,
              unitOffset: detail.sectionFraction,
              range: detail.range,
              pageStats: null,
            })
          },
          onExternalLink: (url) => {
            if (!destroyed && continuousRenderer === renderer) {
              params.events.onExternalLink(url)
            }
          },
          onInternalLink: (href) => {
            if (destroyed || continuousRenderer !== renderer) return
            params.events.onInternalLink?.(href, lastEmittedRelocation)
            const resolved = activeBook.resolveHref?.(href) ?? null
            if (
              resolved &&
              Number.isFinite(resolved.index) &&
              resolved.index >= 0 &&
              resolved.index < activeBook.sections.length
            ) {
              const resolvedAnchor = resolved.anchor
              void renderer
                .goTo({
                  index: resolved.index,
                  anchor: resolvedAnchor
                    ? (targetDoc: Document) =>
                        this.resolver.resolveVisualTarget(
                          targetDoc,
                          resolvedAnchor(targetDoc)
                        )
                    : undefined,
                })
                .catch(() => {})
            }
          },
          hasSectionSelection: (sectionIndex) => {
            const doc = mountedDocs.get(sectionIndex)
            const sel = doc?.defaultView?.getSelection?.()
            return Boolean(sel && !sel.isCollapsed && sel.rangeCount > 0)
          },
        })
        continuousRenderer = renderer

        let navigated = false
        if (targetLocator && targetLocator.kind === "epub") {
          navigated = await navigateToLocatorWithRenderer(renderer, targetLocator)
        }
        if (!navigated && !destroyed && modeSwitchEpoch === epoch && continuousRenderer === renderer) {
          await renderer.goTo({
            index: findFirstLinearIndex(activeBook.sections),
          })
        }
      }

      const reattachActiveViewContents = (
        fallbackIndex?: number | null
      ): void => {
        for (const item of activeView.renderer?.getContents?.() ?? []) {
          const idx = item.index ?? fallbackIndex
          if (
            typeof idx === "number" &&
            Number.isFinite(idx) &&
            item.doc &&
            !mountedDocs.has(idx)
          ) {
            attachMountedSection(idx, item.doc)
          }
        }
      }

      const exitContinuousScrollMode = async (
        targetLocator: BookLocator | null
      ): Promise<void> => {
        const epoch = ++modeSwitchEpoch
        if (!continuousRenderer) return

        continuousRenderer.destroy()
        continuousRenderer = null
        detachAllMountedSections()

        activeView.style.display = "block"
        lastAppliedChapterCss = null
        const renderer = activeView.renderer
        if (renderer) {
          applyPaginatorAttributes(renderer, currentStyle, isFixedLayout)
          lastAppliedChapterCss = currentStyle.chapterCss
          renderer.setStyles?.(currentStyle.chapterCss)
        }

        if (destroyed || modeSwitchEpoch !== epoch) return
        if (targetLocator && targetLocator.kind === "epub") {
          const navigated = await navigateToLocatorWithRenderer(
            activeView.renderer,
            targetLocator
          )
          if (destroyed || modeSwitchEpoch !== epoch) return
          if (navigated) {
            reattachActiveViewContents(targetLocator.spine_index)
            return
          }
        }
        await activeView.init({ showTextStart: true })
        if (destroyed || modeSwitchEpoch !== epoch) return
        reattachActiveViewContents(null)
      }

      const applyStyleInternal = (style: CompiledReaderStyle) => {
        currentStyle = style
        const wantContinuous =
          !isFixedLayout &&
          style.paginatorAttributes.flow === "scrolled" &&
          style.seamlessScroll

        if (wantContinuous && !continuousRenderer) {
          void enterContinuousScrollMode(
            lastEmittedRelocation?.locator ?? null
          ).catch(() => {})
          return
        }
        if (!wantContinuous && continuousRenderer) {
          void exitContinuousScrollMode(
            lastEmittedRelocation?.locator ?? null
          ).catch(() => {})
          return
        }
        if (continuousRenderer) {
          lastAppliedChapterCss = style.chapterCss
          continuousRenderer.applyLayout(style)
          if (style.customFontFace && style.usesFontFaceFallback) {
            for (const doc of mountedDocs.values()) {
              void ensureDocCustomFont(doc, style.customFontFace)
            }
          }
          return
        }

        const renderer = activeView.renderer
        if (renderer) {
          applyPaginatorAttributes(renderer, style, isFixedLayout)
          if (style.chapterCss !== lastAppliedChapterCss) {
            lastAppliedChapterCss = style.chapterCss
            renderer.setStyles?.(style.chapterCss)
          }
        }
        if (style.customFontFace && style.usesFontFaceFallback) {
          for (const doc of mountedDocs.values()) {
            void ensureDocCustomFont(doc, style.customFontFace)
          }
        }
      }

      if (typeof activeView.renderer?.addEventListener === "function") {
        activeView.renderer.addEventListener(
          "scroll",
          () => {
            if (!destroyed && continuousRenderer === null) {
              bridge.notifyScrollActivity()
            }
          },
          { signal: lifecycleController.signal }
        )
        activeView.renderer.addEventListener(
          "relocate",
          (event: Event) => {
            if (continuousRenderer !== null) return
            const detail = (event as CustomEvent<RelocateRendererDetail>).detail
            if (
              detail &&
              typeof detail.index === "number" &&
              typeof detail.fraction === "number" &&
              Number.isFinite(detail.fraction)
            ) {
              lastRendererFraction = {
                index: detail.index,
                fraction: clamp01(detail.fraction),
              }
            }
          },
          { signal: lifecycleController.signal }
        )
      }

      activeView.addEventListener(
        "external-link",
        (event: Event) => {
          event.preventDefault()
          const detail = (event as CustomEvent<FoliateExternalLinkDetail>).detail
          const url = detail?.href ?? detail?.href_ ?? ""
          if (url.length > 0) {
            params.events.onExternalLink(url)
          }
        },
        { signal: lifecycleController.signal }
      )

      activeView.addEventListener(
        "link",
        (event: Event) => {
          if (destroyed) return
          const detail = (event as CustomEvent<FoliateLinkDetail>).detail
          if (detail?.href) {
            params.events.onInternalLink?.(detail.href, lastEmittedRelocation)
            // In translated_only mode, host fragment targets are display:none and raw
            // foliate-js navigation silently aborts on empty rects; wrap with visual handoff.
            if (
              pipeline.getViewState().translationMode === "translated_only" &&
              activeView.renderer?.goTo
            ) {
              const resolved = activeBook.resolveHref?.(detail.href) ?? null
              if (
                resolved &&
                Number.isFinite(resolved.index) &&
                resolved.index >= 0 &&
                resolved.index < activeBook.sections.length
              ) {
                event.preventDefault()
                const resolvedAnchor = resolved.anchor
                void activeView.renderer
                  .goTo({
                    index: resolved.index,
                    anchor: resolvedAnchor
                      ? (targetDoc: Document) =>
                          this.resolver.resolveVisualTarget(
                            targetDoc,
                            resolvedAnchor(targetDoc)
                          )
                      : undefined,
                  })
                  .catch(() => {})
              }
            }
          }
        },
        { signal: lifecycleController.signal }
      )

      activeView.addEventListener(
        "load",
        (event: Event) => {
          if (destroyed || continuousRenderer !== null) return
          const detail = (event as CustomEvent<FoliateLoadDetail>).detail
          if (!detail || !detail.doc) return

          const { doc, index } = detail
          if (!isFixedLayout) {
            for (const oldIndex of [...mountedDocs.keys()]) {
              if (oldIndex !== index) {
                detachMountedSection(oldIndex)
              }
            }
          }

          attachMountedSection(index, doc)
        },
        { signal: lifecycleController.signal }
      )

      activeView.addEventListener(
        "relocate",
        (event: Event) => {
          if (destroyed || continuousRenderer !== null) return
          const detail = (event as CustomEvent<FoliateRelocateDetail>).detail
          if (!detail) return

          const sectionIndex =
            typeof detail.section?.current === "number" &&
            Number.isFinite(detail.section.current)
              ? detail.section.current
              : 0
          const sectionTotal =
            typeof detail.section?.total === "number" &&
            Number.isFinite(detail.section.total)
              ? detail.section.total
              : activeBook.sections.length

          if (isFixedLayout && activeView.renderer?.getContents) {
            const activeDocs = new Set(
              activeView.renderer.getContents().map((item) => item.doc)
            )
            if (activeDocs.size > 0) {
              for (const [idx, mountedDoc] of [...mountedDocs.entries()]) {
                if (!activeDocs.has(mountedDoc)) {
                  detachMountedSection(idx)
                }
              }
            }
          }

          const pageStats = computePageStats(
            activeView.renderer,
            currentStyle.paginatorAttributes.flow,
            isFixedLayout
          )

          const renderer = activeView.renderer
          let unitOffset = 0
          if (
            renderer?.scrolled === true &&
            typeof renderer.start === "number" &&
            typeof renderer.viewSize === "number" &&
            Number.isFinite(renderer.start) &&
            Number.isFinite(renderer.viewSize) &&
            renderer.viewSize > 0
          ) {
            unitOffset = clamp01(renderer.start / renderer.viewSize)
          } else if (
            typeof renderer?.page === "number" &&
            typeof renderer?.pages === "number" &&
            Number.isFinite(renderer.page) &&
            Number.isFinite(renderer.pages) &&
            renderer.pages > 2
          ) {
            unitOffset = clamp01((renderer.page - 1) / (renderer.pages - 2))
          } else if (
            lastRendererFraction &&
            lastRendererFraction.index === sectionIndex
          ) {
            unitOffset = lastRendererFraction.fraction
          } else if (pageStats && pageStats.total > 1) {
            unitOffset = (pageStats.current - 1) / (pageStats.total - 1)
          }

          emitRelocationSnapshot({
            sectionIndex,
            sectionTotal,
            bookFraction: detail.fraction,
            unitOffset,
            range: detail.range,
            pageStats,
          })
        },
        { signal: lifecycleController.signal }
      )

      const goToLocator = async (locator: BookLocator): Promise<void> => {
        await navigateToLocatorWithRenderer(
          continuousRenderer ?? activeView.renderer,
          locator
        )
      }

      const startInContinuous =
        !isFixedLayout &&
        params.initialStyle.paginatorAttributes.flow === "scrolled" &&
        params.initialStyle.seamlessScroll

      if (startInContinuous) {
        await enterContinuousScrollMode(params.initialLocator ?? null)
      } else {
        applyStyleInternal(params.initialStyle)
        if (params.initialLocator && params.initialLocator.kind === "epub") {
          const navigated = await navigateToLocatorWithRenderer(
            activeView.renderer,
            params.initialLocator
          )
          if (!navigated) {
            await activeView.init({ showTextStart: true })
          }
        } else {
          await activeView.init({ showTextStart: true })
        }
      }
      checkNotAborted()

      const instance: ReaderDocumentInstance = {
        format: "EPUB",
        capabilities,
        metadata,
        navigation,
        getSessionId: () =>
          activeLoader.isDisposed ? null : activeLoader.sessionId,
        applyStyle: applyStyleInternal,
        setViewState: (state: EphemeralReaderViewState) => {
          if (destroyed) return
          pipeline.updateViewState(state)
          if (mountedDocs.size > 0) {
            relayout.scheduleRelayout()
          }
        },
        goLeft: async () => {
          if (destroyed) return
          if (continuousRenderer) {
            await continuousRenderer.scrollByPage("prev")
            return
          }
          await activeView.goLeft()
        },
        goRight: async () => {
          if (destroyed) return
          if (continuousRenderer) {
            await continuousRenderer.scrollByPage("next")
            return
          }
          await activeView.goRight()
        },
        goPrev: async () => {
          if (destroyed) return
          if (continuousRenderer) {
            await continuousRenderer.scrollByPage("prev")
            return
          }
          await activeView.prev()
        },
        goNext: async () => {
          if (destroyed) return
          if (continuousRenderer) {
            await continuousRenderer.scrollByPage("next")
            return
          }
          await activeView.next()
        },
        goToLocator,
        goToFraction: async (fraction: number) => {
          if (destroyed) return
          if (continuousRenderer) {
            await continuousRenderer.goToFraction(clamp01(fraction))
            return
          }
          await activeView.goToFraction(clamp01(fraction))
        },
        clearSelection: () => {
          observer.clearSelection()
          for (const doc of mountedDocs.values()) {
            doc.defaultView?.getSelection()?.removeAllRanges()
          }
        },
        destroy: async (options) => {
          if (destroyed) return
          destroyed = true
          abortSignal?.removeEventListener("abort", onExternalAbort)
          lifecycleController.abort()
          abortAllSectionSelectionListeners()
          observer.dispose()
          bridge.dispose()
          pipeline.dispose()
          relayout.dispose()
          mountedDocs.clear()
          try {
            continuousRenderer?.destroy()
          } catch {
            // Ignore errors during continuous scroll teardown.
          }
          continuousRenderer = null
          try {
            activeView.close()
          } catch {
            // Ignore errors during teardown.
          }
          activeView.remove()
          try {
            activeBook.destroy?.()
          } catch {
            // Ignore errors during teardown.
          }
          const keepSession =
            options?.keepBackendSession ?? !manageSessionLifecycleInFrontend
          await activeLoader.close(keepSession)
        },
      }

      return instance
    } catch (error) {
      if (!loader && loaderPromise) {
        void loaderPromise
          .then((opened) => {
            const forceCloseUntracked = shouldForceCloseUntracked(opened)
            return opened.close(
              forceCloseUntracked ? false : !manageSessionLifecycleInFrontend,
              { forceCloseUntracked }
            )
          })
          .catch(() => {})
      }
      await cleanupPartialOpen()
      if (abortSignal?.aborted) {
        throw createAbortError()
      }
      throw error
    }
  }
}

export const foliateEpubEngine = new FoliateEpubEngine()
