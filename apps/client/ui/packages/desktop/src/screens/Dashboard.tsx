import type { ReactNode } from "react"
import { useRef, useState } from "react"
import {
  ChevronsUpDown,
  Clock,
  HardDrive,
  LibraryBig,
  LoaderCircle,
  LogOut,
  NotebookPen,
  ChartNoAxesCombined,
  Settings,
  Trash,
  User,
} from "lucide-react"
import { Avatar, AvatarFallback } from "@shiro/ui-shared/components/ui/avatar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@shiro/ui-shared/components/ui/dropdown-menu"
import { Separator } from "@shiro/ui-shared/components/ui/separator"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@shiro/ui-shared/components/ui/sidebar"
import { BrandMark } from "@shiro/ui-shared/components/brand-mark"
import { useAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { BookSearch } from "@shiro/ui-shared/features/books/book-search"
import { BookshelfBooks } from "@shiro/ui-shared/features/books/bookshelf-books"
import { DesktopDropUploadOverlay } from "../components/desktop-drop-upload-overlay"
import { TrashList } from "@shiro/ui-shared/features/books/trash-list"
import { SettingsDialog } from "@shiro/ui-shared/features/settings/settings-dialog"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

export function DesktopDashboardScreen() {
  const { t } = useTranslation()
  const { isTauri, bootstrap, cloudLogout } = useAppRuntime()
  const [activeMenu, setActiveMenu] = useState<string>("bookshelf")
  const [headerOverlay, setHeaderOverlay] = useState<HTMLDivElement | null>(
    null
  )
  const [bookSearch, setBookSearch] = useState("")
  const [settingsOpen, setSettingsOpen] = useState(false)

  const menus: DashboardMenu[] = [
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

  const isLocalTauriMode = isTauri && bootstrap?.activeMode === "local"
  const isCloudTauriMode = isTauri && bootstrap?.activeMode === "cloud"

  return (
    <SidebarProvider className="h-dvh overflow-hidden">
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

        <SidebarFooter className="pb-2">
          {isLocalTauriMode ? (
            <UserProfileMenu
              leading={
                <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                  <HardDrive className="size-4" />
                </div>
              }
              username={t("local.dashboard.local_library")}
              subtitle={t("local.mode.local")}
              pendingRestart={bootstrap?.pendingRestart}
              onOpenSettings={() => setSettingsOpen(true)}
            />
          ) : isCloudTauriMode ? (
            <UserProfileMenu
              username={bootstrap.cloud?.user?.username ?? null}
              subtitle={bootstrap.cloud?.origin}
              pendingRestart={bootstrap.pendingRestart}
              onOpenSettings={() => setSettingsOpen(true)}
              onLogout={async () => {
                await cloudLogout()
              }}
            />
          ) : null}
        </SidebarFooter>
      </Sidebar>
      <main className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
        <header className="group/header box-content flex h-14 shrink-0 items-center gap-2 border-b px-4 transition-colors has-data-[slot=selection-bar]:bg-muted/60">
          <div className="flex min-w-0 items-center gap-2 group-has-data-[slot=selection-bar]/header:hidden">
            <SidebarTrigger />
            <Separator
              orientation="vertical"
              className="mx-1 data-vertical:h-4 data-vertical:self-center"
            />
            <h1 className="truncate text-base font-semibold tracking-tight">
              {activeTitle}
            </h1>
          </div>
          <div
            ref={setHeaderOverlay}
            className="hidden min-w-0 flex-1 items-center group-has-data-[slot=selection-bar]/header:flex"
          />
          {activeMenu === "bookshelf" && (
            <BookSearch value={bookSearch} onChange={setBookSearch} />
          )}
        </header>
        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-y-contain">
          {activeMenu === "bookshelf" && (
            <BookshelfBooks
              headerOverlay={headerOverlay}
              search={bookSearch}
              onSearchChange={setBookSearch}
            />
          )}
          {activeMenu === "trash" && <TrashList />}
        </div>
      </main>
      {isTauri && (
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      )}
      <DesktopDropUploadOverlay
        enabled={
          isTauri &&
          bootstrap?.canImportBooks === true &&
          activeMenu === "bookshelf"
        }
      />
    </SidebarProvider>
  )
}

type DashboardMenu = {
  id: string
  name: string
  icon: ReactNode
  isActive?: boolean | undefined
  onSelect?: () => void
}

function DashboardMenuItem({ item }: { item: DashboardMenu }) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        tooltip={item.name}
        isActive={item.isActive}
        aria-current={item.isActive ? "page" : undefined}
        onClick={item.onSelect}
      >
        {item.icon}
        <span>{item.name}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

function UserProfileMenu({
  leading,
  username,
  subtitle,
  pendingRestart,
  onOpenSettings,
  onLogout,
}: {
  leading?: ReactNode
  username: string | null
  subtitle?: string | null | undefined
  pendingRestart?: boolean | undefined
  onOpenSettings: () => void
  onLogout?: () => Promise<void>
}) {
  const { t } = useTranslation()
  const { isLoggingOut, logout: handleLogout } = useLogout(onLogout)
  const initial = username?.trim().charAt(0).toUpperCase() || undefined

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton
                size="lg"
                disabled={isLoggingOut}
                aria-busy={isLoggingOut}
                className="data-popup-open:bg-sidebar-accent data-popup-open:text-sidebar-accent-foreground"
              />
            }
          >
            {leading ?? (
              <Avatar className="size-8 rounded-lg">
                <AvatarFallback className="rounded-lg bg-gradient-to-br from-primary/80 to-primary font-semibold text-primary-foreground">
                  {initial ?? <User className="size-4" />}
                </AvatarFallback>
              </Avatar>
            )}
            <div className="grid flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">
                {username || t("local.common.user")}
              </span>
              {subtitle && (
                <span className="truncate text-xs text-muted-foreground">
                  {subtitle}
                </span>
              )}
            </div>
            {pendingRestart && (
              <span
                className="size-2 shrink-0 rounded-full bg-amber-500"
                title={t("local.dashboard.restart_pending")}
              />
            )}
            {isLoggingOut ? (
              <LoaderCircle
                aria-hidden="true"
                className="ml-auto size-4 animate-spin text-muted-foreground"
              />
            ) : (
              <ChevronsUpDown className="ml-auto size-4 text-muted-foreground" />
            )}
          </DropdownMenuTrigger>
          <DropdownMenuContent
            className="min-w-56"
            side="right"
            align="end"
            sideOffset={8}
          >
            <DropdownMenuGroup>
              <DropdownMenuItem onClick={onOpenSettings}>
                <Settings />
                <span>{t("local.dashboard.settings")}</span>
              </DropdownMenuItem>
              {onLogout && (
                <DropdownMenuItem
                  variant="destructive"
                  onClick={handleLogout}
                  disabled={isLoggingOut}
                >
                  <LogOut />
                  <span>{t("local.dashboard.profile.logout")}</span>
                </DropdownMenuItem>
              )}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}

function useLogout(onLogout?: () => Promise<void>) {
  const submitting = useRef(false)
  const [isLoggingOut, setIsLoggingOut] = useState(false)

  const logout = async () => {
    if (submitting.current || !onLogout) return
    submitting.current = true
    setIsLoggingOut(true)
    try {
      await onLogout()
    } catch {
      // The bridge owns error notifications.
    } finally {
      submitting.current = false
      setIsLoggingOut(false)
    }
  }

  return { isLoggingOut, logout }
}
