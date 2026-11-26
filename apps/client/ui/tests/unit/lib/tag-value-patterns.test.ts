import { describe, expect, it } from "vitest"
import {
  LANGUAGE_OPTIONS,
  VALUE_TYPES,
  isValidTagName,
  isValueValid,
  valueTypeLabelKey,
  type ValueType,
} from "@shiro/ui-shared/lib/tag-value-patterns"

const FIXTURE_RULES: Record<
  ValueType,
  { value_type: ValueType; value_regex: string | null }
> = {
  none: { value_type: "none", value_regex: null },
  number: { value_type: "number", value_regex: "^-?[0-9]+([.][0-9]+)?$" },
  text: { value_type: "text", value_regex: null },
  date: { value_type: "date", value_regex: "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" },
  language: {
    value_type: "language",
    value_regex: `^(?:${LANGUAGE_OPTIONS.map(({ code }) => code).join("|")})$`,
  },
  issn_isbn: {
    value_type: "issn_isbn",
    value_regex:
      "^(?:97[89](-?[0-9]){10}|([0-9]-?){9}[0-9Xx]|[0-9]{4}-?[0-9]{3}[0-9Xx])$",
  },
  clc: { value_type: "clc", value_regex: "^[A-Z][A-Z0-9]*([.][A-Z0-9]+)*$" },
  ddc: { value_type: "ddc", value_regex: "^[0-9]{3}([.][0-9]+)*$" },
  lcc: {
    value_type: "lcc",
    value_regex: "^[A-Z]{1,3}[0-9]{0,4}([.][A-Z0-9]+)*$",
  },
}

describe("tag-value-patterns", () => {
  describe("value type labels", () => {
    it("provides an i18n label key for every ValueType", () => {
      for (const type of VALUE_TYPES) {
        expect(valueTypeLabelKey(type)).toBe(`local.tag.value_type.${type}`)
      }
    })
  })

  describe("isValidTagName", () => {
    it.each<[string, boolean]>([
      ["Author", true],
      ["tag.custom", true],
      ["科幻2026", true],
      ["with space", false],
      ["with-dash", false],
      ["tag_underscore", false],
      ["#hash", false],
      ["", false],
    ])("validates tag name %j -> %s", (name, expected) => {
      expect(isValidTagName(name)).toBe(expected)
    })
  })

  describe("isValueValid", () => {
    it.each<[ValueType, string, boolean]>([
      ["none", "", true],
      ["none", "anything", false],
      ["number", "42", true],
      ["number", "-3.14", true],
      ["number", "12.", false],
      ["number", "abc", false],
      ["text", "Any non-empty string", true],
      ["text", "", false],
      ["date", "2024-02-29", true],
      ["date", "2025-02-29", false],
      ["date", "2026-02-31", false],
      ["date", "2026/01/01", false],
      ["language", "zh-Hans", true],
      ["language", "en", true],
      ["language", "xx-invalid", false],
      ["issn_isbn", "978-0-306-40615-7", true],
      ["issn_isbn", "978-0-306-40615-8", false],
      ["issn_isbn", "0-8044-2957-X", true],
      ["issn_isbn", "0-8044-2957-1", false],
      ["issn_isbn", "0378-5955", true],
      ["issn_isbn", "2434-561X", true],
      ["issn_isbn", "0378-5950", false],
      ["clc", "TP312.8", true],
      ["clc", "tp312", false],
      ["ddc", "005.133", true],
      ["ddc", "05.1", false],
      ["lcc", "QA76.73.R8", true],
      ["lcc", "1234", false],
    ])("validates %s value %j -> %s", (type, value, expected) => {
      expect(isValueValid(FIXTURE_RULES[type], value)).toBe(expected)
    })

    it("enforces unanchored and anchored custom text regexes and ignores JS-incompatible regex syntax", () => {
      const unanchoredRule = {
        value_type: "text" as const,
        value_regex: "[A-Z]{3}",
      }
      expect(isValueValid(unanchoredRule, "ABC")).toBe(true)
      expect(isValueValid(unanchoredRule, "AB")).toBe(false)
      expect(isValueValid(unanchoredRule, "ABCD")).toBe(false)
      expect(isValueValid(unanchoredRule, "")).toBe(false)

      const anchoredRule = {
        value_type: "text" as const,
        value_regex: "^[A-Z]{2}$",
      }
      expect(isValueValid(anchoredRule, "AB")).toBe(true)
      expect(isValueValid(anchoredRule, "ABC")).toBe(false)

      const unsupportedSyntaxRule = {
        value_type: "text" as const,
        value_regex: "(?invalid",
      }
      expect(isValueValid(unsupportedSyntaxRule, "anything")).toBe(true)
      expect(isValueValid(unsupportedSyntaxRule, "")).toBe(false)
    })
  })
})
