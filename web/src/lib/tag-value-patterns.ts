import type { TranslationKey } from "@/lib/i18n"
import type { Tag } from "@/lib/types"

/**
 * The server only knows "allows a value" plus an optional regex, so value types
 * are a frontend convention: every type owns a fixed regex, and a tag is mapped
 * back to its type by comparing its stored regex with these.
 */
export type ValueType = "none" | "number" | "text" | "date"

type ValueRule = Pick<Tag, "allows_value" | "value_regex">

/** Order of the options in the value type select. */
export const VALUE_TYPES: readonly ValueType[] = [
  "none",
  "number",
  "text",
  "date",
]

const VALUE_TYPE_SPECS: Record<
  ValueType,
  {
    labelKey: TranslationKey
    /** `null` accepts any value. */
    regex: string | null
  }
> = {
  none: { labelKey: "tag.value_type.none", regex: null },
  number: {
    labelKey: "tag.value_type.number",
    regex: "^-?[0-9]+([.][0-9]+)?$",
  },
  text: { labelKey: "tag.value_type.text", regex: null },
  date: {
    labelKey: "tag.value_type.date",
    regex: "^[0-9]{4}-[0-9]{2}-[0-9]{2}$",
  },
}

export function valueTypeLabelKey(type: ValueType): TranslationKey {
  return VALUE_TYPE_SPECS[type].labelKey
}

/** The fields the server stores for a tag of the given value type. */
export function toValueRule(type: ValueType): ValueRule {
  return {
    allows_value: type !== "none",
    value_regex: VALUE_TYPE_SPECS[type].regex,
  }
}

/** The value type of a stored tag; an unrecognised regex is treated as text. */
export function getValueType(rule: ValueRule): ValueType {
  if (!rule.allows_value) return "none"
  if (rule.value_regex === VALUE_TYPE_SPECS.number.regex) return "number"
  if (rule.value_regex === VALUE_TYPE_SPECS.date.regex) return "date"
  return "text"
}

/** Client-side check; the server still validates the regex on its own. */
export function isValueValid(rule: ValueRule, value: string): boolean {
  const type = getValueType(rule)
  if (type === "none") return value === ""
  if (value === "") return false
  const { regex } = VALUE_TYPE_SPECS[type]
  if (regex !== null && !new RegExp(regex).test(value)) return false
  return type !== "date" || isCalendarDate(value)
}

/** Rejects dates that match the pattern but do not exist, such as 2026-02-31. */
function isCalendarDate(value: string): boolean {
  const [year, month, day] = value.split("-").map(Number)
  const date = new Date(0)
  date.setUTCFullYear(year, month - 1, day)
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() === month - 1 &&
    date.getUTCDate() === day
  )
}
