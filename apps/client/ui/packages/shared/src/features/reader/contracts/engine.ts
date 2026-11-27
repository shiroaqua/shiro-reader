import type { UUID } from "@shiro/ui-shared/lib/types"
import type { SectionAugmenter } from "./augmentation"
import type { SelectionContext } from "./selection"
import type { CompiledReaderStyle } from "./settings"

export type BookFormat = "EPUB" | "PDF"

export function normalizeBookFormat(rawType: string): BookFormat | "UNKNOWN" {
  const upper = rawType.trim().toUpperCase()
  if (upper === "EPUB") return "EPUB"
  if (upper === "PDF") return "PDF"
  return "UNKNOWN"
}

/** 驱动设置面板、双语栏与工具栏按需启用/隐藏控件的能力矩阵 */
export interface EngineCapabilities {
  readonly supportsReflowTypography: boolean
  readonly supportsPublisherStyleOverride: boolean
  readonly supportsFlowSwitch: boolean
  readonly supportsColumnSwitch: boolean
  readonly supportsTextSelection: boolean
  readonly supportsToc: boolean
  readonly supportsFixedZoom: boolean
  /** 仅流式可重排 EPUB 为 true；固定版式纯图漫画 EPUB 与所有 PDF 恒为 false */
  readonly supportsFullTextTranslation: boolean
  readonly supportsBilingualInterleave: boolean
}

/** 标签页级轻量瞬态视图模式（随多标签切换与桌面跨窗口拖拽一并保持） */
export type TranslationDisplayMode = "original" | "bilingual" | "translated_only"

export interface EphemeralReaderViewState {
  readonly translationMode: TranslationDisplayMode
  readonly targetLanguage: string | null
}

/**
 * 直接镜像后端 `shiro_application::library::book::dto::BookLocatorOutput`
 * 严格保持 `snake_case` 字段名且不附带额外属性。
 */
export type PdfDest =
  | { readonly kind: "page_only" }
  | { readonly kind: "fit_page" }
  | {
      readonly kind: "xyz"
      readonly left: number | null
      readonly top: number | null
      readonly zoom: number | null
      readonly normalized_top: number | null
    }
  | {
      readonly kind: "fit_h"
      readonly top: number | null
      readonly normalized_top: number | null
    }

export type BookLocator =
  | {
      readonly kind: "epub"
      readonly spine_index: number | null
      readonly href: string
      readonly fragment: string | null
      readonly cfi: string | null
    }
  | {
      readonly kind: "pdf"
      readonly page_index: number
      readonly dest: PdfDest
    }

export const TOC_FLAG_DEFAULT_COLLAPSED = 1
export const TOC_FLAG_EXTERNAL_LINK = 2

/** 镜像后端扁平先序 Arena 目录节点 `TocNodeOutput` */
export interface ReaderTocNode {
  readonly parent: number | null
  readonly depth: number
  readonly subtree_end: number
  readonly title: string
  readonly locator: BookLocator | null
  readonly unit_index: number | null
  readonly unit_offset: number
  readonly start_progression: number | null
  /** bit 0 (1): DEFAULT_COLLAPSED; bit 1 (2): EXTERNAL_LINK */
  readonly flags: number
  /** 可选外链原始地址（当 EXTERNAL_LINK 置位时由前端引擎从 EPUB TOC 补全） */
  readonly external_url?: string | null | undefined
}

export interface ReaderLinearUnit {
  readonly index: number
  readonly is_linear: boolean
  readonly weight: number
  readonly cumulative_weight: number
  readonly display_label: string | null
}

export interface BookNavigationData {
  readonly toc: readonly ReaderTocNode[]
  readonly linear_units: readonly ReaderLinearUnit[]
  readonly total_weight: number
}

export interface ReaderPageStats {
  readonly current: number
  readonly total: number
}

/** 格式无关的实时阅读位置快照 */
export interface RelocationSnapshot {
  readonly locator: BookLocator
  readonly fraction: number
  readonly sectionIndex: number
  readonly sectionTotal: number
  readonly activeTocIndex: number | null
  readonly pageStats: ReaderPageStats | null
}

export interface ReaderBookMetadata {
  readonly title: string
  readonly author: string | null
  readonly language: string | null
  readonly direction: "ltr" | "rtl"
  readonly isFixedLayout: boolean
}

export interface ReaderEngineEvents {
  onRelocate: (snapshot: RelocationSnapshot) => void
  onSelectionChange: (selection: SelectionContext | null) => void
  onViewportTap: (normX: number, normY: number) => void
  onExternalLink: (url: string) => void
  onInternalLink?: ((href: string, before: RelocationSnapshot | null) => void) | undefined
  onCopySelection?: ((ctx: SelectionContext) => Promise<void>) | undefined
  onKeyCommand: (event: KeyboardEvent) => void
  onFatalError: (error: Error) => void
  onSessionChange?: ((sessionId: string) => void) | undefined
}

export interface OpenDocumentParams {
  readonly tabId: string
  readonly bookId: UUID
  readonly bookHash: string
  readonly fallbackTitle: string
  readonly preloadedNavigation: BookNavigationData | null
  readonly reattachSessionId?: string | undefined
  readonly manageSessionLifecycleInFrontend?: boolean | undefined
  readonly initialLocator: BookLocator | undefined
  readonly initialViewState?: EphemeralReaderViewState | undefined
  readonly initialStyle: CompiledReaderStyle
  readonly container: HTMLElement
  readonly outerContainer?: HTMLElement | undefined
  readonly augmenters?: readonly SectionAugmenter[] | undefined
  readonly signal?: AbortSignal | undefined
  readonly events: ReaderEngineEvents
}

export interface ReaderDocumentInstance {
  readonly format: BookFormat
  readonly capabilities: EngineCapabilities
  readonly metadata: ReaderBookMetadata
  readonly navigation: BookNavigationData
  readonly getSessionId?: (() => string | null) | undefined

  applyStyle(style: CompiledReaderStyle): void
  setViewState(state: EphemeralReaderViewState): void

  goLeft(): Promise<void>
  goRight(): Promise<void>
  goPrev(): Promise<void>
  goNext(): Promise<void>
  goToLocator(locator: BookLocator): Promise<void>
  goToFraction(fraction: number): Promise<void>

  clearSelection(): void
  destroy(options?: { keepBackendSession?: boolean | undefined }): Promise<void>
}

export interface ReaderEngineAdapter {
  readonly format: BookFormat
  open(params: OpenDocumentParams): Promise<ReaderDocumentInstance>
}
