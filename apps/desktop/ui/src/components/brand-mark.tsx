import { Book } from "lucide-react"
import { cn } from "cn"

/** ShiroReader logo mark shared by the login screen and sidebar header. */
export function BrandMark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "flex size-8 shrink-0 items-center justify-center rounded-lg bg-theme-gradient text-theme-gradient-foreground shadow-sm [&_svg:not([class*='size-'])]:size-4",
        className
      )}
    >
      <Book />
    </span>
  )
}
