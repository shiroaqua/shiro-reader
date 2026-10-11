import { useSyncExternalStore } from "react"
import { en } from "./locales/en"
import { zh } from "./locales/zh"

export type Locale = "zh" | "en"
export type TranslationKey = keyof typeof zh

export function detectLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    const base = language.toLowerCase().split(/[-_]/)[0]
    if (base === "zh" || base === "en") return base
  }
  return "en"
}

function getLocale(): Locale {
  return typeof navigator === "undefined"
    ? "en"
    : detectLocale(navigator.languages?.length ? navigator.languages : [navigator.language])
}

export function translate(key: string, locale: Locale = getLocale()): string {
  const messages = locale === "zh" ? zh : en
  return Object.hasOwn(messages, key)
    ? messages[key as TranslationKey]
    : messages["common.error.unknown"]
}

function subscribe(listener: () => void) {
  window.addEventListener("languagechange", listener)
  return () => window.removeEventListener("languagechange", listener)
}

export function useTranslation() {
  const locale = useSyncExternalStore(subscribe, getLocale, () => "en" as const)
  return { locale, t: (key: string) => translate(key, locale) }
}
