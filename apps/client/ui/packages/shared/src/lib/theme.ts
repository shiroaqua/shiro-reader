import type { LocalTranslationKey } from "@shiro/ui-shared/lib/i18n"

/** Color themes; must stay in sync with Rust `AppTheme` and `[data-theme]` in index.css. */
export const APP_THEMES = ["default", "pink", "green", "blue", "yellow"] as const

export type AppTheme = (typeof APP_THEMES)[number]

export const DEFAULT_APP_THEME: AppTheme = "default"

export const APP_THEME_LABEL_KEYS = {
  default: "local.theme.default",
  pink: "local.theme.pink",
  green: "local.theme.green",
  blue: "local.theme.blue",
  yellow: "local.theme.yellow",
} as const satisfies Record<AppTheme, LocalTranslationKey>

/** Applies the theme to the document root so every CSS token resolves against it. */
export function applyAppTheme(theme: AppTheme) {
  if (typeof document === "undefined") return
  document.documentElement.dataset["theme"] = theme
}
