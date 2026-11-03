import type { ReactNode } from "react"
import { useRef, useState } from "react"
import {
  Sidebar,
  SidebarContent,
  SidebarMenu,
  SidebarGroup,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarTrigger,
  SidebarProvider,
  SidebarFooter,
  useSidebar,
} from "@/components/ui/sidebar"
import {
  Search,
  Clock,
  LibraryBig,
  Tag,
  NotebookPen,
  ChartNoAxesCombined,
  Trash,
  LogOut,
  User,
  ChevronsUpDown,
} from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { BookshelfBooks } from "@/components/bookshelf-books"
import { useTranslation } from "@/lib/i18n"
import { useAuth } from "@/context/auth-context"
import { notifyError } from "@/lib/error-notifications"

export function DashboardScreen() {
  const { t } = useTranslation()
  const { username, logout } = useAuth()
  const [isRootBookshelfSelected, setIsRootBookshelfSelected] = useState(false)

  const menus: DashboardMenu[] = [
    {
      id: "search",
      icon: <Search />,
      name: t("dashboard.search"),
    },
    {
      id: "bookshelf",
      icon: <LibraryBig />,
      name: t("dashboard.bookshelf"),
      isActive: isRootBookshelfSelected,
      onSelect: () => setIsRootBookshelfSelected(true),
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
        <SidebarContent>
          <SidebarGroup>
            <SidebarMenu>
              {menus.map((item) => (
                <DashboardMenuItem key={item.id} item={item} />
              ))}
            </SidebarMenu>
          </SidebarGroup>
        </SidebarContent>

        <SidebarFooter>
          <SidebarMenu>
            <SidebarMenuItem>
              <UserProfileMenu username={username} onLogout={logout} />
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
      </Sidebar>
      <main className="flex min-w-0 flex-1 flex-col">
          <SidebarTrigger />
        {isRootBookshelfSelected && <BookshelfBooks />}
      </main>
    </SidebarProvider>
  )
}

type DashboardMenu = {
  id: string
  name: string
  icon: ReactNode
  isActive?: boolean
  onSelect?: () => void
}

function DashboardMenuItem({ item }: { item: DashboardMenu }) {
  const { setOpenMobile } = useSidebar()

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        tooltip={item.name}
        isActive={item.isActive}
        aria-current={item.isActive ? "page" : undefined}
        onClick={
          item.onSelect
            ? () => {
                item.onSelect?.()
                setOpenMobile(false)
              }
            : undefined
        }
      >
        {item.icon}
        <span>{item.name}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

function UserProfileMenu({
  username,
  onLogout,
}: {
  username: string | null
  onLogout: () => Promise<void>
}) {
  const { t } = useTranslation()
  const submitting = useRef(false)
  const [isLoggingOut, setIsLoggingOut] = useState(false)

  const handleLogout = async () => {
    if (submitting.current) return
    submitting.current = true
    setIsLoggingOut(true)
    try {
      await onLogout()
    } catch {
      notifyError("auth.logout_failed")
    } finally {
      submitting.current = false
      setIsLoggingOut(false)
    }
  }

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton
                size="lg"
                disabled={isLoggingOut}
                className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
              />
            }
          >
            <Avatar className="h-8 w-8 rounded-lg">
              <AvatarFallback className="rounded-lg bg-gradient-to-br from-blue-400 to-blue-600">
                <User className="size-4 text-white" />
              </AvatarFallback>
            </Avatar>
            <div className="grid flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">
                {username || t("common.user")}
              </span>
            </div>
            <ChevronsUpDown className="ml-auto size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            className="w-(--radix-dropdown-menu-trigger-width) rounded-lg"
            side="right"
            align="end"
            sideOffset={4}
          >
            <DropdownMenuGroup>
              <DropdownMenuItem
                onClick={handleLogout}
                disabled={isLoggingOut}
                className="text-red-600 focus:text-red-600 dark:text-red-400 dark:focus:text-red-400"
              >
                <LogOut />
                <span>{t("dashboard.profile.logout")}</span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
