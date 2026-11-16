import { useState, type ReactElement, type ReactNode } from "react"
import { Popover as PopoverPrimitive } from "@base-ui/react/popover"
import { Check } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useTranslation } from "@/lib/i18n"
import { cn } from "cn"

/** The color a tag gets when none is chosen, matching the server default. */
export const DEFAULT_TAG_COLOR = "#6f6862"

/**
 * Muted colors with enough depth to remain distinct from the page background.
 * The default stays first so the first swatch matches a new tag.
 */
export const TAG_COLORS = [
  "#000000",
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

/** The text color that stays readable on top of the given background. */
export function readableTextColor(color: string): "#000000" | "#ffffff" {
  const red = Number.parseInt(color.slice(1, 3), 16)
  const green = Number.parseInt(color.slice(3, 5), 16)
  const blue = Number.parseInt(color.slice(5, 7), 16)
  // Perceived brightness; the threshold keeps mid-tones on the lighter side.
  return red * 0.299 + green * 0.587 + blue * 0.114 > 150 ? "#000000" : "#ffffff"
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
        "flex size-7 items-center justify-center rounded-full ring-1 ring-foreground/15 ring-inset transition-transform outline-none hover:scale-110 focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
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
  const custom = isTagColor(draft) ? draft.toLowerCase() : null
  const matchesPreset = TAG_COLORS.some(
    (preset) => preset.toLowerCase() === color.toLowerCase()
  )

  return (
    <div className="grid gap-3">
      <div className="grid grid-cols-7 gap-1.5">
        {TAG_COLORS.map((preset) => (
          <TagColorSwatch
            key={preset}
            color={preset}
            selected={preset.toLowerCase() === color.toLowerCase()}
            label={t("tag.color.option").replace("{color}", preset)}
            disabled={disabled}
            onSelect={onSelect}
          />
        ))}
      </div>
      <form
        className="flex items-center gap-1.5"
        onSubmit={(event) => {
          event.preventDefault()
          if (custom) onSelect(custom)
        }}
      >
        <span
          aria-hidden="true"
          style={{ backgroundColor: custom ?? color }}
          className="size-7 shrink-0 rounded-full ring-1 ring-foreground/15 ring-inset"
        />
        <input
          value={draft}
          disabled={disabled}
          spellCheck={false}
          maxLength={7}
          aria-label={t("tag.color.custom")}
          placeholder={matchesPreset ? color : color.toUpperCase()}
          onChange={(event) => setDraft(event.target.value.trim())}
          className="h-7 w-24 rounded-md border border-input bg-transparent px-2 font-mono text-xs uppercase outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        />
        <Button
          type="submit"
          size="sm"
          variant="outline"
          disabled={disabled || custom === null}
          className="h-7 px-2"
        >
          {t("tag.color.apply")}
        </Button>
      </form>
    </div>
  )
}

/**
 * Text color for a tag name. The default color falls back to the theme's
 * foreground so default tags stay readable in dark mode.
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
  return (
    <TagColorPopover
      color={color}
      disabled={disabled}
      onSelect={onSelect}
      trigger={
        <button
          type="button"
          aria-label={label}
          title={name}
          disabled={disabled}
          style={{ color: tagNameColor(color) }}
          className={cn(
            "min-w-0 cursor-pointer truncate rounded-sm text-left font-medium outline-none hover:underline hover:underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:no-underline",
            className
          )}
        >
          {name}
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
  return (
    <PopoverPrimitive.Root>
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
              {t("tag.color")}
            </p>
            <TagColorPalette color={color} disabled={disabled} onSelect={onSelect} />
          </PopoverPrimitive.Popup>
        </PopoverPrimitive.Positioner>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}
