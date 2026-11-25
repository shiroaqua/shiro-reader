import { useId, useState } from "react"
import { Ban, ChevronDown, Settings, Trash2, X } from "lucide-react"
import type { UploadPanelSnapshot } from "@shiro/ui-shared/api/tauri-bridge"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { Input } from "@shiro/ui-shared/components/ui/input"
import {
  cancelAllUploads,
  dismissUploadErrors,
  setUploadConcurrency,
  toggleUploadOptions,
} from "@shiro/ui-shared/features/books/book-import-store"
import { UploadTaskItem } from "@shiro/ui-shared/features/books/upload-task-item"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

const MAX_RENDERED_TASKS = 100

function parseStrictPositiveInt(raw: string): number | null {
  const trimmed = raw.trim()
  if (!/^\d+$/.test(trimmed)) return null
  const parsed = Number(trimmed)
  return Number.isSafeInteger(parsed) ? parsed : null
}

export function UploadPanelBody({
  upload,
  optionsOpen,
  variant,
  dismissLabel,
  onDismiss,
}: {
  upload: UploadPanelSnapshot
  optionsOpen: boolean
  variant: "floating" | "fullscreen"
  dismissLabel: string
  onDismiss: () => void
}) {
  const { t } = useTranslation()
  const concurrencyInputId = useId()
  const [draftConcurrency, setDraftConcurrency] = useState<string | null>(null)

  const processed = Math.max(0, upload.stats.total - upload.stats.remaining)
  const visibleTasks = upload.tasks.slice(0, MAX_RENDERED_TASKS)
  const hiddenCount = Math.max(0, upload.tasks.length - MAX_RENDERED_TASKS)
  const concurrencyValue =
    optionsOpen && draftConcurrency !== null
      ? draftConcurrency
      : String(upload.concurrency)

  const commitConcurrency = (raw: string) => {
    setDraftConcurrency(null)
    const parsed = parseStrictPositiveInt(raw)
    if (parsed === null) return
    const clamped = Math.min(16, Math.max(1, parsed))
    if (clamped !== upload.concurrency) void setUploadConcurrency(clamped)
  }

  return (
    <section
      aria-label={t("local.upload.panel.title")}
      className={
        variant === "fullscreen"
          ? "flex h-full min-h-0 w-full flex-col bg-background text-foreground"
          : "flex w-84 max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-2xl border bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10"
      }
    >
      <header
        className={
          variant === "fullscreen"
            ? "flex items-start justify-between gap-2 border-b px-4 py-3"
            : "flex items-start justify-between gap-2 border-b px-3 py-2.5"
        }
      >
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-sm font-semibold">
              {t("local.upload.panel.title")}
            </h2>
            <span className="text-xs text-muted-foreground tabular-nums">
              {t("local.upload.panel.progress_summary")
                .replace("{done}", String(processed))
                .replace("{total}", String(upload.stats.total))}
            </span>
          </div>
          <div className="mt-0.5 flex flex-wrap items-center gap-x-2.5 text-[11px] text-muted-foreground tabular-nums">
            <span>
              {t("local.upload.panel.stat_succeeded").replace(
                "{count}",
                String(upload.stats.succeeded)
              )}
            </span>
            {upload.stats.failed > 0 && (
              <span className="font-medium text-destructive">
                {t("local.upload.panel.stat_failed").replace(
                  "{count}",
                  String(upload.stats.failed)
                )}
              </span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          <Button
            type="button"
            variant={optionsOpen ? "secondary" : "ghost"}
            size="icon-xs"
            aria-label={t("local.upload.panel.settings")}
            title={t("local.upload.panel.settings")}
            aria-expanded={optionsOpen}
            onClick={() => {
              setDraftConcurrency(null)
              toggleUploadOptions()
            }}
          >
            <Settings />
          </Button>
          {upload.hasDismissibleErrors && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("local.upload.panel.clear_errors")}
              title={t("local.upload.panel.clear_errors")}
              onClick={() => void dismissUploadErrors()}
            >
              <Trash2 />
            </Button>
          )}
          {upload.stats.remaining > 0 && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("local.upload.panel.cancel_all")}
              title={t("local.upload.panel.cancel_all")}
              onClick={() => void cancelAllUploads()}
            >
              <Ban />
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={dismissLabel}
            title={dismissLabel}
            onClick={onDismiss}
          >
            {variant === "fullscreen" ? <X /> : <ChevronDown />}
          </Button>
        </div>
      </header>

      {optionsOpen && (
        <div
          className={
            variant === "fullscreen"
              ? "flex items-center justify-between gap-3 border-b bg-muted/40 px-4 py-2.5 text-xs"
              : "flex items-center justify-between gap-3 border-b bg-muted/40 px-3 py-2 text-xs"
          }
        >
          <label htmlFor={concurrencyInputId} className="flex flex-col">
            <span className="font-medium text-foreground">
              {t("local.upload.panel.concurrency")}
            </span>
            <span className="text-[11px] text-muted-foreground">
              {t("local.upload.panel.concurrency_hint")}
            </span>
          </label>
          <Input
            id={concurrencyInputId}
            type="text"
            inputMode="numeric"
            value={concurrencyValue}
            onChange={(e) => {
              const raw = e.target.value
              setDraftConcurrency(raw)
              const parsed = parseStrictPositiveInt(raw)
              if (parsed !== null && parsed >= 1 && parsed <= 16) {
                if (parsed !== upload.concurrency) {
                  void setUploadConcurrency(parsed)
                }
              }
            }}
            onBlur={() => commitConcurrency(concurrencyValue)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitConcurrency(concurrencyValue)
              if (e.key === "Escape") setDraftConcurrency(null)
            }}
            className="h-7 w-16 text-center text-xs tabular-nums"
          />
        </div>
      )}

      <div
        className={
          variant === "fullscreen"
            ? "flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-3"
            : "flex max-h-72 flex-col gap-2 overflow-y-auto p-2.5"
        }
      >
        {visibleTasks.map((task) => (
          <UploadTaskItem key={task.id} task={task} />
        ))}
        {hiddenCount > 0 && (
          <p className="py-1 text-center text-[11px] text-muted-foreground tabular-nums">
            {t("local.upload.panel.more_queued").replace(
              "{count}",
              String(hiddenCount)
            )}
          </p>
        )}
      </div>
    </section>
  )
}
