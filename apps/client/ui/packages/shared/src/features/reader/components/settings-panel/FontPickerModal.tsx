import type { CSSProperties } from "react"
import { useMemo, useRef, useState } from "react"
import { Check, Plus, Search, Trash2 } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@shiro/ui-shared/components/ui/dialog"
import { Input } from "@shiro/ui-shared/components/ui/input"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import type {
  CustomFontMeta,
  ReaderFontChoice,
  SystemFontFamily,
} from "../../contracts/settings"

const SYSTEM_LIST_ROW_HEIGHT = 34
const SYSTEM_LIST_VIEWPORT_HEIGHT = 204
const SYSTEM_LIST_OVERSCAN = 6

export interface FontPickerModalProps {
  readonly open: boolean
  readonly onOpenChange: (open: boolean) => void
  readonly value: ReaderFontChoice
  readonly customFonts: readonly CustomFontMeta[]
  readonly systemFonts: readonly SystemFontFamily[]
  readonly canEnumerateSystemFonts: boolean
  readonly onSelect: (choice: ReaderFontChoice) => void
  readonly onImportCustomFont: () => void
  readonly onDeleteCustomFont: (fontId: string) => void
  readonly surfaceCssVars?: Readonly<Record<string, string>> | undefined
  readonly isDarkSurface?: boolean | undefined
}

function isSameChoice(a: ReaderFontChoice, b: ReaderFontChoice): boolean {
  if (a.kind !== b.kind) return false
  switch (a.kind) {
    case "publisher":
      return true
    case "preset":
      return b.kind === "preset" && a.preset === b.preset
    case "system":
      return b.kind === "system" && a.family === b.family
    case "custom":
      return b.kind === "custom" && a.fontId === b.fontId
  }
}

export function FontPickerModal({
  open,
  onOpenChange,
  value,
  customFonts,
  systemFonts,
  canEnumerateSystemFonts,
  onSelect,
  onImportCustomFont,
  onDeleteCustomFont,
  surfaceCssVars,
  isDarkSurface,
}: FontPickerModalProps) {
  const { t } = useTranslation()
  const systemListRef = useRef<HTMLDivElement | null>(null)
  const [searchQuery, setSearchQuery] = useState("")
  const [scrollTop, setScrollTop] = useState(0)

  const filteredSystemFonts = useMemo(() => {
    const q = searchQuery.trim().toLowerCase()
    if (!q) return systemFonts
    return systemFonts.filter((font) =>
      font.family.toLowerCase().includes(q)
    )
  }, [systemFonts, searchQuery])

  const startIndex = Math.max(
    0,
    Math.floor(scrollTop / SYSTEM_LIST_ROW_HEIGHT) - SYSTEM_LIST_OVERSCAN
  )
  const visibleCount =
    Math.ceil(SYSTEM_LIST_VIEWPORT_HEIGHT / SYSTEM_LIST_ROW_HEIGHT) +
    SYSTEM_LIST_OVERSCAN * 2
  const endIndex = Math.min(filteredSystemFonts.length, startIndex + visibleCount)
  const visibleSystemFonts = filteredSystemFonts.slice(startIndex, endIndex)
  const totalHeight = filteredSystemFonts.length * SYSTEM_LIST_ROW_HEIGHT

  const chooseAndClose = (choice: ReaderFontChoice) => {
    onSelect(choice)
    onOpenChange(false)
  }

  const presetItems: Array<{
    readonly choice: ReaderFontChoice
    readonly label: string
  }> = [
    {
      choice: { kind: "publisher" },
      label: t("local.reader.settings.font.publisher"),
    },
    {
      choice: { kind: "preset", preset: "sans" },
      label: t("local.reader.settings.font.preset_sans"),
    },
    {
      choice: { kind: "preset", preset: "serif" },
      label: t("local.reader.settings.font.preset_serif"),
    },
    {
      choice: { kind: "preset", preset: "mono" },
      label: t("local.reader.settings.font.preset_mono"),
    },
  ]

  const dialogStyle: CSSProperties | undefined = surfaceCssVars
    ? {
        ...(surfaceCssVars as CSSProperties),
        colorScheme: isDarkSurface ? "dark" : "light",
      }
    : undefined

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        style={dialogStyle}
        data-dark-surface={
          isDarkSurface === undefined ? undefined : isDarkSurface ? "true" : "false"
        }
        className="flex max-h-[82dvh] flex-col gap-3 overflow-hidden sm:max-w-md"
      >
        <DialogHeader>
          <DialogTitle>{t("local.reader.settings.font.label")}</DialogTitle>
        </DialogHeader>

        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-0.5">
        <div className="flex shrink-0 flex-col gap-1">
          {presetItems.map((item) => {
            const selected = isSameChoice(value, item.choice)
            return (
              <button
                key={item.label}
                type="button"
                onClick={() => chooseAndClose(item.choice)}
                className={cn(
                  "flex items-center justify-between rounded-lg px-2.5 py-2 text-left text-xs transition-colors",
                  selected
                    ? "bg-primary/15 font-semibold"
                    : "hover:bg-muted/70"
                )}
              >
                <span>{item.label}</span>
                {selected && <Check className="size-3.5" aria-hidden="true" />}
              </button>
            )
          })}
        </div>

        <div className="flex flex-col gap-1.5 border-t pt-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-muted-foreground">
              {t("local.reader.settings.font.custom_section")}
            </span>
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={onImportCustomFont}
            >
              <Plus className="size-3" aria-hidden="true" />
              {t("local.reader.settings.font.import")}
            </Button>
          </div>

          {customFonts.map((font) => {
            const selected = isSameChoice(value, {
              kind: "custom",
              fontId: font.fontId,
            })
            return (
              <div key={font.fontId} className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() =>
                    chooseAndClose({ kind: "custom", fontId: font.fontId })
                  }
                  className={cn(
                    "flex min-w-0 flex-1 items-center justify-between rounded-lg px-2.5 py-1.5 text-left text-xs transition-colors",
                    selected
                      ? "bg-primary/15 font-semibold"
                      : "hover:bg-muted/70"
                  )}
                >
                  <span className="truncate">{font.familyName}</span>
                  {selected && (
                    <Check className="size-3.5 shrink-0" aria-hidden="true" />
                  )}
                </button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`${t("local.reader.settings.font.delete")} ${font.familyName}`}
                  onClick={() => onDeleteCustomFont(font.fontId)}
                >
                  <Trash2 className="size-3.5" aria-hidden="true" />
                </Button>
              </div>
            )
          })}
        </div>

        {canEnumerateSystemFonts && (
          <div className="flex flex-col gap-2 border-t pt-2">
            <span className="text-xs font-medium text-muted-foreground">
              {t("local.reader.settings.font.system_section")}
            </span>
            <div className="relative">
              <Search
                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                aria-hidden="true"
              />
              <Input
                value={searchQuery}
                aria-label={t("local.reader.settings.font.search_placeholder")}
                onChange={(event) => {
                  setSearchQuery(event.target.value)
                  setScrollTop(0)
                  if (systemListRef.current) {
                    systemListRef.current.scrollTop = 0
                  }
                }}
                placeholder={t(
                  "local.reader.settings.font.search_placeholder"
                )}
                className="pl-8 text-xs"
              />
            </div>

            {filteredSystemFonts.length === 0 ? (
              <p className="py-4 text-center text-xs text-muted-foreground">
                {t("local.reader.settings.font.empty_system")}
              </p>
            ) : (
              <div
                ref={systemListRef}
                style={{ height: `${SYSTEM_LIST_VIEWPORT_HEIGHT}px` }}
                onScroll={(event) =>
                  setScrollTop(event.currentTarget.scrollTop)
                }
                className="relative overflow-y-auto rounded-lg border"
              >
                <div style={{ height: `${totalHeight}px` }} className="relative">
                  {visibleSystemFonts.map((font, offset) => {
                    const rowIndex = startIndex + offset
                    const selected = isSameChoice(value, {
                      kind: "system",
                      family: font.family,
                    })
                    return (
                      <button
                        key={font.family}
                        type="button"
                        style={{
                          position: "absolute",
                          top: `${rowIndex * SYSTEM_LIST_ROW_HEIGHT}px`,
                          height: `${SYSTEM_LIST_ROW_HEIGHT}px`,
                          left: 0,
                          right: 0,
                        }}
                        onClick={() =>
                          chooseAndClose({ kind: "system", family: font.family })
                        }
                        className={cn(
                          "flex items-center justify-between px-2.5 text-left text-xs transition-colors",
                          selected
                            ? "bg-primary/15 font-semibold"
                            : "hover:bg-muted/70"
                        )}
                      >
                        <span className="truncate">{font.family}</span>
                        {selected && (
                          <Check className="size-3.5 shrink-0" aria-hidden="true" />
                        )}
                      </button>
                    )
                  })}
                </div>
              </div>
            )}
          </div>
        )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
