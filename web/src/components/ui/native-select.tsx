import * as React from "react"
import { ChevronDown } from "lucide-react"
import { cn } from "cn"

/**
 * Native `<select>` styled to match `Input`. `className` sizes the wrapper;
 * the select fills it.
 */
function NativeSelect({
  className,
  ...props
}: React.ComponentProps<"select">) {
  return (
    <div
      data-slot="native-select-wrapper"
      className={cn("relative h-8 w-full min-w-0", className)}
    >
      <select
        data-slot="native-select"
        className="size-full min-w-0 appearance-none rounded-lg border border-input bg-transparent pr-8 pl-2.5 text-sm transition-colors outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 dark:[&>option]:bg-popover"
        {...props}
      />
      <ChevronDown
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 right-2.5 size-4 -translate-y-1/2 text-muted-foreground"
      />
    </div>
  )
}

export { NativeSelect }
