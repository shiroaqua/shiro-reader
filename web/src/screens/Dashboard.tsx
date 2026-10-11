import type { ComponentProps, ReactNode } from "react"
import { useId, useRef, useState } from "react"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  Sidebar,
  SidebarContent,
  SidebarMenu,
  SidebarGroup,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuAction,
  SidebarTrigger,
  SidebarProvider,
} from "@/components/ui/sidebar"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Search,
  Clock,
  LibraryBig,
  Tag,
  NotebookPen,
  ChartNoAxesCombined,
  Trash,
  Plus,
  CircleX,
  Pencil,
} from "lucide-react"
import { useLibraryController } from "@/hooks/useLibraryController"
import { ApiError } from "@/api/library"
import { notifyError } from "@/lib/error-notifications"
import { useTranslation } from "@/lib/i18n"
import type { Bookshelf } from "@/lib/types"

export function DashboardScreen() {
  const controller = useLibraryController()
  const { t } = useTranslation()
  const [isAddShelfDialogOpen, setIsAddShelfDialogOpen] = useState(false)
  const [isCreatingShelf, setIsCreatingShelf] = useState(false)
  const [bookshelfToDelete, setBookshelfToDelete] = useState<Bookshelf | null>(null)
  const cancelDeleteButtonRef = useRef<HTMLButtonElement>(null)
  const deleteRequestPendingRef = useRef(false)
  const shelfNameInputId = useId()
  const [bookshelfToRename, setBookshelfToRename] = useState<Bookshelf | null>(null)
  const [renameShelfName, setRenameShelfName] = useState("")
  const renameShelfInputId = useId()
  const trimmedRenameShelfName = renameShelfName.trim()
  const canSubmitRename =
    !controller.isRenamingBookshelf &&
    trimmedRenameShelfName !== "" &&
    trimmedRenameShelfName !== bookshelfToRename?.name

  const menus: DashboardMenu[] = [
    {
      id: "search",
      icon: <Search />,
      name: t("dashboard.search"),
    },
    {
      id: "shelves",
      icon: <LibraryBig />,
      name: t("dashboard.shelves"),
      action: {
        label: t("bookshelf.add"),
        icon: <Plus className="size-4" />,
        buttonProps: {
          onClick: () => {
            controller.setNewBookshelfName("")
            setIsAddShelfDialogOpen(true)
          },
        },
      },
      content: (
        <SidebarMenuSub>
          {controller.shelves.map((bookshelf) => (
            <ContextMenu key={bookshelf.id}>
              <ContextMenuTrigger
                data-slot="sidebar-menu-sub-item"
                aria-busy={controller.deletingBookshelfId === bookshelf.id}
                render={<SidebarMenuSubItem />}
              >
                <SidebarMenuSubButton
                  className="w-full"
                  render={<button type="button" />}
                >
                  {bookshelf.name}
                </SidebarMenuSubButton>
              </ContextMenuTrigger>
              <ContextMenuContent aria-label={bookshelf.name}>
                <ContextMenuItem
                  disabled={controller.isRenamingBookshelf}
                  onClick={() => {
                    setRenameShelfName(bookshelf.name)
                    setBookshelfToRename(bookshelf)
                  }}
                >
                  <Pencil aria-hidden="true" />
                  <span>{t("bookshelf.rename")}</span>
                </ContextMenuItem>
                <ContextMenuItem
                  variant="destructive"
                  disabled={controller.isDeletingBookshelf}
                  onClick={() => {
                    setBookshelfToDelete(bookshelf)
                  }}
                >
                  <CircleX aria-hidden="true" />
                  <span>{t("bookshelf.delete")}</span>
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          ))}
        </SidebarMenuSub>
      ),
    },
    {
      id: "recent",
      icon: <Clock />,
      name: t("dashboard.recent"),
    },
    {
      id: "tags",
      icon: <Tag />,
      name: t("dashboard.tags"),
    },
    {
      id: "notes",
      icon: <NotebookPen />,
      name: t("dashboard.notes"),
    },
    {
      id: "statistics",
      icon: <ChartNoAxesCombined />,
      name: t("dashboard.statistics"),
    },

    {
      id: "trash",
      icon: <Trash />,
      name: t("dashboard.trash"),
    },
  ]

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarGroup>
          <SidebarContent>
            <SidebarMenu>
              {menus.map((item) => (
                <DashboardMenuItem key={item.id} item={item} />
              ))}
            </SidebarMenu>
          </SidebarContent>
        </SidebarGroup>
      </Sidebar>
      <main>
        <SidebarTrigger />
      </main>
      <AlertDialog
        open={bookshelfToDelete !== null}
        onOpenChange={(open, eventDetails) => {
          if (!open) {
            if (deleteRequestPendingRef.current) {
              eventDetails.cancel()
              return
            }
            setBookshelfToDelete(null)
          }
        }}
      >
        <AlertDialogContent
          initialFocus={cancelDeleteButtonRef}
          aria-busy={controller.isDeletingBookshelf}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>{t("bookshelf.delete_title")}</AlertDialogTitle>
            <AlertDialogDescription className="break-words">
              {t("bookshelf.delete_description").replace(
                "{name}",
                () => bookshelfToDelete?.name ?? ""
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel
              ref={cancelDeleteButtonRef}
              disabled={controller.isDeletingBookshelf}
            >
              {t("common.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={controller.isDeletingBookshelf}
              onClick={async () => {
                if (!bookshelfToDelete || deleteRequestPendingRef.current) return

                deleteRequestPendingRef.current = true
                try {
                  await controller.deleteBookshelf(bookshelfToDelete.id)
                  setBookshelfToDelete(null)
                } catch (error) {
                  if (!(error instanceof ApiError)) {
                    notifyError("common.error.unknown")
                  }
                } finally {
                  deleteRequestPendingRef.current = false
                }
              }}
            >
              {t(
                controller.isDeletingBookshelf
                  ? "bookshelf.deleting"
                  : "bookshelf.delete"
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <Dialog
        open={isAddShelfDialogOpen}
        onOpenChange={(open) => {
          if (!isCreatingShelf) setIsAddShelfDialogOpen(open)
        }}
      >
        <DialogContent showCloseButton={!isCreatingShelf}>
          <form
            className="grid gap-4"
            onSubmit={async (event) => {
              event.preventDefault()
              if (isCreatingShelf || !controller.newBookshelfName.trim()) return

              setIsCreatingShelf(true)
              try {
                await controller.createBookshelf()
                setIsAddShelfDialogOpen(false)
              } catch (error) {
                if (!(error instanceof ApiError)) notifyError("common.error.unknown")
              } finally {
                setIsCreatingShelf(false)
              }
            }}
          >
            <DialogHeader>
              <DialogTitle>{t("bookshelf.add")}</DialogTitle>
            </DialogHeader>
            <div className="grid gap-2">
              <Input
                id={shelfNameInputId}
                name="shelfName"
                aria-label={t("bookshelf.name")}
                placeholder={t("bookshelf.name_placeholder")}
                value={controller.newBookshelfName}
                onChange={(event) => {
                  controller.setNewBookshelfName(event.target.value)
                }}
                disabled={isCreatingShelf}
              />
            </div>
            <DialogFooter>
              <Button
                type="submit"
                disabled={isCreatingShelf || !controller.newBookshelfName.trim()}
              >
                {t(isCreatingShelf ? "bookshelf.submitting" : "common.submit")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
      <Dialog
        open={bookshelfToRename !== null}
        onOpenChange={(open) => {
          if (!open && !controller.isRenamingBookshelf) setBookshelfToRename(null)
        }}
      >
        <DialogContent showCloseButton={!controller.isRenamingBookshelf}>
          <form
            className="grid gap-4"
            onSubmit={async (event) => {
              event.preventDefault()
              if (!bookshelfToRename || !canSubmitRename) return

              try {
                await controller.renameBookshelf({
                  id: bookshelfToRename.id,
                  name: trimmedRenameShelfName,
                })
                setBookshelfToRename(null)
              } catch (error) {
                if (!(error instanceof ApiError)) notifyError("common.error.unknown")
              }
            }}
          >
            <DialogHeader>
              <DialogTitle>{t("bookshelf.rename_title")}</DialogTitle>
            </DialogHeader>
            <div className="grid gap-2">
              <Input
                id={renameShelfInputId}
                name="shelfName"
                aria-label={t("bookshelf.name")}
                placeholder={t("bookshelf.name_placeholder")}
                value={renameShelfName}
                onChange={(event) => {
                  setRenameShelfName(event.target.value)
                }}
                disabled={controller.isRenamingBookshelf}
              />
            </div>
            <DialogFooter>
              <Button type="submit" disabled={!canSubmitRename}>
                {t(
                  controller.isRenamingBookshelf
                    ? "bookshelf.renaming"
                    : "common.submit"
                )}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </SidebarProvider>
  )
}

type DashboardMenu = {
  id: string
  name: string
  icon: ReactNode
  buttonProps?: Omit<ComponentProps<typeof SidebarMenuButton>, "children">
  content?: ReactNode
  defaultOpen?: boolean
  action?: {
    label: string
    icon: ReactNode
    buttonProps?: Omit<ComponentProps<typeof SidebarMenuAction>, "children">
  }
}

function DashboardMenuItem({ item }: { item: DashboardMenu }) {
  const button = (
    <SidebarMenuButton tooltip={item.name} {...item.buttonProps}>
      {item.icon}
      <span>{item.name}</span>
    </SidebarMenuButton>
  )
  const menuItem = (
    <SidebarMenuItem>
      {item.content != null ? <CollapsibleTrigger render={button} /> : button}
      {item.action && (
        <SidebarMenuAction
          type="button"
          aria-label={item.action.label}
          title={item.action.label}
          {...item.action.buttonProps}
          onClick={(event) => {
            event.stopPropagation()
            item.action?.buttonProps?.onClick?.(event)
          }}
        >
          {item.action.icon}
        </SidebarMenuAction>
      )}
      {item.content != null && (
        <CollapsibleContent>{item.content}</CollapsibleContent>
      )}
    </SidebarMenuItem>
  )

  return item.content != null ? (
    <Collapsible defaultOpen={item.defaultOpen}>{menuItem}</Collapsible>
  ) : (
    menuItem
  )
}
