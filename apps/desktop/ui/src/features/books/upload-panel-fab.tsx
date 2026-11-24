import { Upload } from "lucide-react"
import { cn } from "cn"

export function UploadPanelMinimizedButton({
  remaining,
  failed,
  label,
  onExpand,
}: {
  remaining: number
  failed: number
  label: string
  onExpand: () => void
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onExpand}
      className="relative flex size-12 items-center justify-center rounded-full border bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10 transition-colors hover:bg-accent"
    >
      <Upload
        aria-hidden="true"
        className={cn("size-5", remaining > 0 && "animate-pulse")}
      />
      {remaining > 0 && (
        <span
          data-upload-badge-remaining=""
          className="absolute -top-1 -left-1 flex min-w-5 items-center justify-center rounded-full bg-primary px-1 text-[11px] font-semibold text-primary-foreground tabular-nums shadow-xs ring-1 ring-foreground/15"
        >
          {remaining}
        </span>
      )}
      {failed > 0 && (
        <span
          data-upload-badge-failed=""
          className="absolute -top-1 -right-1 flex min-w-5 items-center justify-center rounded-full bg-destructive px-1 text-[11px] font-semibold text-white tabular-nums shadow-xs"
        >
          {failed}
        </span>
      )}
    </button>
  )
}
