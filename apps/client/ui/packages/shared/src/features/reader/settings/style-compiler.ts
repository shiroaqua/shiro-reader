import { getCustomFontProtocolUrl } from "@shiro/ui-shared/api/reader-bridge"
import type { AppTheme } from "@shiro/ui-shared/lib/theme"
import type {
  CompiledFontFace,
  CompiledReaderStyle,
  CustomFontMeta,
  ReaderFlowMode,
  ReaderFontChoice,
  ReaderPaginatorAttributes,
  ReaderProfile,
} from "../contracts/settings"
import {
  READER_FONT_PRESET_STACKS,
  READER_SETTING_BOUNDS,
  resolveSurfacePalette,
} from "./defaults"

export interface CompileReaderStyleOptions {
  readonly isFixedLayout?: boolean | undefined
  readonly useFontFaceFallback?: boolean | undefined
  readonly viewportHeightPx?: number | undefined
}

function clampNumber(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback
  return Math.min(max, Math.max(min, value))
}

export function escapeCssString(value: string): string {
  let out = ""
  for (let i = 0; i < value.length; i++) {
    const ch = value[i]!
    const code = ch.charCodeAt(0)
    if (ch === "\\" || ch === '"' || ch === "'") {
      out += `\\${ch}`
    } else if (code < 0x20 || code === 0x7f) {
      out += `\\${code.toString(16)} `
    } else {
      out += ch
    }
  }
  return out
}

export function escapeCssUrl(rawUrl: string): string {
  const trimmed = rawUrl.trim()
  const lower = trimmed.toLowerCase()
  const isSafe =
    lower.startsWith("http://") ||
    lower.startsWith("https://") ||
    lower.startsWith("shiro-font://") ||
    lower.startsWith("/")
  if (!isSafe) {
    return "about:blank"
  }

  let out = ""
  for (let i = 0; i < trimmed.length; i++) {
    const ch = trimmed[i]!
    const code = ch.charCodeAt(0)
    if (ch === "\\" || ch === '"' || ch === "'" || ch === "(" || ch === ")") {
      out += `\\${ch}`
    } else if (code < 0x20 || code === 0x7f) {
      out += encodeURIComponent(ch)
    } else {
      out += ch
    }
  }
  return out
}

function resolveFontStack(
  choice: ReaderFontChoice,
  customFonts: readonly CustomFontMeta[],
  fontUrlBase: string
): {
  readonly bodyFontCss: string | null
  readonly customFontFace: CompiledFontFace | null
} {
  switch (choice.kind) {
    case "publisher":
      return { bodyFontCss: null, customFontFace: null }
    case "preset":
      return {
        bodyFontCss: READER_FONT_PRESET_STACKS[choice.preset],
        customFontFace: null,
      }
    case "system": {
      const family = choice.family.trim()
      if (!family) return { bodyFontCss: null, customFontFace: null }
      return {
        bodyFontCss: `"${escapeCssString(family)}", sans-serif`,
        customFontFace: null,
      }
    }
    case "custom": {
      const meta = customFonts.find((item) => item.fontId === choice.fontId)
      if (!meta) return { bodyFontCss: null, customFontFace: null }
      const url = getCustomFontProtocolUrl(fontUrlBase, meta.fontId)
      return {
        bodyFontCss: `"${escapeCssString(meta.familyName)}", serif`,
        customFontFace: {
          family: meta.familyName,
          fontId: meta.fontId,
          url,
          format: meta.format,
        },
      }
    }
  }
}

export function compileReaderStyle(
  profile: ReaderProfile,
  appTheme: AppTheme,
  customFonts: readonly CustomFontMeta[],
  fontUrlBase: string,
  isMobile: boolean,
  viewportWidthPx: number,
  options: CompileReaderStyleOptions = {}
): CompiledReaderStyle {
  const isFixedLayout = options.isFixedLayout === true
  const useFontFaceFallback = options.useFontFaceFallback === true
  const palette = resolveSurfacePalette(profile.appearance.surface, appTheme)

  const fontSizePx = clampNumber(
    profile.typography.fontSizePx,
    READER_SETTING_BOUNDS.fontSizePx.min,
    READER_SETTING_BOUNDS.fontSizePx.max,
    18
  )
  const fontWeight =
    profile.typography.fontWeight === "auto"
      ? null
      : clampNumber(
          profile.typography.fontWeight,
          READER_SETTING_BOUNDS.fontWeight.min,
          READER_SETTING_BOUNDS.fontWeight.max,
          400
        )
  const lineHeight = clampNumber(
    profile.typography.lineHeight,
    READER_SETTING_BOUNDS.lineHeight.min,
    READER_SETTING_BOUNDS.lineHeight.max,
    1.75
  )
  const letterSpacingEm = clampNumber(
    profile.typography.letterSpacingEm,
    READER_SETTING_BOUNDS.letterSpacingEm.min,
    READER_SETTING_BOUNDS.letterSpacingEm.max,
    0
  )
  const wordSpacingEm = clampNumber(
    profile.typography.wordSpacingEm,
    READER_SETTING_BOUNDS.wordSpacingEm.min,
    READER_SETTING_BOUNDS.wordSpacingEm.max,
    0
  )
  const paragraphSpacingEm = clampNumber(
    profile.layout.paragraphSpacingEm,
    READER_SETTING_BOUNDS.paragraphSpacingEm.min,
    READER_SETTING_BOUNDS.paragraphSpacingEm.max,
    0.75
  )
  const paragraphIndentEm =
    profile.layout.paragraphIndentEm === "auto"
      ? null
      : clampNumber(
          profile.layout.paragraphIndentEm,
          READER_SETTING_BOUNDS.paragraphIndentEm.min,
          READER_SETTING_BOUNDS.paragraphIndentEm.max,
          0
        )
  const pageMarginInlinePx = clampNumber(
    profile.layout.pageMarginInlinePx,
    READER_SETTING_BOUNDS.pageMarginInlinePx.min,
    READER_SETTING_BOUNDS.pageMarginInlinePx.max,
    READER_SETTING_BOUNDS.pageMarginInlinePx.min
  )
  const pageMarginBlockPx = clampNumber(
    profile.layout.pageMarginBlockPx,
    READER_SETTING_BOUNDS.pageMarginBlockPx.min,
    READER_SETTING_BOUNDS.pageMarginBlockPx.max,
    READER_SETTING_BOUNDS.pageMarginBlockPx.min
  )
  const maxInlineSizePx = Math.round(
    clampNumber(
      profile.layout.maxInlineSizePx,
      READER_SETTING_BOUNDS.maxInlineSizePx.min,
      READER_SETTING_BOUNDS.maxInlineSizePx.max,
      760
    )
  )
  const columnGapPercent = Math.round(
    clampNumber(
      profile.layout.columnGapPercent,
      READER_SETTING_BOUNDS.columnGapPercent.min,
      READER_SETTING_BOUNDS.columnGapPercent.max,
      6
    )
  )

  // Fixed-layout EPUBs (<foliate-fxl>) are always paginated; coercing here ensures
  // tap zones remain functional even if global flow is "scrolled".
  const flow: ReaderFlowMode = isFixedLayout ? "paginated" : profile.behavior.flow
  // Never apply horizontal outer container padding in scrolled or fixed-layout modes.
  const viewportPaddingInlinePx =
    flow === "scrolled" || isFixedLayout
      ? 0
      : isMobile
        ? Math.max(0, Math.min(60, Math.round(pageMarginInlinePx * 0.5)))
        : Math.max(0, Math.min(72, Math.round((pageMarginInlinePx - 8) * 0.6)))

  const maxColumnCount =
    isMobile || profile.layout.columnCount === 1 ? "1" : "2"
  const isLandscape =
    options.viewportHeightPx === undefined ||
    !Number.isFinite(options.viewportHeightPx) ||
    options.viewportHeightPx <= 0 ||
    viewportWidthPx >= options.viewportHeightPx
  let effectiveMaxInlineSizePx = maxInlineSizePx
  if (
    !isMobile &&
    profile.layout.columnCount === 2 &&
    isLandscape &&
    Number.isFinite(viewportWidthPx) &&
    viewportWidthPx > 0
  ) {
    const halfViewport = Math.floor(viewportWidthPx / 2) - 1
    effectiveMaxInlineSizePx = Math.max(
      240,
      Math.min(maxInlineSizePx, halfViewport)
    )
  }

  const paginatorAttributes: ReaderPaginatorAttributes = {
    flow,
    margin: `${Math.round(pageMarginBlockPx)}px`,
    gap: `${columnGapPercent}%`,
    maxInlineSize: `${effectiveMaxInlineSizePx}px`,
    maxColumnCount,
    animated: profile.behavior.animatedPageTurn,
  }

  const surface = profile.appearance.surface
  const semanticSurfaceVars: Record<string, string> =
    surface === "follow_app"
      ? {}
      : palette.isDark
        ? {
            "--background": palette.chromeBgHex,
            "--foreground": palette.foregroundHex,
            "--card": palette.chromeBgHex,
            "--card-foreground": palette.foregroundHex,
            "--popover": palette.chromeBgHex,
            "--popover-foreground": palette.foregroundHex,
            "--primary": palette.linkHex,
            "--primary-foreground": "#0b0c0e",
            "--secondary": surface === "amoled_black" ? "#181a20" : "#2d3039",
            "--secondary-foreground": palette.foregroundHex,
            "--muted": surface === "amoled_black" ? "#15171c" : "#2a2d36",
            "--muted-foreground": palette.mutedHex,
            "--accent": surface === "amoled_black" ? "#1e2129" : "#343844",
            "--accent-foreground": palette.foregroundHex,
            "--border": palette.chromeBorderHex,
            "--input": palette.chromeBorderHex,
            "--ring": palette.linkHex,
            "--destructive": "#f87171",
          }
        : {
            "--background": palette.backgroundHex,
            "--foreground": palette.foregroundHex,
            "--card": palette.chromeBgHex,
            "--card-foreground": palette.foregroundHex,
            "--popover": palette.chromeBgHex,
            "--popover-foreground": palette.foregroundHex,
            "--primary": palette.linkHex,
            "--primary-foreground": "#ffffff",
            "--secondary": palette.chromeBgHex,
            "--secondary-foreground": palette.foregroundHex,
            "--muted": palette.borderHex,
            "--muted-foreground": palette.mutedHex,
            "--accent": palette.borderHex,
            "--accent-foreground": palette.foregroundHex,
            "--border": palette.chromeBorderHex,
            "--input": palette.chromeBorderHex,
            "--ring": palette.linkHex,
          }

  const viewportCssVars: Readonly<Record<string, string>> = {
    ...semanticSurfaceVars,
    "--reader-bg": palette.backgroundHex,
    "--reader-fg": palette.foregroundHex,
    "--reader-muted": palette.mutedHex,
    "--reader-border": palette.borderHex,
    "--reader-chrome-bg": palette.chromeBgHex,
    "--reader-chrome-border": palette.chromeBorderHex,
    "--reader-selection-bg": palette.selectionBgHex,
    "--reader-selection-fg": palette.selectionFgHex,
    "--reader-link": palette.linkHex,
    "--reader-hover-bg": palette.isDark
      ? "rgba(255, 255, 255, 0.1)"
      : "rgba(0, 0, 0, 0.05)",
    "--reader-hover-strong-bg": palette.isDark
      ? "rgba(255, 255, 255, 0.16)"
      : "rgba(0, 0, 0, 0.1)",
  }

  const { bodyFontCss, customFontFace } = resolveFontStack(
    profile.typography.font,
    customFonts,
    fontUrlBase
  )

  const cssParts: string[] = []

  if (customFontFace && !useFontFaceFallback) {
    const escapedFamily = escapeCssString(customFontFace.family)
    const escapedUrl = escapeCssUrl(customFontFace.url)
    const formatHint =
      customFontFace.format === "otf" ? "opentype" : "truetype"
    cssParts.push(`@font-face {
  font-family: "${escapedFamily}";
  src: url("${escapedUrl}") format("${formatHint}");
  font-weight: 100 900;
  font-style: normal;
  font-display: swap;
}`)
  }

  const effectiveBodyFont = bodyFontCss ?? "inherit"
  cssParts.push(`:root {
  --theme-bg-color: ${palette.backgroundHex};
  --shiro-body-font: ${effectiveBodyFont};
  --shiro-body-size: ${fontSizePx}px;
  --shiro-body-weight: ${fontWeight ?? "inherit"};
  --shiro-body-line-height: ${lineHeight};
  --shiro-body-letter-spacing: ${letterSpacingEm}em;
  --shiro-body-word-spacing: ${wordSpacingEm}em;
  --shiro-body-fg: ${palette.foregroundHex};
  --shiro-body-bg: ${palette.backgroundHex};
  --shiro-trans-font: ${effectiveBodyFont};
  --shiro-trans-scale: 0.92;
  --shiro-trans-fg-muted: ${palette.mutedHex};
  --shiro-bilingual-inner-gap: 0.35em;
}`)

  const textAlignRule =
    profile.typography.textAlign !== "auto"
      ? `  text-align: ${profile.typography.textAlign};\n`
      : ""
  const hyphensRule =
    profile.typography.hyphenation === "none"
      ? "  hyphens: none;\n  -webkit-hyphens: none;\n"
      : "  hyphens: auto;\n  -webkit-hyphens: auto;\n"
  const fontFamilyRule = bodyFontCss
    ? `  font-family: var(--shiro-body-font);\n`
    : ""
  const fontWeightRule =
    fontWeight !== null ? `  font-weight: ${fontWeight};\n` : ""

  const scrolledBodyMarginRule =
    flow === "scrolled" && !isFixedLayout
      ? `body {
  padding-inline: ${Math.round(pageMarginInlinePx)}px;
  box-sizing: border-box;
}`
      : ""

  cssParts.push(`:root, body {
  color-scheme: ${palette.isDark ? "dark" : "light"};
  background-color: ${palette.backgroundHex};
  color: ${palette.foregroundHex};
  font-size: var(--shiro-body-size);
  line-height: var(--shiro-body-line-height);
  letter-spacing: var(--shiro-body-letter-spacing);
  word-spacing: var(--shiro-body-word-spacing);
${fontFamilyRule}${fontWeightRule}${textAlignRule}${hyphensRule}}${scrolledBodyMarginRule ? `\n\n${scrolledBodyMarginRule}` : ""}`)

  if (!isFixedLayout) {
    const codeBg = `color-mix(in srgb, ${palette.foregroundHex} 7%, ${palette.backgroundHex})`
    const markBg = palette.isDark
      ? "rgba(250, 204, 21, 0.24)"
      : "rgba(253, 224, 71, 0.45)"
    cssParts.push(`html:root {
  background-color: ${palette.backgroundHex} !important;
  background-image: none !important;
}
html:root body,
html:root body *:not(img, picture, video, canvas, svg, svg *, hr, pre, code, kbd, samp, mark, [data-shiro-synthetic], [data-shiro-synthetic] *) {
  background-color: transparent !important;
  background-image: none !important;
}
:where(pre, code, kbd, samp) {
  background-color: ${codeBg} !important;
  background-image: none !important;
}
:where(pre) :where(code, kbd, samp) {
  background-color: transparent !important;
}
:where(mark) {
  background-color: ${markBg} !important;
  color: ${palette.foregroundHex} !important;
}`)
  }

  const indentRule =
    paragraphIndentEm !== null ? `  text-indent: ${paragraphIndentEm}em;\n` : ""
  cssParts.push(`p {
  margin-block-end: ${paragraphSpacingEm}em;
${indentRule}}`)

  if (profile.typography.overridePublisherStyles) {
    const importantFont = bodyFontCss
      ? "  font-family: var(--shiro-body-font) !important;\n"
      : ""
    const importantWeight =
      fontWeight !== null ? `  font-weight: ${fontWeight} !important;\n` : ""
    const importantAlign =
      profile.typography.textAlign !== "auto"
        ? `  text-align: ${profile.typography.textAlign} !important;\n`
        : ""
    const protectedResetWeight =
      fontWeight !== null ? "  font-weight: normal;\n" : ""
    const protectedResetAlign =
      profile.typography.textAlign !== "auto" ? "  text-align: start;\n" : ""
    const protectedSubtree =
      "pre, pre *, code, code *, kbd, kbd *, samp, samp *, math, math *, svg, svg *, rt, rt *, rp, rp *, table, table *"

    cssParts.push(`body:not([data-shiro-synthetic="translation"]),
p:not([data-shiro-synthetic="translation"], ${protectedSubtree}),
div:not([data-shiro-synthetic="translation"], ${protectedSubtree}),
li:not([data-shiro-synthetic="translation"], ${protectedSubtree}),
blockquote:not([data-shiro-synthetic="translation"], ${protectedSubtree}),
dd:not([data-shiro-synthetic="translation"], ${protectedSubtree}),
dt:not([data-shiro-synthetic="translation"], ${protectedSubtree}),
span:not([class*="math"], [data-shiro-synthetic="translation"], ${protectedSubtree}) {
${importantFont}${importantWeight}${importantAlign}  line-height: var(--shiro-body-line-height) !important;
  letter-spacing: var(--shiro-body-letter-spacing) !important;
  word-spacing: var(--shiro-body-word-spacing) !important;
}
:where(pre, code, kbd, samp) {
  font-family: ${READER_FONT_PRESET_STACKS.mono};
${protectedResetWeight}${protectedResetAlign}  line-height: normal;
  letter-spacing: normal;
  word-spacing: normal;
}
:where(math, svg) {
${protectedResetWeight}${protectedResetAlign}  line-height: normal;
  letter-spacing: normal;
  word-spacing: normal;
}
:where(rt, rp) {
${protectedResetWeight}  text-align: center;
  line-height: 1.2;
  letter-spacing: normal;
  word-spacing: normal;
}
:where(table) {
  line-height: 1.4;
  letter-spacing: normal;
  word-spacing: normal;
  text-indent: 0;
}`)
  }

  if (palette.isDark) {
    const darkBgRule = isFixedLayout
      ? `:root, body {
  background-color: ${palette.backgroundHex} !important;
  color: ${palette.foregroundHex} !important;
}`
      : `:root, body {
  color: ${palette.foregroundHex} !important;
}`
    cssParts.push(`${darkBgRule}
p:not([data-shiro-synthetic="translation"]), div:not([data-shiro-synthetic="translation"]), span:not([data-shiro-synthetic="translation"]), li:not([data-shiro-synthetic="translation"]), blockquote:not([data-shiro-synthetic="translation"]), h1, h2, h3, h4, h5, h6, dt, dd:not([data-shiro-synthetic="translation"]), figcaption, em:not([data-shiro-synthetic="translation"]), strong:not([data-shiro-synthetic="translation"]), i:not([data-shiro-synthetic="translation"]), b:not([data-shiro-synthetic="translation"]), u:not([data-shiro-synthetic="translation"]), s:not([data-shiro-synthetic="translation"]), del:not([data-shiro-synthetic="translation"]), ins:not([data-shiro-synthetic="translation"]), sub:not([data-shiro-synthetic="translation"]), sup:not([data-shiro-synthetic="translation"]), small:not([data-shiro-synthetic="translation"]), cite:not([data-shiro-synthetic="translation"]), q:not([data-shiro-synthetic="translation"]), dfn:not([data-shiro-synthetic="translation"]), abbr:not([data-shiro-synthetic="translation"]), time:not([data-shiro-synthetic="translation"]), var:not([data-shiro-synthetic="translation"]), samp:not([data-shiro-synthetic="translation"]), kbd:not([data-shiro-synthetic="translation"]), code:not([data-shiro-synthetic="translation"]), pre:not([data-shiro-synthetic="translation"]), caption:not([data-shiro-synthetic="translation"]), label:not([data-shiro-synthetic="translation"]), summary:not([data-shiro-synthetic="translation"]) {
  color: inherit !important;
}
table, thead, tbody, tr, th, td {
  background-color: transparent !important;
  color: inherit !important;
  border-color: ${palette.borderHex} !important;
}
a {
  color: ${palette.linkHex} !important;
}
img, picture, video, svg:has(image), canvas {
  filter: none;
}
svg:not(:has(image)) {
  color: ${palette.foregroundHex};
}
svg:not(:has(image)) :is([stroke="#000" i], [stroke="#000000" i], [stroke="black" i], [stroke="rgb(0,0,0)" i], [stroke="rgb(0, 0, 0)" i]) {
  stroke: ${palette.foregroundHex} !important;
}
svg:not(:has(image)) :is([fill="#000" i], [fill="#000000" i], [fill="black" i], [fill="rgb(0,0,0)" i], [fill="rgb(0, 0, 0)" i]) {
  fill: ${palette.foregroundHex} !important;
}`)
  }

  cssParts.push(`::selection {
  background-color: ${palette.selectionBgHex};
  color: ${palette.selectionFgHex};
}
h1, h2, h3, h4, h5, h6 {
  break-after: avoid-column;
  page-break-after: avoid;
}
html[data-shiro-trans-mode="original"] [data-shiro-synthetic="translation"] {
  display: none !important;
}
html[data-shiro-trans-mode="bilingual"] [data-shiro-has-translation="ready"],
html[data-shiro-trans-mode="bilingual"] [data-shiro-has-translation="streaming"] {
  margin-block-end: 0 !important;
}
html[data-shiro-trans-mode="bilingual"] [data-shiro-synthetic="translation"] {
  display: block;
  margin-block-start: var(--shiro-bilingual-inner-gap);
  margin-block-end: ${paragraphSpacingEm}em;
  font-family: var(--shiro-trans-font) !important;
  font-size: calc(var(--shiro-body-size) * var(--shiro-trans-scale));
  line-height: var(--shiro-body-line-height);
  color: var(--shiro-trans-fg-muted) !important;
  break-inside: auto;
}
html[data-shiro-trans-mode="translated_only"] [data-shiro-has-translation="ready"]:not(td, th, caption) {
  display: none !important;
}
html[data-shiro-trans-mode="translated_only"] :is(td, th, caption)[data-shiro-has-translation="ready"] {
  font-size: 0 !important;
  line-height: 0 !important;
  color: transparent !important;
}
html[data-shiro-trans-mode="translated_only"] :is(td, th, caption)[data-shiro-has-translation="ready"] > :not([data-shiro-synthetic="translation"]) {
  display: none !important;
}
html[data-shiro-trans-mode="translated_only"] :is(td, th, caption)[data-shiro-has-translation="ready"] > [data-shiro-synthetic="translation"] {
  font-size: var(--shiro-body-size) !important;
  line-height: var(--shiro-body-line-height) !important;
}
html[data-shiro-trans-mode="translated_only"] [data-shiro-synthetic="translation"] {
  display: block;
  font-family: var(--shiro-trans-font) !important;
  color: var(--shiro-body-fg) !important;
  break-inside: auto;
}
html:not([data-shiro-trans-mode="original"]) li[data-shiro-synthetic="translation"] {
  display: block !important;
  list-style: none !important;
}`)

  return {
    surface: profile.appearance.surface,
    isDarkSurface: palette.isDark,
    surfaceBackgroundHex: palette.backgroundHex,
    viewportCssVars,
    viewportPaddingInlinePx,
    paginatorAttributes,
    showPageNumber: profile.behavior.showPageNumber,
    seamlessScroll:
      !isFixedLayout &&
      flow === "scrolled" &&
      profile.behavior.seamlessScroll,
    chapterCss: cssParts.join("\n\n"),
    customFontFace,
    usesFontFaceFallback: Boolean(customFontFace && useFontFaceFallback),
  }
}
