import { Input } from "@/components/ui/input"
import { useTranslation } from "@/lib/i18n"
import { getValueType, isValueValid } from "@/lib/tag-value-patterns"
import type { Tag } from "@/lib/types"
import { cn } from "cn"

/** Value field of a bound tag; its control follows the tag's value type. */
export function TagValueInput({
  tag,
  value,
  disabled,
  onChange,
}: {
  tag: Pick<Tag, "allows_value" | "value_regex">
  value: string
  disabled: boolean
  onChange: (value: string) => void
}) {
  const { t } = useTranslation()
  const valueType = getValueType(tag)
  const isDate = valueType === "date"

  return (
    <Input
      // The native date picker yields YYYY-MM-DD, which is what the date regex expects.
      type={isDate ? "date" : "text"}
      inputMode={valueType === "number" ? "decimal" : undefined}
      value={value}
      disabled={disabled}
      required
      aria-label={t("tag.value")}
      aria-invalid={!isValueValid(tag, value)}
      placeholder={isDate ? undefined : t("tag.value_placeholder")}
      className={cn("h-7 shrink-0", isDate ? "w-40" : "w-32")}
      onChange={(event) => onChange(event.target.value)}
    />
  )
}
