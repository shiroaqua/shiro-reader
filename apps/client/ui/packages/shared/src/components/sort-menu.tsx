import { ArrowDownWideNarrow, ArrowUpNarrowWide } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@shiro/ui-shared/components/ui/dropdown-menu"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

export type SortOrder = "asc" | "desc"

/** Sort field and direction picker shared by the list pages. */
export function SortMenu<Field extends string>({
  fields,
  sort,
  onChange,
}: {
  fields: readonly { value: Field; label: string }[]
  sort: { field: Field; order: SortOrder }
  onChange: (sort: { field: Field; order: SortOrder }) => void
}) {
  const { t } = useTranslation()
  const Icon = sort.order === "asc" ? ArrowUpNarrowWide : ArrowDownWideNarrow
  const current = fields.find((field) => field.value === sort.field)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button type="button" variant="outline" size="sm" />}
      >
        <Icon aria-hidden="true" />
        {current?.label}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" aria-label={t("local.book.sort")}>
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("local.book.sort.by")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.field}
            onValueChange={(field: Field) => onChange({ ...sort, field })}
          >
            {fields.map((field) => (
              <DropdownMenuRadioItem key={field.value} value={field.value}>
                {field.label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("local.book.sort.order")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort.order}
            onValueChange={(order: SortOrder) => onChange({ ...sort, order })}
          >
            <DropdownMenuRadioItem value="asc">
              {t("local.book.sort.asc")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="desc">
              {t("local.book.sort.desc")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
