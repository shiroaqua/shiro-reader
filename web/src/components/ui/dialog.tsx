import * as React from "react"
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"
import { cn } from "cn"

function isWithinErrorAlerts(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("[data-error-alerts]") !== null
}

function Dialog({ onOpenChange, ...props }: DialogPrimitive.Root.Props) {
  return (
    <DialogPrimitive.Root
      data-slot="dialog"
      onOpenChange={(open, eventDetails) => {
        if (
          !open &&
          (eventDetails.reason === "outside-press" ||
            eventDetails.reason === "focus-out")
        ) {
          const event = eventDetails.event
          const relatedTarget =
            "relatedTarget" in event ? event.relatedTarget : null
          if (
            isWithinErrorAlerts(event.target) ||
            isWithinErrorAlerts(relatedTarget)
          ) {
            eventDetails.cancel()
            return
          }
        }
        onOpenChange?.(open, eventDetails)
      }}
      {...props}
    />
  )
}

function DialogContent({ className, ...props }: DialogPrimitive.Popup.Props) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Backdrop className="fixed inset-0 z-50 bg-black/10 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0 supports-backdrop-filter:backdrop-blur-xs" />
      <DialogPrimitive.Popup
        data-slot="dialog-content"
        className={cn(
          "fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100%-2rem)] w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-xl bg-popover p-4 text-sm text-popover-foreground ring-1 ring-foreground/10 outline-none transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0 sm:max-w-md",
          className
        )}
        {...props}
      />
    </DialogPrimitive.Portal>
  )
}

function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("grid gap-1.5", className)} {...props} />
}

function DialogFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("flex flex-col-reverse gap-2 sm:flex-row sm:justify-end", className)}
      {...props}
    />
  )
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  return (
    <DialogPrimitive.Title
      className={cn("font-heading text-base font-medium", className)}
      {...props}
    />
  )
}

function DialogDescription({ className, ...props }: DialogPrimitive.Description.Props) {
  return (
    <DialogPrimitive.Description
      className={cn("text-sm wrap-break-word text-muted-foreground", className)}
      {...props}
    />
  )
}

function DialogClose({ ...props }: DialogPrimitive.Close.Props) {
  return <DialogPrimitive.Close data-slot="dialog-close" {...props} />
}

export {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
  DialogClose,
}
