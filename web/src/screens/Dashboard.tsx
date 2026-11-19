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
  SidebarHeader,
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
import { Separator } from "@/components/ui/separator"
import { BrandMark } from "@/components/brand-mark"
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

  const activeTitle =
    menus.find((item) => item.id === activeMenu)?.name ??
    t("local.dashboard.bookshelf")

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex h-12 items-center gap-2.5 px-2 group-data-[collapsible=icon]:px-0">
            <BrandMark />
            <span className="truncate text-base font-semibold tracking-tight group-data-[collapsible=icon]:hidden">
              ShiroReader
            </span>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarMenu className="gap-1">
              {menus.map((item) => (
                <DashboardMenuItem key={item.id} item={item} />
              ))}
            </SidebarMenu>
          </SidebarGroup>
        </SidebarContent>

        <SidebarFooter>
          <UserProfileMenu username={username} onLogout={logout} />
        </SidebarFooter>
      </Sidebar>
      <main className="flex min-w-0 flex-1 flex-col bg-background">
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger />
          <Separator orientation="vertical" className="mx-1 data-vertical:h-4 data-vertical:self-center" />
          <h1 className="text-base font-semibold tracking-tight">
            {activeTitle}
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
  const initial = username?.trim().charAt(0).toUpperCase() || undefined

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
                className="data-popup-open:bg-sidebar-accent data-popup-open:text-sidebar-accent-foreground"
              />
            }
          >
            <Avatar className="size-8 rounded-lg">
              <AvatarFallback className="rounded-lg bg-gradient-to-br from-primary/80 to-primary font-semibold text-primary-foreground">
                {initial ?? <User className="size-4" />}
              </AvatarFallback>
            </Avatar>
            <div className="grid flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">
                {username || t("local.common.user")}
              </span>
            </div>
            <ChevronsUpDown className="ml-auto size-4 text-muted-foreground" />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            className="min-w-56"
            side="right"
            align="end"
            sideOffset={8}
          >
            <DropdownMenuGroup>
              <DropdownMenuItem
                variant="destructive"
                onClick={handleLogout}
                disabled={isLoggingOut}
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
