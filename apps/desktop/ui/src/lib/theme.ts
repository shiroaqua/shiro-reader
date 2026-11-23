import type { LocalTranslationKey } from "@/lib/i18n"

/** Color themes; must stay in sync with Rust `AppTheme` and `[data-theme]` in index.css. */
export const APP_THEMES = ["default", "sakura", "mint", "sky", "peach"] as const

export type AppTheme = (typeof APP_THEMES)[number]

export const DEFAULT_APP_THEME: AppTheme = "default"

export const APP_THEME_LABEL_KEYS = {
  default: "local.theme.default",
  sakura: "local.theme.sakura",
  mint: "local.theme.mint",
  sky: "local.theme.sky",
  peach: "local.theme.peach",
} as const satisfies Record<AppTheme, LocalTranslationKey>

/** Applies the theme to the document root so every CSS token resolves against it. */
export function applyAppTheme(theme: AppTheme) {
  if (typeof document === "undefined") return
  document.documentElement.dataset["theme"] = theme
}
