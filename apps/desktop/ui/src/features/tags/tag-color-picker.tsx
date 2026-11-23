import { useState, type ReactElement, type ReactNode } from "react"
import { Popover as PopoverPrimitive } from "@base-ui/react/popover"
import { Check } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { translateTagName, useTranslation } from "@/lib/i18n"
import { cn } from "cn"

/** The color a tag gets when none is chosen, matching the server default. */
export const DEFAULT_TAG_COLOR = "#000000"

/**
 * Muted colors with enough depth to remain distinct from the page background.
 * The default stays first so the first swatch matches a new tag.
 */
export const TAG_COLORS = [
  DEFAULT_TAG_COLOR,
  "#968b82",
  "#d98282",
  "#df9a74",
  "#d8b45e",
  "#a4bd69",
  "#79b78f",
  "#70b9b1",
  "#75a9cc",
  "#8495d1",
  "#a18ace",
  "#cf8fc4",
  "#d98da3",
  "#b39479",
] as const

const HEX_COLOR = /^#[0-9a-fA-F]{6}$/

/** True for the exact `#RRGGBB` format the server accepts. */
export function isTagColor(value: string): boolean {
  return HEX_COLOR.test(value)
}

function normalizeCustomColor(value: string): string | null {
  const trimmed = value.trim()
  const candidate = trimmed.startsWith("#") ? trimmed : `#${trimmed}`
  return isTagColor(candidate) ? candidate.toLowerCase() : null
}

/** The text color that stays readable on top of the given background. */
export function readableTextColor(color: string): "#000000" | "#ffffff" {
  const red = Number.parseInt(color.slice(1, 3), 16)
  const green = Number.parseInt(color.slice(3, 5), 16)
  const blue = Number.parseInt(color.slice(5, 7), 16)
  // Perceived brightness; the threshold keeps mid-tones on the lighter side.
  return red * 0.299 + green * 0.587 + blue * 0.114 > 150
    ? "#000000"
    : "#ffffff"
}

/** A round swatch filled with the color, ringed when it is the current choice. */
export function TagColorSwatch({
  color,
  selected,
  label,
  disabled,
  onSelect,
}: {
  color: string
  selected: boolean
  label: string
  disabled?: boolean
  onSelect: (color: string) => void
}) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={selected}
      disabled={disabled}
      title={label}
      onClick={() => onSelect(color)}
      style={{ backgroundColor: color }}
      className={cn(
        "flex size-7 items-center justify-center rounded-full ring-1 ring-foreground/15 transition-transform outline-none ring-inset hover:scale-110 focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
        selected && "ring-2 ring-ring ring-offset-2 ring-offset-popover"
      )}
    >
      {selected && (
        <Check
          aria-hidden="true"
          className="size-3.5"
          style={{ color: readableTextColor(color) }}
        />
      )}
    </button>
  )
}

/** Palette plus a free-form hex field, shared by the define dialog and the row popover. */
export function TagColorPalette({
  color,
  disabled,
  onSelect,
}: {
  color: string
  disabled?: boolean
  onSelect: (color: string) => void
}) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState("")
  const custom = normalizeCustomColor(draft)
  const matchesPreset = TAG_COLORS.some(
    (preset) => preset.toLowerCase() === color.toLowerCase()
  )

  const applyCustom = () => {
    if (!custom || disabled) return
    onSelect(custom)
    setDraft("")
  }

  return (
    <div className="grid gap-3">
      <div className="grid grid-cols-7 gap-1.5">
        {TAG_COLORS.map((preset) => (
          <TagColorSwatch
            key={preset}
            color={preset}
            selected={preset.toLowerCase() === color.toLowerCase()}
            label={t("local.tag.color.option").replace("{color}", preset)}
            disabled={disabled}
            onSelect={onSelect}
          />
        ))}
      </div>
      <div className="flex items-center gap-1.5">
        <span
          aria-hidden="true"
          style={{ backgroundColor: custom ?? color }}
          className="size-7 shrink-0 rounded-full ring-1 ring-foreground/15 ring-inset"
        />
        <Input
          value={draft}
          disabled={disabled}
          spellCheck={false}
          maxLength={7}
          aria-label={t("local.tag.color.custom")}
          placeholder={matchesPreset ? color : color.toUpperCase()}
          onChange={(event) => setDraft(event.target.value.trim())}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return
            if (event.key === "Enter") {
              event.preventDefault()
              applyCustom()
            }
          }}
          className="h-7 w-24 px-2 font-mono text-xs uppercase md:text-xs"
        />
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled || custom === null}
          onClick={applyCustom}
          className="h-7 px-2"
        >
          {t("local.tag.color.apply")}
        </Button>
      </div>
    </div>
  )
}

/**
 * Text color for a tag name. The default color falls back to the theme's
 * foreground so default tags stay readable on every color theme.
 */
export function tagNameColor(color: string): string | undefined {
  return color.toLowerCase() === DEFAULT_TAG_COLOR ? undefined : color
}

/** A tag name drawn in its color; clicking it opens the palette. */
export function TagColorName({
  name,
  color,
  disabled,
  label,
  className,
  onSelect,
}: {
  name: string
  color: string
  disabled?: boolean
  label: string
  className?: string
  onSelect: (color: string) => void
}) {
  const { locale } = useTranslation()
  const displayName = translateTagName(name, locale)
  return (
    <TagColorPopover
      color={color}
      disabled={disabled}
      onSelect={onSelect}
      trigger={
        <button
          type="button"
          aria-label={label}
          title={displayName}
          disabled={disabled}
          style={{ color: tagNameColor(color) }}
          className={cn(
            "min-w-0 cursor-pointer truncate rounded-sm text-left font-medium outline-none hover:underline hover:underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:no-underline",
            className
          )}
        >
          {displayName}
        </button>
      }
    />
  )
}

/** Popover wrapping the palette, anchored to whatever trigger the caller renders. */
export function TagColorPopover({
  color,
  disabled,
  onSelect,
  trigger,
}: {
  color: string
  disabled?: boolean
  onSelect: (color: string) => void
  trigger: ReactNode
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <PopoverPrimitive.Root
      open={open}
      onOpenChange={(next) => {
        if (!disabled || !next) setOpen(next)
      }}
    >
      <PopoverPrimitive.Trigger
        disabled={disabled}
        render={trigger as ReactElement}
      />
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Positioner
          side="bottom"
          align="start"
          sideOffset={6}
          className="z-50 outline-none"
        >
          <PopoverPrimitive.Popup className="origin-(--transform-origin) rounded-xl bg-popover p-3 text-popover-foreground shadow-lg ring-1 ring-foreground/10 outline-none data-ending-style:opacity-0 data-starting-style:opacity-0">
            <p className="mb-2 text-xs font-medium text-muted-foreground">
              {t("local.tag.color")}
            </p>
            <TagColorPalette
              color={color}
              disabled={disabled}
              onSelect={(nextColor) => {
                onSelect(nextColor)
                setOpen(false)
              }}
            />
          </PopoverPrimitive.Popup>
        </PopoverPrimitive.Positioner>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}
