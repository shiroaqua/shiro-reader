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
import { BookshelfBooks } from "@/features/books/bookshelf-books"
import { TrashList } from "@/features/books/trash-list"
import { useTranslation } from "@/lib/i18n"
import { useAuth } from "@/context/auth-context"
import { notifyError } from "@/lib/error-notifications"

export function DashboardScreen() {
  const { t } = useTranslation()
  const { username, logout } = useAuth()
  const [activeMenu, setActiveMenu] = useState<string>("bookshelf")

  const menus: DashboardMenu[] = [
    {
      id: "search",
      icon: <Search />,
      name: t("local.dashboard.search"),
    },
    {
      id: "bookshelf",
      icon: <LibraryBig />,
      name: t("local.dashboard.bookshelf"),
      isActive: activeMenu === "bookshelf",
      onSelect: () => {
        setActiveMenu("bookshelf")
      },
    },
    {
      id: "recent",
      icon: <Clock />,
      name: t("local.dashboard.recent"),
    },
    {
      id: "notes",
      icon: <NotebookPen />,
      name: t("local.dashboard.notes"),
    },
    {
      id: "statistics",
      icon: <ChartNoAxesCombined />,
      name: t("local.dashboard.statistics"),
    },
    {
      id: "trash",
      icon: <Trash />,
      name: t("local.dashboard.trash"),
      isActive: activeMenu === "trash",
      onSelect: () => {
        setActiveMenu("trash")
      },
    },
  ]

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarContent>
          <SidebarGroup className="pt-3">
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
      <main className="flex min-w-0 flex-1 flex-col bg-background">
        <header className="flex h-12 items-center gap-2 border-b border-border/50 px-4">
          <SidebarTrigger />
          <div className="h-4 w-px bg-border/60" />
          <h1 className="text-sm font-medium text-foreground">
            {activeMenu === "bookshelf"
              ? t("local.dashboard.bookshelf")
              : activeMenu === "trash"
                ? t("local.dashboard.trash")
                : t("local.dashboard.bookshelf")}
          </h1>
        </header>
        <div className="flex-1 overflow-y-auto">
          {activeMenu === "bookshelf" && <BookshelfBooks />}
          {activeMenu === "trash" && <TrashList />}
        </div>
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
      notifyError({ source: "local", key: "local.auth.logout_failed" })
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
              <AvatarFallback className="rounded-lg bg-gradient-to-br from-primary/80 to-primary">
                <User className="size-4 text-primary-foreground" />
              </AvatarFallback>
            </Avatar>
            <div className="grid flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">
                {username || t("local.common.user")}
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
                <span>{t("local.dashboard.profile.logout")}</span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
