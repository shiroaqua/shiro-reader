import { describe, expect, it } from "vitest"
import {
  clearBookPatchField,
  countBookOverrides,
  hasBookOverrides,
  isFieldOverridden,
  mergeBookPatchField,
  pruneEmptyPatch,
  resolveEffectiveProfile,
} from "@shiro/ui-shared/features/reader/settings/cascade"
import { getDefaultReaderProfile } from "@shiro/ui-shared/features/reader/settings/defaults"

describe("two-tier reader settings cascade", () => {
  it("inherits un-overridden global updates while isolating single-book field overrides", () => {
    const baseGlobal = getDefaultReaderProfile(false)

    // Override fontSizePx and surface for book A
    let patch = mergeBookPatchField(undefined, "typography", "fontSizePx", 24)
    patch = mergeBookPatchField(patch, "appearance", "surface", "sepia_warm")

    expect(hasBookOverrides(patch)).toBe(true)
    expect(countBookOverrides(patch)).toBe(2)
    expect(isFieldOverridden(patch, "typography", "fontSizePx")).toBe(true)
    expect(isFieldOverridden(patch, "typography", "lineHeight")).toBe(false)

    // Global changes lineHeight -> book A automatically inherits new lineHeight while keeping fontSizePx=24
    const updatedGlobal = {
      ...baseGlobal,
      typography: {
        ...baseGlobal.typography,
        fontSizePx: 16,
        lineHeight: 2.1,
      },
      appearance: {
        surface: "night_dark" as const,
      },
    }

    const effective = resolveEffectiveProfile(updatedGlobal, patch)
    expect(effective.typography.fontSizePx).toBe(24)
    expect(effective.typography.lineHeight).toBe(2.1)
    expect(effective.appearance.surface).toBe("sepia_warm")
  })

  it("clears individual fields and prunes empty book patches back to full global inheritance", () => {
    let patch = mergeBookPatchField(undefined, "typography", "fontSizePx", 22)
    patch = mergeBookPatchField(patch, "layout", "columnCount", 1)

    const afterClearOne = clearBookPatchField(patch, "typography", "fontSizePx")
    expect(afterClearOne).not.toBeNull()
    expect(countBookOverrides(afterClearOne!)).toBe(1)
    expect(isFieldOverridden(afterClearOne!, "typography", "fontSizePx")).toBe(
      false
    )
    expect(isFieldOverridden(afterClearOne!, "layout", "columnCount")).toBe(
      true
    )

    const afterClearAll = clearBookPatchField(
      afterClearOne!,
      "layout",
      "columnCount"
    )
    expect(afterClearAll).toBeNull()
    expect(hasBookOverrides(afterClearAll ?? undefined)).toBe(false)
    expect(pruneEmptyPatch({ typography: {}, layout: {} })).toBeNull()
  })
})
