import { describe, expect, it } from "vitest"
import { isTagColor, readableTextColor, tagNameColor, DEFAULT_TAG_COLOR } from "@/features/tags/tag-color-picker"

describe("tag-color utilities", () => {
  it.each<[string, boolean]>([
    ["#000000", true],
    ["#75a9cc", true],
    ["#ABCDEF", true],
    ["#12345", false],
    ["75a9cc", false],
    ["#gggggg", false],
    ["", false],
  ])("validates tag color %j -> %s", (color, expected) => {
    expect(isTagColor(color)).toBe(expected)
  })

  it("picks dark or light foreground text based on perceived luminance", () => {
    expect(readableTextColor("#000000")).toBe("#ffffff")
    expect(readableTextColor("#334455")).toBe("#ffffff")
    expect(readableTextColor("#ffffff")).toBe("#000000")
    expect(readableTextColor("#d8b45e")).toBe("#000000")
  })

  it("omits inline text color for the default tag color", () => {
    expect(tagNameColor(DEFAULT_TAG_COLOR)).toBeUndefined()
    expect(tagNameColor("#75a9cc")).toBe("#75a9cc")
  })
})
