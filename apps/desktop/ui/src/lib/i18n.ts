import { useSyncExternalStore } from "react"
import { enLocal } from "./locales/en/local"
import { enTagNames } from "./locales/en/tags"
import { zhLocal } from "./locales/zh/local"
import { zhTagNames } from "./locales/zh/tags"

export type Locale = "zh" | "en"
export type LocalTranslationKey = keyof typeof zhLocal
export type TranslationMessage =
  | { source: "local"; key: LocalTranslationKey }
  | { source: "rust"; key: string; message: string; errorId: string }

let overrideLocale: Locale | null = null
const localeListeners = new Set<() => void>()

export function detectLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    const base = language.toLowerCase().split(/[-_]/)[0]
    if (base === "zh" || base === "en") return base
  }
  return "en"
}

function getBrowserLocale(): Locale {
  return typeof navigator === "undefined"
    ? "en"
    : detectLocale(
        navigator.languages?.length ? navigator.languages : [navigator.language]
      )
}

export function getLocale(): Locale {
  return overrideLocale ?? getBrowserLocale()
}

export function setAppLocaleOverride(locale: Locale | null) {
  if (overrideLocale === locale) return
  overrideLocale = locale
  localeListeners.forEach((listener) => listener())
}

export function translateLocal(
  key: LocalTranslationKey,
  locale: Locale = getLocale()
): string {
  const messages = locale === "zh" ? zhLocal : enLocal
  return Object.hasOwn(messages, key)
    ? messages[key]
    : messages["local.common.error.unknown"]
}

export function translateTagName(
  name: string,
  locale: Locale = getLocale()
): string {
  if (!name.includes(".")) return name
  const messages = locale === "zh" ? zhTagNames : enTagNames
  return Object.hasOwn(messages, name) ? messages[name] : name
}

export function translateMessage(
  message: TranslationMessage,
  locale: Locale = getLocale()
): string {
  if (message.source === "rust") return message.message
  return translateLocal(message.key, locale)
}

function subscribe(listener: () => void) {
  localeListeners.add(listener)
  window.addEventListener("languagechange", listener)
  return () => {
    localeListeners.delete(listener)
    window.removeEventListener("languagechange", listener)
  }
}

export function formatCountMessage(
  count: number,
  singularKey: LocalTranslationKey,
  pluralKey: LocalTranslationKey,
  t: (key: LocalTranslationKey) => string
): string {
  return t(count === 1 ? singularKey : pluralKey).replace(
    "{count}",
    String(count)
  )
}

export function useTranslation() {
  const locale = useSyncExternalStore(subscribe, getLocale, () => "en" as const)
  return {
    locale,
    t: (key: LocalTranslationKey) => translateLocal(key, locale),
  }
}
