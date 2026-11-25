import { useRef, type ReactNode } from "react"
import { cn } from "cn"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@shiro/ui-shared/components/ui/sheet"

export type ActionSheetItem = {
  key: string
  label: string
  icon: ReactNode
  onSelect: () => void
  disabled?: boolean
  destructive?: boolean
}

/**
 * Touch replacement for a context menu: a bottom sheet of large rows. The
 * chosen action runs after the sheet has finished closing so that anything it
 * opens (dialogs, inline inputs) can take focus cleanly.
 */
export function ActionSheet({
  open,
  onOpenChange,
  title,
  items,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  items: ActionSheetItem[]
}) {
  const pendingAction = useRef<(() => void) | null>(null)

  return (
    <Sheet
      open={open}
      onOpenChange={onOpenChange}
      onOpenChangeComplete={(isOpen) => {
        if (isOpen) return
        const action = pendingAction.current
        pendingAction.current = null
        action?.()
      }}
    >
      <SheetContent
        side="bottom"
        showCloseButton={false}
        finalFocus={false}
        className="max-h-[85dvh] gap-0 overflow-y-auto rounded-t-2xl pb-[max(0.5rem,env(safe-area-inset-bottom))]"
      >
        <div
          aria-hidden="true"
          className="mx-auto mt-2 h-1 w-10 shrink-0 rounded-full bg-muted-foreground/25"
        />
        <SheetHeader className="px-4 pt-3 pb-2">
          <SheetTitle className="truncate text-center text-sm font-medium text-muted-foreground">
            {title}
          </SheetTitle>
        </SheetHeader>
        <ul className="flex flex-col gap-0.5 px-2 pb-2">
          {items.map((item) => (
            <li key={item.key}>
              <button
                type="button"
                disabled={item.disabled}
                onClick={() => {
                  pendingAction.current = item.onSelect
                  onOpenChange(false)
                }}
                className={cn(
                  "flex h-12 w-full items-center gap-3 rounded-lg px-3 text-left text-base outline-none active:bg-accent disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-5 [&_svg]:shrink-0",
                  item.destructive && "text-destructive"
                )}
              >
                {item.icon}
                <span className="min-w-0 truncate">{item.label}</span>
              </button>
            </li>
          ))}
        </ul>
      </SheetContent>
    </Sheet>
  )
}
