import type { AppTheme } from "@shiro/ui-shared/lib/theme"
import type {
  ReaderFontPreset,
  ReaderProfile,
  ReaderSurfacePreset,
} from "../contracts/settings"

export const READER_SURFACE_PRESETS: readonly ReaderSurfacePreset[] = [
  "follow_app",
  "paper_light",
  "sepia_warm",
  "eye_green",
  "night_dark",
  "amoled_black",
] as const

export const READER_SETTING_BOUNDS = {
  fontSizePx: { min: 12, max: 48, step: 1 },
  fontWeight: { min: 300, max: 700, step: 100 },
  lineHeight: { min: 1.2, max: 2.4, step: 0.05 },
  letterSpacingEm: { min: -0.05, max: 0.3, step: 0.01 },
  wordSpacingEm: { min: 0, max: 0.5, step: 0.02 },
  paragraphSpacingEm: { min: 0, max: 2.5, step: 0.05 },
  paragraphIndentEm: { min: 0, max: 4, step: 0.25 },
  pageMarginInlinePx: { min: 8, max: 120, step: 4 },
  pageMarginBlockPx: { min: 8, max: 96, step: 4 },
  maxInlineSizePx: { min: 480, max: 1400, step: 20 },
  columnGapPercent: { min: 3, max: 12, step: 1 },
} as const

export const READER_FONT_PRESET_STACKS: Readonly<
  Record<ReaderFontPreset, string>
> = {
  sans: `ui-sans-serif, system-ui, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif`,
  serif: `"Source Han Serif SC", "Noto Serif CJK SC", "Songti SC", "SimSun", Georgia, serif`,
  mono: `ui-monospace, "JetBrains Mono", "Cascadia Mono", "Source Han Sans HW SC", monospace`,
}

export interface ReaderSurfacePalette {
  readonly isDark: boolean
  readonly backgroundHex: string
  readonly foregroundHex: string
  readonly mutedHex: string
  readonly borderHex: string
  readonly chromeBgHex: string
  readonly chromeBorderHex: string
  readonly selectionBgHex: string
  readonly selectionFgHex: string
  readonly linkHex: string
}

const FOLLOW_APP_PALETTES: Readonly<Record<AppTheme, ReaderSurfacePalette>> = {
  default: {
    isDark: false,
    backgroundHex: "#fafafa",
    foregroundHex: "#1f1f1d",
    mutedHex: "#666664",
    borderHex: "#e4e4e2",
    chromeBgHex: "#f5f5f4",
    chromeBorderHex: "#e2e2e0",
    selectionBgHex: "rgba(37, 99, 235, 0.22)",
    selectionFgHex: "#111827",
    linkHex: "#2563eb",
  },
  pink: {
    isDark: false,
    backgroundHex: "#fff4fe",
    foregroundHex: "#311e2b",
    mutedHex: "#76626b",
    borderHex: "#efddf0",
    chromeBgHex: "#fdf0fb",
    chromeBorderHex: "#ecd7ed",
    selectionBgHex: "rgba(190, 24, 93, 0.18)",
    selectionFgHex: "#311e2b",
    linkHex: "#9d174d",
  },
  green: {
    isDark: false,
    backgroundHex: "#effff2",
    foregroundHex: "#1c2d23",
    mutedHex: "#5d7667",
    borderHex: "#d5ebd9",
    chromeBgHex: "#e6faea",
    chromeBorderHex: "#cde7d3",
    selectionBgHex: "rgba(21, 128, 61, 0.2)",
    selectionFgHex: "#1c2d23",
    linkHex: "#15803d",
  },
  blue: {
    isDark: false,
    backgroundHex: "#e8ffff",
    foregroundHex: "#1a2b30",
    mutedHex: "#5c757d",
    borderHex: "#cde8ec",
    chromeBgHex: "#dff9fb",
    chromeBorderHex: "#c4e4e8",
    selectionBgHex: "rgba(2, 132, 199, 0.2)",
    selectionFgHex: "#1a2b30",
    linkHex: "#0369a1",
  },
  yellow: {
    isDark: false,
    backgroundHex: "#fff8e9",
    foregroundHex: "#2f2718",
    mutedHex: "#796a48",
    borderHex: "#efe2c6",
    chromeBgHex: "#fdf3dc",
    chromeBorderHex: "#ebdcbc",
    selectionBgHex: "rgba(180, 83, 9, 0.2)",
    selectionFgHex: "#2f2718",
    linkHex: "#b45309",
  },
}

const FIXED_SURFACE_PALETTES: Readonly<
  Record<Exclude<ReaderSurfacePreset, "follow_app">, ReaderSurfacePalette>
> = {
  paper_light: {
    isDark: false,
    backgroundHex: "#fcfcfa",
    foregroundHex: "#1f1f1d",
    mutedHex: "#696864",
    borderHex: "#e6e4de",
    chromeBgHex: "#f6f5f0",
    chromeBorderHex: "#e2e0d8",
    selectionBgHex: "rgba(37, 99, 235, 0.2)",
    selectionFgHex: "#1f1f1d",
    linkHex: "#1d4ed8",
  },
  sepia_warm: {
    isDark: false,
    backgroundHex: "#f6efe2",
    foregroundHex: "#3a2f23",
    mutedHex: "#7b6853",
    borderHex: "#e5d7c1",
    chromeBgHex: "#efe5d4",
    chromeBorderHex: "#dfd0b7",
    selectionBgHex: "rgba(180, 83, 9, 0.22)",
    selectionFgHex: "#3a2f23",
    linkHex: "#9a3412",
  },
  eye_green: {
    isDark: false,
    backgroundHex: "#e7f2e8",
    foregroundHex: "#1f3024",
    mutedHex: "#536d5a",
    borderHex: "#cfe2d1",
    chromeBgHex: "#dcebdd",
    chromeBorderHex: "#c5dcc8",
    selectionBgHex: "rgba(22, 101, 52, 0.2)",
    selectionFgHex: "#1f3024",
    linkHex: "#166534",
  },
  night_dark: {
    isDark: true,
    backgroundHex: "#1c1d21",
    foregroundHex: "#d8dae0",
    mutedHex: "#9498a4",
    borderHex: "#32353f",
    chromeBgHex: "#23252b",
    chromeBorderHex: "#363944",
    selectionBgHex: "rgba(96, 165, 250, 0.3)",
    selectionFgHex: "#f8fafc",
    linkHex: "#7dd3fc",
  },
  amoled_black: {
    isDark: true,
    backgroundHex: "#000000",
    foregroundHex: "#c9ccd4",
    mutedHex: "#878b96",
    borderHex: "#24262b",
    chromeBgHex: "#0b0c0e",
    chromeBorderHex: "#222429",
    selectionBgHex: "rgba(96, 165, 250, 0.34)",
    selectionFgHex: "#ffffff",
    linkHex: "#93c5fd",
  },
}

export function resolveSurfacePalette(
  surface: ReaderSurfacePreset,
  appTheme: AppTheme
): ReaderSurfacePalette {
  if (surface === "follow_app") {
    return FOLLOW_APP_PALETTES[appTheme] ?? FOLLOW_APP_PALETTES.default
  }
  return FIXED_SURFACE_PALETTES[surface]
}

export function getDefaultReaderProfile(isMobile: boolean): ReaderProfile {
  return {
    appearance: {
      surface: "follow_app",
    },
    typography: {
      font: { kind: "publisher" },
      fontSizePx: 18,
      fontWeight: "auto",
      lineHeight: 1.75,
      letterSpacingEm: 0,
      wordSpacingEm: 0,
      textAlign: "auto",
      hyphenation: "auto",
      overridePublisherStyles: false,
    },
    layout: {
      paragraphSpacingEm: 0.75,
      paragraphIndentEm: "auto",
      pageMarginInlinePx: isMobile ? 20 : 48,
      pageMarginBlockPx: isMobile ? 20 : 36,
      maxInlineSizePx: 760,
      columnCount: isMobile ? 1 : "auto",
      columnGapPercent: 6,
    },
    behavior: {
      flow: "paginated",
      animatedPageTurn: true,
      tapZonesEnabled: false,
      showPageNumber: true,
      seamlessScroll: false,
    },
  }
}
