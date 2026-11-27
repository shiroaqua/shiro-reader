export type ReaderSurfacePreset =
  | "follow_app"
  | "paper_light"
  | "sepia_warm"
  | "eye_green"
  | "night_dark"
  | "amoled_black"

export type ReaderFontPreset = "sans" | "serif" | "mono"

export type ReaderFontChoice =
  | { readonly kind: "publisher" }
  | { readonly kind: "preset"; readonly preset: ReaderFontPreset }
  | { readonly kind: "system"; readonly family: string }
  | { readonly kind: "custom"; readonly fontId: string }

export type ReaderTextAlign = "auto" | "start" | "justify"
export type ReaderHyphenation = "auto" | "none"
export type ReaderFlowMode = "paginated" | "scrolled"
export type ReaderColumnCount = "auto" | 1 | 2

export interface ReaderAppearanceSettings {
  readonly surface: ReaderSurfacePreset
}

export interface ReaderTypographySettings {
  readonly font: ReaderFontChoice
  readonly fontSizePx: number
  readonly fontWeight: number | "auto"
  readonly lineHeight: number
  readonly letterSpacingEm: number
  readonly wordSpacingEm: number
  readonly textAlign: ReaderTextAlign
  readonly hyphenation: ReaderHyphenation
  readonly overridePublisherStyles: boolean
}

export interface ReaderLayoutSettings {
  readonly paragraphSpacingEm: number
  readonly paragraphIndentEm: "auto" | number
  readonly pageMarginInlinePx: number
  readonly pageMarginBlockPx: number
  readonly maxInlineSizePx: number
  readonly columnCount: ReaderColumnCount
  readonly columnGapPercent: number
}

export interface ReaderBehaviorSettings {
  readonly flow: ReaderFlowMode
  readonly animatedPageTurn: boolean
  readonly tapZonesEnabled: boolean
  readonly showPageNumber: boolean
  readonly seamlessScroll: boolean
}

export interface ReaderProfile {
  readonly appearance: ReaderAppearanceSettings
  readonly typography: ReaderTypographySettings
  readonly layout: ReaderLayoutSettings
  readonly behavior: ReaderBehaviorSettings
}

export interface ReaderProfilePatch {
  readonly appearance?: Partial<ReaderAppearanceSettings> | undefined
  readonly typography?: Partial<ReaderTypographySettings> | undefined
  readonly layout?: Partial<ReaderLayoutSettings> | undefined
  readonly behavior?: Partial<ReaderBehaviorSettings> | undefined
}

export type CustomFontFormat = "ttf" | "otf"

export interface CustomFontMeta {
  readonly fontId: string
  readonly familyName: string
  readonly fileName: string
  readonly format: CustomFontFormat
  readonly sizeBytes: number
}

export interface SystemFontFamily {
  readonly family: string
}

export interface ReaderSettingsSnapshot {
  readonly global: ReaderProfile
  readonly bookOverrides: Readonly<Record<string, ReaderProfilePatch>>
  readonly customFonts: readonly CustomFontMeta[]
  readonly fontUrlBase: string
  readonly canEnumerateSystemFonts: boolean
}

export interface ReaderPaginatorAttributes {
  readonly flow: ReaderFlowMode
  readonly margin: string
  readonly gap: string
  readonly maxInlineSize: string
  readonly maxColumnCount: string
  readonly animated: boolean
}

export interface CompiledFontFace {
  readonly family: string
  readonly fontId: string
  readonly url: string
  readonly format: CustomFontFormat
}

export interface CompiledReaderStyle {
  readonly surface: ReaderSurfacePreset
  readonly isDarkSurface: boolean
  readonly surfaceBackgroundHex: string
  readonly viewportCssVars: Readonly<Record<string, string>>
  readonly viewportPaddingInlinePx: number
  readonly paginatorAttributes: ReaderPaginatorAttributes
  readonly showPageNumber: boolean
  readonly seamlessScroll: boolean
  readonly chapterCss: string
  readonly customFontFace: CompiledFontFace | null
  readonly usesFontFaceFallback: boolean
}
