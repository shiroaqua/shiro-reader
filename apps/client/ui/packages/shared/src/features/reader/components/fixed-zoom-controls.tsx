import { Maximize2, Minimize2, ZoomIn, ZoomOut } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import type { FixedZoomMode } from "../contracts/engine"

export interface FixedZoomControlsProps {
  readonly zoom: FixedZoomMode
  readonly onChangeZoom: (next: FixedZoomMode) => void
}

const MIN_FIXED_ZOOM = 0.5
const MAX_FIXED_ZOOM = 4.0
const FIXED_ZOOM_STEP = 0.25

function clampZoom(value: number): number {
  return Math.min(MAX_FIXED_ZOOM, Math.max(MIN_FIXED_ZOOM, Math.round(value * 100) / 100))
}

export function FixedZoomControls({
  zoom,
  onChangeZoom,
}: FixedZoomControlsProps) {
  const { t } = useTranslation()
  const numericZoom = typeof zoom === "number" ? zoom : 1
  const canZoomOut = typeof zoom !== "number" || numericZoom > MIN_FIXED_ZOOM
  const canZoomIn = typeof zoom !== "number" || numericZoom < MAX_FIXED_ZOOM

  const label =
    zoom === "fit-page"
      ? t("local.reader.zoom.fit_page")
      : zoom === "fit-width"
        ? t("local.reader.zoom.fit_width")
        : `${Math.round(zoom * 100)}%`

  const nextPreset: FixedZoomMode =
    zoom === "fit-page" ? "fit-width" : "fit-page"

  return (
    <div
      data-testid="reader-fixed-zoom-controls"
      data-reader-chrome="zoom"
      style={{
        backgroundColor: "var(--reader-chrome-bg, var(--background))",
        borderColor: "var(--reader-chrome-border, var(--border))",
        color: "var(--reader-fg, inherit)",
      }}
      className="pointer-events-auto absolute bottom-3 left-3 z-10 flex items-center gap-0.5 rounded-lg border px-1 py-0.5 shadow-md backdrop-blur-md"
    >
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        className="size-7"
        disabled={!canZoomOut}
        aria-label={t("local.reader.zoom.out")}
        onClick={() => onChangeZoom(clampZoom(numericZoom - FIXED_ZOOM_STEP))}
      >
        <ZoomOut className="size-3.5" aria-hidden="true" />
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="xs"
        className="h-7 min-w-14 px-2 text-[11px] tabular-nums"
        aria-label={
          nextPreset === "fit-width"
            ? t("local.reader.zoom.fit_width")
            : t("local.reader.zoom.fit_page")
        }
        onClick={() => onChangeZoom(nextPreset)}
      >
        {zoom === "fit-width" ? (
          <Minimize2 className="size-3 shrink-0" aria-hidden="true" />
        ) : (
          <Maximize2 className="size-3 shrink-0" aria-hidden="true" />
        )}
        <span>{label}</span>
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        className="size-7"
        disabled={!canZoomIn}
        aria-label={t("local.reader.zoom.in")}
        onClick={() => onChangeZoom(clampZoom(numericZoom + FIXED_ZOOM_STEP))}
      >
        <ZoomIn className="size-3.5" aria-hidden="true" />
      </Button>
    </div>
  )
}
