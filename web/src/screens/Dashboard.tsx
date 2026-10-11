import type { ComponentProps, ReactNode } from "react"
import { useId, useState } from "react"
import { Button } from "@/components/ui/button"
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
} from "lucide-react"
import { useLibraryController } from "@/hooks/useLibraryController"
import { ApiError } from "@/api/library"
import { notifyError } from "@/lib/error-notifications"
import { useTranslation } from "@/lib/i18n"

export function DashboardScreen() {
  const controller = useLibraryController()
  const { t } = useTranslation()
  const [isAddShelfDialogOpen, setIsAddShelfDialogOpen] = useState(false)
  const [isCreatingShelf, setIsCreatingShelf] = useState(false)
  const shelfNameInputId = useId()

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
            <SidebarMenuSubItem key={bookshelf.id}>
              <SidebarMenuSubButton>{bookshelf.name}</SidebarMenuSubButton>
            </SidebarMenuSubItem>
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
