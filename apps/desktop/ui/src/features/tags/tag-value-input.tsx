import { Input } from "@/components/ui/input"
import { NativeSelect } from "@/components/ui/native-select"
import { useTranslation } from "@/lib/i18n"
import {
  getValueType,
  isValueValid,
  LANGUAGE_OPTIONS,
} from "@/lib/tag-value-patterns"
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

  if (valueType === "language") {
    return (
      <NativeSelect
        value={value}
        disabled={disabled}
        required
        aria-label={t("local.tag.value")}
        aria-invalid={!isValueValid(tag, value)}
        className="h-7 w-40 shrink-0"
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("local.tag.language_placeholder")}</option>
        {LANGUAGE_OPTIONS.map(({ code, label }) => (
          <option key={code} value={code}>
            {label}
          </option>
        ))}
      </NativeSelect>
    )
  }

  return (
    <Input
      // The native date picker yields YYYY-MM-DD, which is what the date regex expects.
      type={isDate ? "date" : "text"}
      inputMode={valueType === "number" ? "decimal" : undefined}
      value={value}
      disabled={disabled}
      required
      aria-label={t("local.tag.value")}
      aria-invalid={!isValueValid(tag, value)}
      placeholder={isDate ? undefined : t("local.tag.value_placeholder")}
      className={cn("h-7 shrink-0", isDate ? "w-40" : "w-32")}
      onChange={(event) => onChange(event.target.value)}
    />
  )
}
