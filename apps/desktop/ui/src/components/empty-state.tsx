import type { ReactNode } from "react"
import { cn } from "cn"

/** Centered placeholder for empty, failed or not-yet-available content. */
export function EmptyState({
  icon,
  title,
  description,
  action,
  tone = "default",
  className,
}: {
  icon: ReactNode
  title: ReactNode
  description?: ReactNode
  action?: ReactNode
  tone?: "default" | "destructive"
  className?: string
}) {
  return (
    <div
      role="status"
      className={cn(
        "flex flex-1 flex-col items-center justify-center gap-3 px-4 py-12 text-center",
        className
      )}
    >
      <div
        className={cn(
          "flex size-16 items-center justify-center rounded-2xl [&_svg:not([class*='size-'])]:size-8",
          tone === "destructive"
            ? "bg-destructive/10 text-destructive"
            : "bg-muted/80 text-muted-foreground"
        )}
      >
        {icon}
      </div>
      <div className="grid gap-1">
        <p className="text-sm font-medium text-foreground">{title}</p>
        {description && (
          <p className="text-sm text-muted-foreground">{description}</p>
        )}
      </div>
      {action}
    </div>
  )
}
