import { describe, expect, it } from "vitest"
import { formatBookDate } from "@/lib/format"
import {
  detectLocale,
  formatCountMessage,
  getLocale,
  setAppLocaleOverride,
  translateLocal,
  translateMessage,
  translateTagName,
  type Locale,
} from "@/lib/i18n"
import { enLocal } from "@/lib/locales/en/local"
import { enTagNames } from "@/lib/locales/en/tags"
import { zhLocal } from "@/lib/locales/zh/local"
import { zhTagNames } from "@/lib/locales/zh/tags"

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
    it("translates local keys, dotted built-in tags, Rust errors, and count messages", () => {
      expect(translateLocal("local.dashboard.bookshelf", "en")).toBe("Bookshelf")
      expect(translateLocal("local.dashboard.bookshelf", "zh")).toBe("书架")

      expect(translateTagName("tag.author", "en")).toBe("Author")
      expect(translateTagName("tag.author", "zh")).toBe("作者")
      expect(translateTagName("CustomTag", "zh")).toBe("CustomTag")
      expect(translateTagName("tag.unknown", "en")).toBe("tag.unknown")

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
})
