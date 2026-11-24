import { describe, expect, it } from "vitest"
import {
  VALUE_TYPES,
  getValueType,
  isValidTagName,
  isValueValid,
  toValueRule,
  valueTypeLabelKey,
  type ValueType,
} from "@/lib/tag-value-patterns"

describe("tag-value-patterns", () => {
  describe("value type rule mapping", () => {
    it("round-trips every ValueType through toValueRule and getValueType", () => {
      for (const type of VALUE_TYPES) {
        const rule = toValueRule(type)
        expect(getValueType(rule)).toBe(type)
        expect(valueTypeLabelKey(type)).toBe(`local.tag.value_type.${type}`)
      }
    })

    it("maps unknown custom regexes to text", () => {
      expect(
        getValueType({ allows_value: true, value_regex: "^[A-Z]{2}$" })
      ).toBe("text")
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
      expect(isValueValid(toValueRule(type), value)).toBe(expected)
    })

    it("enforces valid custom text regexes and ignores JS-incompatible regex syntax", () => {
      const customRule = { allows_value: true, value_regex: "[A-Z]{3}" }
      expect(isValueValid(customRule, "ABC")).toBe(true)
      expect(isValueValid(customRule, "AB")).toBe(false)

      const unsupportedSyntaxRule = {
        allows_value: true,
        value_regex: "(?invalid",
      }
      expect(isValueValid(unsupportedSyntaxRule, "anything")).toBe(true)
    })
  })
})
