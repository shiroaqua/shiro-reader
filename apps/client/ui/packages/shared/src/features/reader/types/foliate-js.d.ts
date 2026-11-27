declare module "foliate-js/epub.js" {
  export interface EpubLoader {
    getSize(name: string): number
    /** 归档内不存在该可选文件时必须返回 null（不可抛错），否则 EPUB.init() 解析可选 XML 时会中断 */
    loadBlob(name: string): Promise<Blob | null>
    loadText(name: string): Promise<string | null>
    /** IDPF 字体去混淆 SHA-1（输入为书籍标识符字符串而非文件名；非安全上下文下须由前端提供纯 JS 实现） */
    sha1?: ((str: string) => Promise<Uint8Array>) | undefined
  }

  export interface EpubTocEntry {
    id?: number | string | undefined
    label?: string | undefined
    href?: string | null | undefined
    subitems?: EpubTocEntry[] | null | undefined
  }

  export interface EpubSection {
    id: string | number
    linear?: string | null | undefined
    size: number
    cfi?: string | undefined
    load?: (() => Promise<string>) | undefined
    unload?: (() => void) | undefined
    resolveHref?: ((href: string) => string) | undefined
    createDocument?: (() => Promise<Document>) | undefined
  }

  export interface EpubDataTransformDetail {
    readonly name: string
    data: string | Blob | Promise<string | Blob>
    type: string | Promise<string>
  }

  export interface EpubManifestItem {
    id: string
    href: string
    mediaType: string
    properties?: string[] | undefined
    mediaOverlay?: string | undefined
  }

  export interface EpubSpineItem {
    idref: string
    id?: string | undefined
    linear?: string | undefined
    properties?: string[] | undefined
  }

  export interface EpubResources {
    manifest?: EpubManifestItem[] | undefined
    spine?: EpubSpineItem[] | undefined
    cover?: EpubManifestItem | undefined
    getItemByID?(id: string): EpubManifestItem | undefined
    getItemByHref?(href: string): EpubManifestItem | undefined
  }

  export class EPUB {
    constructor(loader: EpubLoader)
    init(): Promise<this>
    transformTarget?: EventTarget | undefined
    resources?: EpubResources | undefined
    loadText?: ((name: string) => Promise<string | null>) | undefined
    loadBlob?: ((name: string) => Promise<Blob | null>) | undefined
    getCover?: (() => Promise<Blob | null>) | undefined
    metadata?:
      | {
          title?: string | Record<string, string> | undefined
          author?:
            | string
            | Record<string, string>
            | { name?: string | Record<string, string> | undefined }
            | Array<
                | string
                | Record<string, string>
                | { name?: string | Record<string, string> | undefined }
              >
            | undefined
          language?: string | string[] | undefined
        }
      | undefined
    dir?: "ltr" | "rtl" | "default" | null | undefined
    rendition?: { layout?: "pre-paginated" | "reflowable" | undefined } | undefined
    toc?: EpubTocEntry[] | undefined
    sections: EpubSection[]
    resolveCFI?(cfi: string): { index: number; anchor: (doc: Document) => Range | Element | null }
    resolveHref?(href: string): { index: number; anchor: (doc: Document) => Element | number | null } | null
    isExternal?(href: string): boolean
    destroy?(): void
  }
}

declare module "foliate-js/view.js" {
  import type { EPUB, EpubTocEntry } from "foliate-js/epub.js"

  export interface FoliateRendererElement extends HTMLElement {
    setStyles?(styles: string | [string, string]): void
    /** Paginator 公开的同步重排方法（FixedLayout 无公开 render，须用可选链调用） */
    render?(): void
    readonly scrolled?: boolean | undefined
    readonly size?: number | undefined
    readonly start?: number | undefined
    readonly end?: number | undefined
    readonly viewSize?: number | undefined
    readonly page?: number | undefined
    readonly pages?: number | undefined
    prev?(distance?: number): Promise<void>
    next?(distance?: number): Promise<void>
    goTo?(target: {
      index: number
      anchor?: ((doc: Document) => Range | Element | number | null) | undefined
    }): Promise<void>
    /** 注意：<foliate-fxl> 在双页模式下会返回多个对象且不保证带 index */
    getContents?(): Array<{ index?: number | undefined; doc: Document; overlayer?: unknown }>
  }

  export interface FoliateLoadDetail {
    doc: Document
    index: number
  }

  /** 对齐 view.js #onRelocate 真实事件载荷：当前章节序号位于 detail.section.current，无顶层 index */
  export interface FoliateRelocateDetail {
    cfi: string
    fraction: number
    section: { current: number; total: number }
    location?: { current: number; next: number; total: number } | undefined
    time?: { section: number; total: number } | undefined
    range?: Range | null | undefined
    tocItem?: EpubTocEntry | null | undefined
    pageItem?: { label: string } | null | undefined
  }

  export interface FoliateExternalLinkDetail {
    a: HTMLAnchorElement
    /** 兼容上游不同提交对 href_ / href 的字段命名 */
    href?: string | undefined
    href_?: string | undefined
  }

  export interface FoliateLinkDetail {
    a: HTMLAnchorElement
    href: string
  }

  export class View extends HTMLElement {
    renderer: FoliateRendererElement
    book?: EPUB | undefined
    open(book: EPUB): Promise<void>
    init(options: { lastLocation?: string | undefined; showTextStart?: boolean | undefined }): Promise<void>
    close(): void
    goLeft(): Promise<void>
    goRight(): Promise<void>
    prev(distance?: number): Promise<void>
    next(distance?: number): Promise<void>
    goTo(target: string | number): Promise<unknown>
    goToFraction(fraction: number): Promise<void>
    getCFI(index: number, range?: Range | undefined): string
    resolveCFI(cfi: string): { index: number; anchor: (doc: Document) => Range | Element | null }
  }
}

declare module "foliate-js/epubcfi.js" {
  export type CfiNodeFilter = (node: Node) => number

  export interface CfiStepPart {
    index: number
    id?: string | null | undefined
    offset?: number | undefined
    temporal?: number | undefined
    spatial?: number[] | undefined
    text?: string[] | undefined
    side?: string | undefined
  }

  export type ParsedPointCfi = CfiStepPart[][]

  export interface ParsedRangeCfi {
    parent: CfiStepPart[][]
    start: CfiStepPart[][]
    end: CfiStepPart[][]
  }

  /** 由 parse() 返回、作为 toRange() 唯一入参的 CFI AST 结构 */
  export type ParsedCfi = ParsedPointCfi | ParsedRangeCfi

  export const isCFI: RegExp
  export function parse(cfi: string): ParsedCfi
  export function collapse(cfi: string, toEnd?: boolean | undefined): string
  export function collapse(
    cfi: ParsedCfi,
    toEnd?: boolean | undefined
  ): ParsedPointCfi
  export function compare(
    a: string | ParsedCfi,
    b: string | ParsedCfi
  ): number
  export function fromRange(
    range: Range,
    filter?: CfiNodeFilter | undefined
  ): string
  /** 仅接受 parse() 产出的 ParsedCfi，严禁直接传入原始 CFI 字符串 */
  export function toRange(
    doc: Document,
    parts: ParsedCfi,
    filter?: CfiNodeFilter | undefined
  ): Range
  export function joinIndir(...cfis: string[]): string
  export const fake: {
    readonly fromIndex: (index: number) => string
    readonly toIndex: (parts: CfiStepPart[] | null | undefined) => number
  }
}

declare module "foliate-js/overlayer.js" {
  export interface OverlayerRect {
    readonly left: number
    readonly top: number
    readonly right: number
    readonly bottom: number
    readonly width: number
    readonly height: number
  }

  export type OverlayerDrawFn<TOptions = Record<string, unknown>> = (
    rects: OverlayerRect[],
    options?: TOptions | undefined
  ) => SVGElement

  export class Overlayer {
    constructor(doc: Document)
    readonly element: SVGSVGElement
    add<TOptions = Record<string, unknown>>(
      key: string,
      range: Range | ((root: Node) => Range),
      draw: OverlayerDrawFn<TOptions>,
      options?: TOptions | undefined
    ): void
    remove(key: string): void
    redraw(): void
    hitTest(point: { x: number; y: number }): [string, Range] | []
    static highlight: OverlayerDrawFn<{ color?: string | undefined; padding?: number | undefined }>
    static underline: OverlayerDrawFn<{
      color?: string | undefined
      width?: number | undefined
      padding?: number | undefined
      writingMode?: string | undefined
    }>
    static outline: OverlayerDrawFn<{
      color?: string | undefined
      width?: number | undefined
      padding?: number | undefined
      radius?: number | undefined
    }>
  }
}
