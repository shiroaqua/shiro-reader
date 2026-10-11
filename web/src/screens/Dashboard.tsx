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

export function DashboardScreen() {
  const controller = useLibraryController()
  const [isAddShelfDialogOpen, setIsAddShelfDialogOpen] = useState(false)
  const [isCreatingShelf, setIsCreatingShelf] = useState(false)
  const [createShelfError, setCreateShelfError] = useState<string | null>(null)
  const shelfNameInputId = useId()

  const menus: DashboardMenu[] = [
    {
      id: "search",
      icon: <Search />,
      name: "搜索",
    },
    {
      id: "shelves",
      icon: <LibraryBig />,
      name: "书架",
      action: {
        label: "添加书架",
        icon: <Plus className="size-4" />,
        buttonProps: {
          onClick: () => {
            controller.setNewBookshelfName("")
            setCreateShelfError(null)
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
      name: "最近",
    },
    {
      id: "tags",
      icon: <Tag />,
      name: "标签",
    },
    {
      id: "notes",
      icon: <NotebookPen />,
      name: "笔记",
    },
    {
      id: "statistics",
      icon: <ChartNoAxesCombined />,
      name: "统计",
    },

    {
      id: "trash",
      icon: <Trash />,
      name: "回收站",
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
              setCreateShelfError(null)
              try {
                await controller.createBookshelf()
                setIsAddShelfDialogOpen(false)
              } catch {
                setCreateShelfError("添加书架失败，请重试。")
              } finally {
                setIsCreatingShelf(false)
              }
            }}
          >
            <DialogHeader>
              <DialogTitle>添加书架</DialogTitle>
            </DialogHeader>
            <div className="grid gap-2">
              <Input
                id={shelfNameInputId}
                name="shelfName"
                aria-label="书架名称"
                placeholder="请输入书架名称"
                value={controller.newBookshelfName}
                onChange={(event) => {
                  controller.setNewBookshelfName(event.target.value)
                  setCreateShelfError(null)
                }}
                disabled={isCreatingShelf}
              />
            </div>
            {createShelfError && (
              <p role="alert" className="text-sm text-destructive">
                {createShelfError}
              </p>
            )}
            <DialogFooter>
              <Button
                type="submit"
                disabled={isCreatingShelf || !controller.newBookshelfName.trim()}
              >
                {isCreatingShelf ? "添加中…" : "提交"}
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
