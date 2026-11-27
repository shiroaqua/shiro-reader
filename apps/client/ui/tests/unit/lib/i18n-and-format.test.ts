import { describe, expect, it } from "vitest"
import {
  formatBookDate,
  formatBookDateTime,
  formatBookPublishedDate,
  resolveRegionalLocale,
} from "@shiro/ui-shared/lib/format"
import {
  detectLocale,
  formatCountMessage,
  getLocale,
  setAppLocaleOverride,
  translateLocal,
  translateMessage,
  translateTagName,
  type Locale,
} from "@shiro/ui-shared/lib/i18n"
import { enLocal } from "@shiro/ui-shared/lib/locales/en/local"
import { enTagNames } from "@shiro/ui-shared/lib/locales/en/tags"
import { zhLocal } from "@shiro/ui-shared/lib/locales/zh/local"
import { zhTagNames } from "@shiro/ui-shared/lib/locales/zh/tags"

function extractPlaceholders(text: string): string[] {
  return [...text.matchAll(/\{[a-zA-Z0-9_]+\}/g)]
    .map((match) => match[0])
    .sort()
}

describe("i18n and format", () => {
  describe("locale detection and override", () => {
    it.each<[readonly string[], Locale]>([
      [["zh-CN", "en-US"], "zh"],
      [["zh_TW"], "zh"],
      [["en-GB"], "en"],
      [["fr-FR", "zh-Hans"], "zh"],
      [["ja-JP", "de-DE"], "en"],
      [[], "en"],
    ])("detects %j as %s", (languages, expected) => {
      expect(detectLocale(languages)).toBe(expected)
    })

    it("prefers explicit app locale override and falls back when cleared", () => {
      setAppLocaleOverride("zh")
      expect(getLocale()).toBe("zh")

      setAppLocaleOverride("en")
      expect(getLocale()).toBe("en")

      setAppLocaleOverride(null)
      expect(["zh", "en"]).toContain(getLocale())
    })
  })

  describe("translation helpers", () => {
    it("translates local keys, dotted tag keys, Rust errors, and count messages", () => {
      expect(translateLocal("local.dashboard.bookshelf", "en")).toBe("Bookshelf")
      expect(translateLocal("local.dashboard.bookshelf", "zh")).toBe("书架")

      expect(translateTagName("CustomTag", "zh")).toBe("CustomTag")
      expect(translateTagName("tag.unknown", "en")).toBe("tag.unknown")

      enTagNames["tag.sample"] = "Sample"
      zhTagNames["tag.sample"] = "示例"
      try {
        expect(translateTagName("tag.sample", "en")).toBe("Sample")
        expect(translateTagName("tag.sample", "zh")).toBe("示例")
      } finally {
        delete enTagNames["tag.sample"]
        delete zhTagNames["tag.sample"]
      }

      expect(
        translateMessage(
          { source: "rust", key: "err", message: "Backend failed", errorId: "e1" },
          "en"
        )
      ).toBe("Backend failed")

      const t = (key: keyof typeof zhLocal) => translateLocal(key, "en")
      expect(
        formatCountMessage(1, "local.book.count_one", "local.book.count", t)
      ).toBe("1 book")
      expect(
        formatCountMessage(3, "local.book.count_one", "local.book.count", t)
      ).toBe("3 books")
    })

    it("keeps zh and en local and tag catalogs in exact key and placeholder parity", () => {
      const zhKeys = Object.keys(zhLocal).sort()
      const enKeys = Object.keys(enLocal).sort()
      expect(enKeys).toEqual(zhKeys)

      for (const key of zhKeys as (keyof typeof zhLocal)[]) {
        expect(extractPlaceholders(enLocal[key]), `placeholders for ${key}`).toEqual(
          extractPlaceholders(zhLocal[key])
        )
      }

      expect(enTagNames).toEqual({})
      expect(zhTagNames).toEqual({})
      expect(Object.keys(enTagNames).sort()).toEqual(
        Object.keys(zhTagNames).sort()
      )
    })
  })

  describe("formatBookDate", () => {
    it("formats valid timestamps as local YYYY/MM/DD and returns empty string for invalid inputs", () => {
      const date = new Date(2026, 2, 9, 14, 30, 0)
      expect(formatBookDate(date.toISOString())).toBe("2026/03/09")
      expect(formatBookDate("not-a-date")).toBe("")
    })
  })

  describe("resolveRegionalLocale & formatBookDateTime", () => {
    it("resolves regional BCP 47 tag matching the active language or falls back to default country", () => {
      expect(resolveRegionalLocale("zh", ["zh_CN", "en-US"])).toBe("zh-CN")
      expect(resolveRegionalLocale("zh", ["zh-TW"])).toBe("zh-TW")
      expect(resolveRegionalLocale("zh", ["en-US", "fr-FR"])).toBe("zh-CN")
      expect(resolveRegionalLocale("en", ["en-GB", "zh-CN"])).toBe("en-GB")
      expect(resolveRegionalLocale("en", ["zh-CN", "en"])).toBe("en-US")
      expect(resolveRegionalLocale("en", ["not_a_locale", "en-AU"])).toBe("en-AU")
    })

    it("formats date with minutes and seconds according to language and country conventions", () => {
      const morning = new Date(2026, 0, 2, 2, 11, 5)
      const afternoon = new Date(2026, 0, 2, 14, 11, 5)

      expect(formatBookDateTime(morning.toISOString(), "zh", [])).toBe(
        "2026年1月2日 02:11:05"
      )
      expect(formatBookDateTime(afternoon.toISOString(), "zh", ["zh-CN"])).toBe(
        "2026年1月2日 14:11:05"
      )

      const enUsExpected = new Intl.DateTimeFormat("en-US", {
        dateStyle: "medium",
        timeStyle: "medium",
      }).format(morning)
      expect(formatBookDateTime(morning.toISOString(), "en", ["en-US"])).toBe(
        enUsExpected
      )
      expect(enUsExpected).toContain(":11:05")

      const enGbExpected = new Intl.DateTimeFormat("en-GB", {
        dateStyle: "medium",
        timeStyle: "medium",
      }).format(morning)
      expect(formatBookDateTime(morning.toISOString(), "en", ["en-GB"])).toBe(
        enGbExpected
      )
      expect(enGbExpected).toContain("02:11:05")

      expect(formatBookDateTime("not-a-date", "zh", ["zh-CN"])).toBe("")
    })
  })

  describe("formatBookPublishedDate", () => {
    it("formats full and partial ISO dates per regional locale conventions without time", () => {
      expect(formatBookPublishedDate("2021-09-27", "zh", [])).toBe(
        "2021年9月27日"
      )
      expect(formatBookPublishedDate("2021-09-27", "zh", ["zh-TW"])).toBe(
        "2021年9月27日"
      )

      const enUsDate = new Date(0)
      enUsDate.setUTCFullYear(2021, 8, 27)
      const enUsExpected = new Intl.DateTimeFormat("en-US", {
        dateStyle: "medium",
        timeZone: "UTC",
      }).format(enUsDate)
      const enUsFormatted = formatBookPublishedDate("2021-09-27", "en", [
        "en-US",
      ])
      expect(enUsFormatted).toBe(enUsExpected)
      expect(enUsFormatted).not.toContain(":")

      const enGbExpected = new Intl.DateTimeFormat("en-GB", {
        dateStyle: "medium",
        timeZone: "UTC",
      }).format(enUsDate)
      const enGbFormatted = formatBookPublishedDate("2021-09-27", "en", [
        "en-GB",
      ])
      expect(enGbFormatted).toBe(enGbExpected)
      expect(enGbFormatted).not.toContain(":")

      expect(formatBookPublishedDate("2024-05", "zh", ["zh-CN"])).toBe(
        "2024年5月"
      )
      expect(formatBookPublishedDate("2024-05", "en", ["en-US"])).toBe(
        "May 2024"
      )
      expect(formatBookPublishedDate("2024-05", "en", ["en-GB"])).toBe(
        "May 2024"
      )

      expect(formatBookPublishedDate("2024", "zh", ["zh-CN"])).toBe("2024年")
      expect(formatBookPublishedDate("2024", "en", ["en-US"])).toBe("2024")

      expect(formatBookPublishedDate("0050", "zh", ["zh-CN"])).toBe("50年")
      expect(formatBookPublishedDate("0050", "en", ["en-US"])).toBe("50")
    })

    it("returns null for empty inputs and falls back to trimmed raw string for invalid dates", () => {
      expect(formatBookPublishedDate(null, "en", ["en-US"])).toBeNull()
      expect(formatBookPublishedDate(undefined, "en", ["en-US"])).toBeNull()
      expect(formatBookPublishedDate("   ", "en", ["en-US"])).toBeNull()
      expect(formatBookPublishedDate(" 2021-02-30 ", "en", ["en-US"])).toBe(
        "2021-02-30"
      )
      expect(formatBookPublishedDate("not-a-date", "zh", ["zh-CN"])).toBe(
        "not-a-date"
      )
    })
  })
})
