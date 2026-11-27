import { useRef, useState } from "react"
import { Undo2 } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import type { ReaderPageStats } from "../contracts/engine"

const SLIDER_RESOLUTION = 1000

export interface ProgressScrubberProps {
  readonly fraction: number
  readonly pageStats: ReaderPageStats | null
  readonly activeChapterTitle?: string | null | undefined
  readonly canGoBack: boolean
  readonly backFraction?: number | null | undefined
  readonly disabled?: boolean | undefined
  readonly onCommitFraction: (fraction: number) => void
  readonly onGoBack: () => void
  readonly className?: string | undefined
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.min(1, Math.max(0, value))
}

export function ProgressScrubber({
  fraction,
  pageStats,
  activeChapterTitle,
  canGoBack,
  backFraction,
  disabled = false,
  onCommitFraction,
  onGoBack,
  className,
}: ProgressScrubberProps) {
  const { t } = useTranslation()
  const [draggingFraction, setDraggingFraction] = useState<number | null>(null)
  const pendingFractionRef = useRef<number | null>(null)

  const displayFraction = clamp01(draggingFraction ?? fraction)
  const sliderValue = Math.round(displayFraction * SLIDER_RESOLUTION)
  const percentageLabel = `${(displayFraction * 100).toFixed(1)}%`

  const commitIfPending = () => {
    const pending = pendingFractionRef.current
    pendingFractionRef.current = null
    setDraggingFraction(null)
    if (pending !== null) {
      onCommitFraction(pending)
    }
  }

  return (
    <div
      className={cn(
        "flex flex-col gap-1.5 px-3 py-2 text-xs select-none",
        className
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          {canGoBack && (
            <Button
              type="button"
              variant="secondary"
              size="xs"
              data-testid="reader-history-back-pill"
              onClick={onGoBack}
              className="shrink-0 gap-1 rounded-full"
            >
              <Undo2 className="size-3" aria-hidden="true" />
              <span>{t("local.reader.progress.back_to_previous")}</span>
              {typeof backFraction === "number" && Number.isFinite(backFraction) && (
                <span className="opacity-70">
                  {`${(clamp01(backFraction) * 100).toFixed(0)}%`}
                </span>
              )}
            </Button>
          )}
          {activeChapterTitle && (
            <span className="truncate opacity-75">{activeChapterTitle}</span>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-2 tabular-nums opacity-80">
          {pageStats && (
            <span data-testid="reader-page-stats">
              {t("local.reader.progress.page_of")
                .replace("{current}", String(pageStats.current))
                .replace("{total}", String(pageStats.total))}
            </span>
          )}
          <span data-testid="reader-progress-percent">{percentageLabel}</span>
        </div>
      </div>

      <input
        type="range"
        min={0}
        max={SLIDER_RESOLUTION}
        step={1}
        value={sliderValue}
        disabled={disabled}
        aria-label={t("local.reader.progress.slider_label")}
        onChange={(event) => {
          const next = clamp01(Number(event.target.value) / SLIDER_RESOLUTION)
          pendingFractionRef.current = next
          setDraggingFraction(next)
        }}
        onPointerUp={commitIfPending}
        onMouseUp={commitIfPending}
        onTouchEnd={commitIfPending}
        onKeyUp={commitIfPending}
        onBlur={commitIfPending}
        className="h-2 w-full cursor-pointer accent-primary disabled:cursor-not-allowed disabled:opacity-50"
      />
    </div>
  )
}
