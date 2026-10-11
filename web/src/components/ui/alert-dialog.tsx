import type { ComponentProps } from "react"
import { AlertDialog as AlertDialogPrimitive } from "@base-ui/react/alert-dialog"

import { Button } from "@/components/ui/button"
import {
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

function AlertDialog({ ...props }: AlertDialogPrimitive.Root.Props) {
  return <AlertDialogPrimitive.Root {...props} />
}

function AlertDialogContent({ ...props }: AlertDialogPrimitive.Popup.Props) {
  return (
    <DialogContent
      data-slot="alert-dialog-content"
      showCloseButton={false}
      {...props}
    />
  )
}

function AlertDialogHeader({ ...props }: ComponentProps<typeof DialogHeader>) {
  return <DialogHeader data-slot="alert-dialog-header" {...props} />
}

function AlertDialogFooter({ ...props }: ComponentProps<typeof DialogFooter>) {
  return <DialogFooter data-slot="alert-dialog-footer" {...props} />
}

function AlertDialogTitle({ ...props }: AlertDialogPrimitive.Title.Props) {
  return <DialogTitle data-slot="alert-dialog-title" {...props} />
}

function AlertDialogDescription({
  ...props
}: AlertDialogPrimitive.Description.Props) {
  return <DialogDescription data-slot="alert-dialog-description" {...props} />
}

function AlertDialogCancel({
  ...props
}: ComponentProps<typeof AlertDialogPrimitive.Close>) {
  return (
    <AlertDialogPrimitive.Close
      data-slot="alert-dialog-cancel"
      render={<Button variant="outline" />}
      {...props}
    />
  )
}

// Async actions close the controlled dialog only after their work succeeds.
function AlertDialogAction({ ...props }: ComponentProps<typeof Button>) {
  return (
    <Button data-slot="alert-dialog-action" variant="destructive" {...props} />
  )
}

export {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
}
