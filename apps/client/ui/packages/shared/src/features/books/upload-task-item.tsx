import { memo } from "react"
import {
  CheckCircle2,
  CircleAlert,
  Clock,
  LoaderCircle,
  RotateCcw,
  X,
} from "lucide-react"
import type { UploadActiveStage, UploadTask } from "@shiro/ui-shared/api/tauri-bridge"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  cancelUploadTask,
  dismissUploadTask,
  retryUploadTask,
} from "@shiro/ui-shared/features/books/book-import-store"
import { formatByteSize, formatEtaSeconds } from "@shiro/ui-shared/lib/format"
import { useTranslation, type LocalTranslationKey } from "@shiro/ui-shared/lib/i18n"

const STAGE_LABEL_KEYS: Record<UploadActiveStage, LocalTranslationKey> = {
  staging: "local.upload.stage.staging",
  uploading: "local.upload.stage.uploading",
  finalizing: "local.upload.stage.finalizing",
}

export const UploadTaskItem = memo(function UploadTaskItem({
  task,
}: {
  task: UploadTask
}) {
  const { t } = useTranslation()
  const { status } = task

  const progressText =
    status.state === "active"
      ? [
          `${t(STAGE_LABEL_KEYS[status.stage])} ${status.progressPercent}%`,
          status.speedBytesPerSec !== null
            ? `${formatByteSize(status.speedBytesPerSec)}/s`
            : null,
          status.etaSeconds !== null
            ? formatEtaSeconds(status.etaSeconds)
            : null,
        ]
          .filter((segment): segment is string => segment !== null)
          .join(" · ")
      : null

  return (
    <div
      data-upload-task={task.id}
      className="flex flex-col gap-1.5 rounded-xl border bg-muted/50 p-2.5 text-xs"
    >
      <div className="flex items-start gap-2">
        <span className="mt-0.5 shrink-0" aria-hidden="true">
          {status.state === "queued" && (
            <Clock className="size-3.5 text-muted-foreground" />
          )}
          {status.state === "active" && (
            <LoaderCircle className="size-3.5 animate-spin text-foreground" />
          )}
          {status.state === "succeeded" && (
            <CheckCircle2 className="size-3.5 text-emerald-600 dark:text-emerald-400" />
          )}
          {status.state === "failed" && (
            <CircleAlert className="size-3.5 text-destructive" />
          )}
        </span>

        <div className="min-w-0 flex-1">
          <p className="wrap-anywhere font-medium leading-snug text-foreground">
            {task.fileName}
          </p>
          {status.state === "queued" && (
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {t("local.upload.stage.queued")}
            </p>
          )}
          {status.state === "succeeded" && (
            <p className="mt-0.5 text-[11px] text-emerald-700 dark:text-emerald-400">
              {t("local.upload.stage.succeeded")}
            </p>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-0.5">
          {(status.state === "queued" || status.state === "active") && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("local.upload.task.cancel")}
              title={t("local.upload.task.cancel")}
              onClick={() => void cancelUploadTask(task.id)}
            >
              <X />
            </Button>
          )}
          {status.state === "failed" && (
            <>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={t("local.upload.task.retry")}
                title={t("local.upload.task.retry")}
                onClick={() => void retryUploadTask(task.id)}
              >
                <RotateCcw />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={t("local.upload.task.dismiss")}
                title={t("local.upload.task.dismiss")}
                onClick={() => void dismissUploadTask(task.id)}
              >
                <X />
              </Button>
            </>
          )}
        </div>
      </div>

      {status.state === "active" && progressText !== null && (
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={status.progressPercent}
          aria-label={task.fileName}
          className="relative h-5 w-full overflow-hidden rounded-full bg-foreground/10 ring-1 ring-inset ring-border/60"
        >
          <div
            className="h-full bg-primary ring-1 ring-inset ring-foreground/15 transition-[width] duration-150"
            style={{
              width: `${Math.min(100, Math.max(0, status.progressPercent))}%`,
            }}
          />
          <span className="absolute inset-0 flex items-center justify-center truncate px-2 text-[11px] font-medium text-primary-foreground tabular-nums">
            {progressText}
          </span>
        </div>
      )}

      {status.state === "failed" && (
        <p className="wrap-anywhere text-[11px] leading-snug text-destructive">
          {status.error.message}
        </p>
      )}
    </div>
  )
})
