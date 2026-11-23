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
} from "@/components/ui/sidebar"
import {
  ArrowLeft,
  Clock,
  EllipsisVertical,
  HardDrive,
  LibraryBig,
  LoaderCircle,
  NotebookPen,
  Search,
  ChartNoAxesCombined,
  Settings,
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
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { BrandMark } from "@/components/brand-mark"
import { BookshelfBooks } from "@/features/books/bookshelf-books"
import { BookSearch } from "@/features/books/book-search"
import { TrashList } from "@/features/books/trash-list"
import { SettingsDialog } from "@/features/settings/settings-dialog"
import { useAppRuntime } from "@/context/app-runtime-context"
import { useBackHandler } from "@/hooks/use-back-handler"
import { useBottomNavigation } from "@/hooks/use-mobile"
import { BACK_PRIORITY } from "@/lib/back-navigation"
import { useTranslation } from "@/lib/i18n"
import { cn } from "cn"

export function DashboardScreen() {
  const { t } = useTranslation()
  const { isTauri, bootstrap, cloudLogout } = useAppRuntime()
  const [activeMenu, setActiveMenu] = useState<string>("bookshelf")
  const [headerOverlay, setHeaderOverlay] = useState<HTMLDivElement | null>(
    null
  )
  const [bookSearch, setBookSearch] = useState("")
  const [settingsOpen, setSettingsOpen] = useState(false)
  const useBottomNav = useBottomNavigation()
  const [mobileSearchOpen, setMobileSearchOpen] = useState(false)
  const searchingOnMobile =
    useBottomNav && mobileSearchOpen && activeMenu === "bookshelf"
  useBackHandler(searchingOnMobile, BACK_PRIORITY.search, () =>
    setMobileSearchOpen(false)
  )
  useBackHandler(activeMenu !== "bookshelf", BACK_PRIORITY.navigation, () =>
    setActiveMenu("bookshelf")
  )

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

  const accountMenu = isLocalTauriMode ? (
    <MobileAccountMenu
      username={t("local.dashboard.local_library")}
      subtitle={t("local.mode.local")}
      pendingRestart={bootstrap?.pendingRestart}
      onOpenSettings={() => setSettingsOpen(true)}
    />
  ) : isCloudTauriMode ? (
    <MobileAccountMenu
      username={bootstrap.cloud?.user?.username ?? null}
      subtitle={bootstrap.cloud?.origin}
      pendingRestart={bootstrap.pendingRestart}
      onOpenSettings={() => setSettingsOpen(true)}
      onLogout={async () => {
        await cloudLogout()
      }}
    />
  ) : null

  return (
    <SidebarProvider className="h-dvh overflow-hidden">
      {!useBottomNav && (
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex h-12 items-center gap-2.5 mt-[env(safe-area-inset-top)] px-2 group-data-[collapsible=icon]:px-0">
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

        <SidebarFooter className="pb-[max(0.5rem,env(safe-area-inset-bottom))]">
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
      )}
      <main className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
        <header className="group/header box-content flex h-14 shrink-0 pt-[env(safe-area-inset-top)] pl-[max(1rem,env(safe-area-inset-left))] pr-[max(1rem,env(safe-area-inset-right))] items-center gap-2 border-b transition-colors has-data-[slot=selection-bar]:bg-muted/60">
          {searchingOnMobile ? (
            <div className="flex min-w-0 flex-1 items-center gap-1 group-has-data-[slot=selection-bar]/header:hidden">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={t("local.common.back")}
                onClick={() => setMobileSearchOpen(false)}
              >
                <ArrowLeft aria-hidden="true" />
              </Button>
              <BookSearch
                value={bookSearch}
                onChange={setBookSearch}
                autoFocus
                fullWidth
              />
            </div>
          ) : (
            <div
              className={cn(
                "flex min-w-0 items-center gap-2 group-has-data-[slot=selection-bar]/header:hidden",
                useBottomNav && "flex-1"
              )}
            >
              {!useBottomNav && (
                <>
                  <SidebarTrigger />
                  <Separator
                    orientation="vertical"
                    className="mx-1 data-vertical:h-4 data-vertical:self-center"
                  />
                </>
              )}
              <h1 className="truncate text-base font-semibold tracking-tight">
                {activeTitle}
              </h1>
            </div>
          )}
          <div
            ref={setHeaderOverlay}
            className="hidden min-w-0 flex-1 items-center group-has-data-[slot=selection-bar]/header:flex"
          />
          {!useBottomNav && activeMenu === "bookshelf" && (
            <BookSearch value={bookSearch} onChange={setBookSearch} />
          )}
          {useBottomNav && !searchingOnMobile && (
            <div className="flex shrink-0 items-center gap-1 group-has-data-[slot=selection-bar]/header:hidden">
              {activeMenu === "bookshelf" && (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="relative"
                  aria-label={t("local.dashboard.search")}
                  onClick={() => setMobileSearchOpen(true)}
                >
                  <Search aria-hidden="true" />
                  {bookSearch !== "" && (
                    <span
                      aria-hidden="true"
                      className="absolute top-2 right-2 size-2 rounded-full bg-primary"
                    />
                  )}
                </Button>
              )}
              {accountMenu}
            </div>
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
        {useBottomNav && (
          <MobileNav menus={menus} label={t("local.dashboard.navigation")} />
        )}
      </main>
      {isTauri && (
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      )}
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
  subtitle?: string | null
  pendingRestart?: boolean
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

/** Phone replacement for the sidebar footer: account info, settings, logout. */
function MobileAccountMenu({
  username,
  subtitle,
  pendingRestart,
  onOpenSettings,
  onLogout,
}: {
  username: string | null
  subtitle?: string | null
  pendingRestart?: boolean
  onOpenSettings: () => void
  onLogout?: () => Promise<void>
}) {
  const { t } = useTranslation()
  const { isLoggingOut, logout } = useLogout(onLogout)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            className="relative"
            aria-label={t("local.dashboard.more")}
            disabled={isLoggingOut}
            aria-busy={isLoggingOut}
          />
        }
      >
        {isLoggingOut ? (
          <LoaderCircle aria-hidden="true" className="animate-spin" />
        ) : (
          <EllipsisVertical aria-hidden="true" />
        )}
        {pendingRestart && (
          <span
            aria-hidden="true"
            className="absolute top-2 right-2 size-2 rounded-full bg-amber-500"
          />
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent className="min-w-56" align="end" sideOffset={4}>
        <div className="grid gap-0.5 px-2 py-1.5">
          <span className="truncate text-sm font-medium">
            {username || t("local.common.user")}
          </span>
          {subtitle && (
            <span className="truncate text-xs text-muted-foreground">
              {subtitle}
            </span>
          )}
          {pendingRestart && (
            <span className="text-xs text-amber-600">
              {t("local.dashboard.restart_pending")}
            </span>
          )}
        </div>
        <Separator className="my-1" />
        <DropdownMenuGroup>
          <DropdownMenuItem onClick={onOpenSettings}>
            <Settings />
            <span>{t("local.dashboard.settings")}</span>
          </DropdownMenuItem>
          {onLogout && (
            <DropdownMenuItem
              variant="destructive"
              onClick={logout}
              disabled={isLoggingOut}
            >
              <LogOut />
              <span>{t("local.dashboard.profile.logout")}</span>
            </DropdownMenuItem>
          )}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** Phone bottom navigation; mirrors the sidebar entries. */
function MobileNav({
  menus,
  label,
}: {
  menus: DashboardMenu[]
  label: string
}) {
  return (
    <nav
      aria-label={label}
      className="flex shrink-0 border-t bg-sidebar pb-[env(safe-area-inset-bottom)]"
    >
      {menus.map((item) => (
        <button
          key={item.id}
          type="button"
          aria-current={item.isActive ? "page" : undefined}
          onClick={item.onSelect}
          className={cn(
            "flex h-14 min-w-0 flex-1 flex-col items-center justify-center gap-0.5 px-1 text-[11px] font-medium outline-none transition-colors [&_svg]:size-5 [&_svg]:shrink-0"
          )}
          style={{
            color: item.isActive
              ? "var(--mobile-nav-active)"
              : "var(--mobile-nav-inactive)",
          }}
        >
          {item.icon}
          <span className="max-w-full truncate">{item.name}</span>
        </button>
      ))}
    </nav>
  )
}
