import type { ReaderProfile, ReaderProfilePatch } from "../contracts/settings"

function mergeDefinedGroup<T extends object>(
  base: T,
  patch: Partial<T> | undefined
): T {
  if (!patch) return base
  const result = { ...base }
  for (const key of Object.keys(patch) as Array<keyof T>) {
    const val = patch[key]
    if (val !== undefined) {
      result[key] = val
    }
  }
  return result
}

function cleanPartialGroup<T extends object>(
  group: Partial<T> | undefined
): Partial<T> | undefined {
  if (!group) return undefined
  const next: Partial<T> = {}
  let hasKeys = false
  for (const key of Object.keys(group) as Array<keyof T>) {
    const val = group[key]
    if (val !== undefined) {
      next[key] = val
      hasKeys = true
    }
  }
  return hasKeys ? next : undefined
}

export function pruneEmptyPatch(
  patch: ReaderProfilePatch | undefined
): ReaderProfilePatch | null {
  if (!patch) return null
  const appearance = cleanPartialGroup(patch.appearance)
  const typography = cleanPartialGroup(patch.typography)
  const layout = cleanPartialGroup(patch.layout)
  const behavior = cleanPartialGroup(patch.behavior)

  const pruned: {
    appearance?: ReaderProfilePatch["appearance"]
    typography?: ReaderProfilePatch["typography"]
    layout?: ReaderProfilePatch["layout"]
    behavior?: ReaderProfilePatch["behavior"]
  } = {}

  if (appearance) pruned.appearance = appearance
  if (typography) pruned.typography = typography
  if (layout) pruned.layout = layout
  if (behavior) pruned.behavior = behavior

  return appearance || typography || layout || behavior ? pruned : null
}

export function resolveEffectiveProfile(
  global: ReaderProfile,
  bookPatch: ReaderProfilePatch | undefined
): ReaderProfile {
  return {
    appearance: mergeDefinedGroup(global.appearance, bookPatch?.appearance),
    typography: mergeDefinedGroup(global.typography, bookPatch?.typography),
    layout: mergeDefinedGroup(global.layout, bookPatch?.layout),
    behavior: mergeDefinedGroup(global.behavior, bookPatch?.behavior),
  }
}

export function countBookOverrides(
  bookPatch: ReaderProfilePatch | undefined
): number {
  if (!bookPatch) return 0
  let count = 0
  const groups = [
    bookPatch.appearance,
    bookPatch.typography,
    bookPatch.layout,
    bookPatch.behavior,
  ]
  for (const group of groups) {
    if (!group) continue
    for (const val of Object.values(group)) {
      if (val !== undefined) {
        count += 1
      }
    }
  }
  return count
}

export function hasBookOverrides(
  bookPatch: ReaderProfilePatch | undefined
): boolean {
  return countBookOverrides(bookPatch) > 0
}

export function isFieldOverridden<G extends keyof ReaderProfile>(
  bookPatch: ReaderProfilePatch | undefined,
  group: G,
  key: keyof ReaderProfile[G]
): boolean {
  const groupPatch = bookPatch?.[group] as
    | Partial<ReaderProfile[G]>
    | undefined
  return groupPatch !== undefined && groupPatch[key] !== undefined
}

export function mergeBookPatchField<
  G extends keyof ReaderProfile,
  K extends keyof ReaderProfile[G],
>(
  bookPatch: ReaderProfilePatch | undefined,
  group: G,
  key: K,
  value: ReaderProfile[G][K]
): ReaderProfilePatch {
  const currentGroup = (bookPatch?.[group] ?? {}) as Partial<ReaderProfile[G]>
  const nextGroup: Partial<ReaderProfile[G]> = {
    ...currentGroup,
    [key]: value,
  }
  const next: ReaderProfilePatch = {
    ...bookPatch,
    [group]: nextGroup,
  }
  return pruneEmptyPatch(next) ?? {}
}

export function clearBookPatchField<
  G extends keyof ReaderProfile,
  K extends keyof ReaderProfile[G],
>(
  bookPatch: ReaderProfilePatch | undefined,
  group: G,
  key: K
): ReaderProfilePatch | null {
  if (!bookPatch) return null
  const currentGroup = bookPatch[group] as Partial<ReaderProfile[G]> | undefined
  if (!currentGroup || currentGroup[key] === undefined) {
    return pruneEmptyPatch(bookPatch)
  }

  const nextGroup: Partial<ReaderProfile[G]> = { ...currentGroup }
  delete nextGroup[key]

  const next: ReaderProfilePatch = {
    ...bookPatch,
    [group]: nextGroup,
  }
  return pruneEmptyPatch(next)
}
