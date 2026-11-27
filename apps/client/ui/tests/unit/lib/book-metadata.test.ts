import { describe, expect, it } from "vitest"
import {
  formatBookAuthors,
  formatBookLanguage,
  isFullIsoDate,
  matchLanguageOption,
  normalizeAuthorInputs,
} from "@shiro/ui-shared/lib/book-metadata"

describe("book-metadata helpers", () => {
  describe("matchLanguageOption & formatBookLanguage", () => {
    it("matches exact codes regardless of case or underscore separators", () => {
      expect(matchLanguageOption("en")).toEqual({ code: "en", label: "English" })
      expect(matchLanguageOption("JA")).toEqual({ code: "ja", label: "日本語" })
      expect(matchLanguageOption("zh-hans")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })
      expect(matchLanguageOption("zh_hant")).toEqual({
        code: "zh-Hant",
        label: "繁體中文",
      })
    })

    it("fuzzy-matches Chinese regional and script tags to Simplified or Traditional", () => {
      expect(matchLanguageOption("zh")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })
      expect(matchLanguageOption("zh-CN")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })
      expect(matchLanguageOption("zh-Hans-CN")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })
      expect(matchLanguageOption("zh-SG")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })
      expect(matchLanguageOption("zho")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })

      expect(matchLanguageOption("zh-TW")).toEqual({
        code: "zh-Hant",
        label: "繁體中文",
      })
      expect(matchLanguageOption("zh-HK")).toEqual({
        code: "zh-Hant",
        label: "繁體中文",
      })
      expect(matchLanguageOption("zh-MO")).toEqual({
        code: "zh-Hant",
        label: "繁體中文",
      })
      expect(matchLanguageOption("zh-Hant-TW")).toEqual({
        code: "zh-Hant",
        label: "繁體中文",
      })
      expect(matchLanguageOption("yue")).toEqual({
        code: "zh-Hant",
        label: "繁體中文",
      })
      expect(matchLanguageOption("yue-Hans")).toEqual({
        code: "zh-Hans",
        label: "简体中文",
      })
    })

    it("fuzzy-matches regional tags, ISO 639-2/3 codes, and legacy aliases", () => {
      expect(matchLanguageOption("en-US")).toEqual({
        code: "en",
        label: "English",
      })
      expect(matchLanguageOption("ja-JP")).toEqual({
        code: "ja",
        label: "日本語",
      })
      expect(matchLanguageOption("es-419")).toEqual({
        code: "es",
        label: "español",
      })
      expect(matchLanguageOption("de-CH-1996")).toEqual({
        code: "de",
        label: "Deutsch",
      })
      expect(matchLanguageOption("eng")).toEqual({
        code: "en",
        label: "English",
      })
      expect(matchLanguageOption("jpn")).toEqual({
        code: "ja",
        label: "日本語",
      })
      expect(matchLanguageOption("fra")).toEqual({
        code: "fr",
        label: "français",
      })
      expect(matchLanguageOption("deu")).toEqual({
        code: "de",
        label: "Deutsch",
      })
      expect(matchLanguageOption("iw")).toEqual({
        code: "he",
        label: "עברית",
      })
      expect(matchLanguageOption("in")).toEqual({
        code: "id",
        label: "Bahasa Indonesia",
      })
    })

    it("returns null for empty or unknown tags and falls back to raw tag on format", () => {
      expect(matchLanguageOption(null)).toBeNull()
      expect(matchLanguageOption(undefined)).toBeNull()
      expect(matchLanguageOption("   ")).toBeNull()
      expect(matchLanguageOption("tlh")).toBeNull()

      expect(formatBookLanguage(null)).toBeNull()
      expect(formatBookLanguage("   ")).toBeNull()
      expect(formatBookLanguage("zh-TW")).toBe("繁體中文")
      expect(formatBookLanguage("en-GB")).toBe("English")
      expect(formatBookLanguage("tlh")).toBe("tlh")
    })
  })

  describe("isFullIsoDate", () => {
    it("accepts only YYYY-MM-DD full calendar strings", () => {
      expect(isFullIsoDate("2025-12-01")).toBe(true)
      expect(isFullIsoDate("2024-05")).toBe(false)
      expect(isFullIsoDate("2024")).toBe(false)
      expect(isFullIsoDate("")).toBe(false)
    })
  })

  describe("formatBookAuthors & normalizeAuthorInputs", () => {
    it("formats empty and multi-author lists according to locale", () => {
      expect(formatBookAuthors([], "en")).toBeNull()
      expect(formatBookAuthors([], "zh")).toBeNull()
      expect(formatBookAuthors(["Alice"], "en")).toBe("Alice")
      expect(formatBookAuthors(["Alice", "Bob"], "en")).toBe("Alice, Bob")
      expect(formatBookAuthors(["张三", "李四"], "zh")).toBe("张三、李四")
    })

    it("trims, drops empty entries, and deduplicates preserving order", () => {
      expect(
        normalizeAuthorInputs(["  Alice  ", "", "   ", "Bob", "Alice", "Carol "])
      ).toEqual(["Alice", "Bob", "Carol"])
    })
  })
})
