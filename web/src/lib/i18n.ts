import { useSyncExternalStore } from "react"
import { enLocal } from "./locales/en/local"
import { enServer } from "./locales/en/server"
import { enTagNames } from "./locales/en/tags"
import { zhLocal } from "./locales/zh/local"
import { zhServer } from "./locales/zh/server"
import { zhTagNames } from "./locales/zh/tags"

export type Locale = "zh" | "en"
export type LocalTranslationKey = keyof typeof zhLocal
export type TranslationMessage =
  | { source: "local"; key: LocalTranslationKey }
  | { source: "server"; key: string }

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
    : detectLocale(
        navigator.languages?.length ? navigator.languages : [navigator.language]
      )
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

export function translateServerMessage(
  key: string,
  locale: Locale = getLocale()
): string {
  const messages = locale === "zh" ? zhServer : enServer
  return Object.hasOwn(messages, key)
    ? messages[key as keyof typeof zhServer]
    : translateLocal("local.common.error.unknown", locale)
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
  return message.source === "local"
    ? translateLocal(message.key, locale)
    : translateServerMessage(message.key, locale)
}

function subscribe(listener: () => void) {
  window.addEventListener("languagechange", listener)
  return () => window.removeEventListener("languagechange", listener)
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
