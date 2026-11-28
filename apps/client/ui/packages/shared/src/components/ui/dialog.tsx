import * as React from "react"
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"
import { cn } from "cn"

function isWithinFloatingOverlay(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest("[data-error-alerts], [data-upload-panel]") !== null
  )
}

export function shouldCancelUnderlyingModalDismiss(
  open: boolean,
  eventDetails: {
    reason: string
    event: Event
    cancel: () => void
    allowPropagation?: () => void
  },
  protectOutsideOverlays: boolean
): boolean {
  if (open) return false
  if (
    eventDetails.reason === "escape-key" &&
    document.querySelector('[data-upload-panel][role="dialog"]') !== null
  ) {
    eventDetails.cancel()
    // Keep Escape bubbling past the underlying modal's portal so the
    // top-level MobileUploadScreen document listener can dismiss itself even
    // when focus is still inside the underlying modal.
    eventDetails.allowPropagation?.()
    return true
  }
  if (
    protectOutsideOverlays &&
    (eventDetails.reason === "outside-press" ||
      eventDetails.reason === "focus-out")
  ) {
    const event = eventDetails.event
    const relatedTarget =
      "relatedTarget" in event ? (event.relatedTarget as EventTarget | null) : null
    if (
      isWithinFloatingOverlay(event.target) ||
      isWithinFloatingOverlay(relatedTarget)
    ) {
      eventDetails.cancel()
      return true
    }
  }
  return false
}

function Dialog({ onOpenChange, ...props }: DialogPrimitive.Root.Props) {
  return (
    <DialogPrimitive.Root
      data-slot="dialog"
      onOpenChange={(open, eventDetails) => {
        if (shouldCancelUnderlyingModalDismiss(open, eventDetails, true)) return
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
          // Phones: bottom sheet. sm and up: centered modal.
          "fixed z-50 flex flex-col gap-4 bg-popover p-4 text-sm text-popover-foreground ring-1 ring-foreground/10 outline-none transition-[opacity,translate] duration-200 data-ending-style:opacity-0 data-starting-style:opacity-0",
          "max-sm:inset-x-0 max-sm:bottom-0 max-sm:max-h-[92dvh] max-sm:w-full max-sm:overflow-y-auto max-sm:overscroll-contain max-sm:rounded-t-2xl max-sm:pb-[max(1rem,env(safe-area-inset-bottom))] max-sm:data-ending-style:translate-y-8 max-sm:data-starting-style:translate-y-8",
          "max-sm:before:mx-auto max-sm:before:-mb-2 max-sm:before:h-1 max-sm:before:w-10 max-sm:before:shrink-0 max-sm:before:rounded-full max-sm:before:bg-muted-foreground/25 max-sm:before:content-['']",
          "sm:top-1/2 sm:left-1/2 sm:max-h-[calc(100%-2rem)] sm:w-full sm:max-w-md sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-xl",
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
