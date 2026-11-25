import type { ComponentProps, ReactNode } from "react"
import { cn } from "cn"

/** Padded, vertically stacked content area of a dashboard page. */
export function PageSection({ className, ...props }: ComponentProps<"section">) {
  return (
    <section
      className={cn(
        "flex w-full min-w-0 flex-1 flex-col gap-6 p-4 sm:p-6",
        className
      )}
      {...props}
    />
  )
}

/** Toolbar row above page content: context on the start side, actions on the end side. */
export function PageToolbar({
  start,
  end,
}: {
  start?: ReactNode
  end?: ReactNode
}) {
  return (
    <div className="flex min-h-8 w-full min-w-0 flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-center gap-3 text-sm text-muted-foreground">
        {start}
      </div>
      <div className="ml-auto flex max-w-full min-w-0 flex-wrap items-center justify-end gap-2">
        {end}
      </div>
    </div>
  )
}
