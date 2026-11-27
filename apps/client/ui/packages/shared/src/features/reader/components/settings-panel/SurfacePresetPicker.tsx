import { Check } from "lucide-react"
import type { AppTheme } from "@shiro/ui-shared/lib/theme"
import { useTranslation, type LocalTranslationKey } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import type { ReaderSurfacePreset } from "../../contracts/settings"
import {
  READER_SURFACE_PRESETS,
  resolveSurfacePalette,
} from "../../settings/defaults"

const SURFACE_LABEL_KEYS: Readonly<
  Record<ReaderSurfacePreset, LocalTranslationKey>
> = {
  follow_app: "local.reader.settings.surface.follow_app",
  paper_light: "local.reader.settings.surface.paper_light",
  sepia_warm: "local.reader.settings.surface.sepia_warm",
  eye_green: "local.reader.settings.surface.eye_green",
  night_dark: "local.reader.settings.surface.night_dark",
  amoled_black: "local.reader.settings.surface.amoled_black",
}

export interface SurfacePresetPickerProps {
  readonly value: ReaderSurfacePreset
  readonly appTheme: AppTheme
  readonly onChange: (surface: ReaderSurfacePreset) => void
}

export function SurfacePresetPicker({
  value,
  appTheme,
  onChange,
}: SurfacePresetPickerProps) {
  const { t } = useTranslation()

  return (
    <div className="grid grid-cols-3 gap-2">
      {READER_SURFACE_PRESETS.map((preset) => {
        const palette = resolveSurfacePalette(preset, appTheme)
        const selected = value === preset
        return (
          <button
            key={preset}
            type="button"
            aria-pressed={selected}
            onClick={() => onChange(preset)}
            className={cn(
              "flex items-center gap-2 rounded-lg border px-2 py-1.5 text-left text-xs transition-colors",
              selected
                ? "border-primary ring-2 ring-primary/25 font-medium"
                : "border-border hover:border-foreground/30"
            )}
          >
            <span
              className="flex size-4 shrink-0 items-center justify-center rounded-full border border-foreground/20"
              style={{
                backgroundColor: palette.backgroundHex,
                color: palette.foregroundHex,
              }}
            >
              {selected && <Check className="size-2.5" aria-hidden="true" />}
            </span>
            <span className="truncate">{t(SURFACE_LABEL_KEYS[preset])}</span>
          </button>
        )
      })}
    </div>
  )
}
