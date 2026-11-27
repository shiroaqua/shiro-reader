import { getLocale, type Locale } from "@shiro/ui-shared/lib/i18n"

const DEFAULT_REGIONAL_LOCALE: Record<Locale, string> = {
  zh: "zh-CN",
  en: "en-US",
}

const dateTimeFormatterCache = new Map<string, Intl.DateTimeFormat>()

type PublishedDatePrecision = "year" | "year-month" | "full"

const publishedDateFormatterCache = new Map<string, Intl.DateTimeFormat>()

const PUBLISHED_DATE_FORMAT_OPTIONS: Record<
  PublishedDatePrecision,
  Intl.DateTimeFormatOptions
> = {
  full: { dateStyle: "medium", timeZone: "UTC" },
  "year-month": { year: "numeric", month: "short", timeZone: "UTC" },
  year: { year: "numeric", timeZone: "UTC" },
}

function getNavigatorLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return []
  if (navigator.languages && navigator.languages.length > 0) {
    return navigator.languages
  }
  return navigator.language ? [navigator.language] : []
}

/** Resolves a region-qualified BCP 47 tag for the active app language. */
export function resolveRegionalLocale(
  locale: Locale,
  preferredLanguages?: readonly string[]
): string {
  const candidates = preferredLanguages ?? getNavigatorLanguages()
  for (const candidate of candidates) {
    const normalized = candidate.trim().replaceAll("_", "-")
    if (!normalized) continue
    const parts = normalized.split("-").filter((part) => part.length > 0)
    if (parts.length < 2) continue
    if (parts[0]?.toLowerCase() !== locale) continue
    try {
      const [canonical] = Intl.getCanonicalLocales(normalized)
      if (canonical) return canonical
    } catch {
      // Ignore malformed browser locale tags.
    }
  }
  return DEFAULT_REGIONAL_LOCALE[locale]
}

function getDateTimeFormatter(regionalLocale: string): Intl.DateTimeFormat {
  let formatter = dateTimeFormatterCache.get(regionalLocale)
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(regionalLocale, {
      dateStyle: "medium",
      timeStyle: "medium",
    })
    dateTimeFormatterCache.set(regionalLocale, formatter)
  }
  return formatter
}

function getPublishedDateFormatter(
  precision: PublishedDatePrecision,
  regionalLocale: string
): Intl.DateTimeFormat {
  const key = `${precision}:${regionalLocale}`
  let formatter = publishedDateFormatterCache.get(key)
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(
      regionalLocale,
      PUBLISHED_DATE_FORMAT_OPTIONS[precision]
    )
    publishedDateFormatterCache.set(key, formatter)
  }
  return formatter
}

function makeUtcCalendarDate(
  year: number,
  monthIndex: number,
  day: number
): Date {
  const date = new Date(0)
  date.setUTCFullYear(year, monthIndex, day)
  return date
}

// Formats an ISO timestamp as YYYY/MM/DD in local time, regardless of locale.
export function formatBookDate(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ""
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${date.getFullYear()}/${month}/${day}`
}

/** Formats an ISO timestamp with date, hours, minutes, and seconds per regional locale conventions. */
export function formatBookDateTime(
  value: string,
  locale: Locale = getLocale(),
  preferredLanguages?: readonly string[]
): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ""
  const regionalLocale = resolveRegionalLocale(locale, preferredLanguages)
  return getDateTimeFormatter(regionalLocale).format(date)
}

/** Formats a stored ISO published date (`YYYY`, `YYYY-MM`, or `YYYY-MM-DD`) per regional locale date conventions without time. */
export function formatBookPublishedDate(
  value: string | null | undefined,
  locale: Locale = getLocale(),
  preferredLanguages?: readonly string[]
): string | null {
  if (value == null) return null
  const trimmed = value.trim()
  if (trimmed === "") return null

  const regionalLocale = resolveRegionalLocale(locale, preferredLanguages)

  const fullMatch = /^(\d{4})-(\d{2})-(\d{2})$/.exec(trimmed)
  if (fullMatch) {
    const [, yearStr = "", monthStr = "", dayStr = ""] = fullMatch
    const year = Number.parseInt(yearStr, 10)
    const month = Number.parseInt(monthStr, 10)
    const day = Number.parseInt(dayStr, 10)
    if (year >= 1 && month >= 1 && month <= 12 && day >= 1 && day <= 31) {
      const date = makeUtcCalendarDate(year, month - 1, day)
      if (
        date.getUTCFullYear() === year &&
        date.getUTCMonth() === month - 1 &&
        date.getUTCDate() === day
      ) {
        return getPublishedDateFormatter("full", regionalLocale).format(date)
      }
    }
    return trimmed
  }

  const yearMonthMatch = /^(\d{4})-(\d{2})$/.exec(trimmed)
  if (yearMonthMatch) {
    const [, yearStr = "", monthStr = ""] = yearMonthMatch
    const year = Number.parseInt(yearStr, 10)
    const month = Number.parseInt(monthStr, 10)
    if (year >= 1 && month >= 1 && month <= 12) {
      const date = makeUtcCalendarDate(year, month - 1, 1)
      return getPublishedDateFormatter("year-month", regionalLocale).format(
        date
      )
    }
    return trimmed
  }

  const yearMatch = /^(\d{4})$/.exec(trimmed)
  if (yearMatch) {
    const [, yearStr = ""] = yearMatch
    const year = Number.parseInt(yearStr, 10)
    if (year >= 1) {
      const date = makeUtcCalendarDate(year, 0, 1)
      return getPublishedDateFormatter("year", regionalLocale).format(date)
    }
    return trimmed
  }

  return trimmed
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const

export function formatByteSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B"
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  const unit = BYTE_UNITS[unitIndex] ?? "B"
  if (unitIndex === 0) return `${Math.round(value)} ${unit}`
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`
}

export function formatEtaSeconds(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0s"
  const rounded = Math.ceil(seconds)
  if (rounded < 60) return `${rounded}s`
  const minutes = Math.floor(rounded / 60)
  const remSeconds = rounded % 60
  if (minutes < 60) {
    return remSeconds > 0 ? `${minutes}m ${remSeconds}s` : `${minutes}m`
  }
  const hours = Math.floor(minutes / 60)
  const remMinutes = minutes % 60
  return remMinutes > 0 ? `${hours}h ${remMinutes}m` : `${hours}h`
}
