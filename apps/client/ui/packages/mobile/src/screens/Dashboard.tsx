import type { ReactNode } from "react"
import { useRef, useState } from "react"
import {
  ArrowLeft,
  ChartNoAxesCombined,
  Clock,
  EllipsisVertical,
  LibraryBig,
  LoaderCircle,
  LogOut,
  NotebookPen,
  Search,
  Settings,
  Trash,
} from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@shiro/ui-shared/components/ui/dropdown-menu"
import { Separator } from "@shiro/ui-shared/components/ui/separator"
import { useAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { BookSearch } from "@shiro/ui-shared/features/books/book-search"
import { BookshelfBooks } from "@shiro/ui-shared/features/books/bookshelf-books"
import { TrashList } from "@shiro/ui-shared/features/books/trash-list"
import { SettingsDialog } from "@shiro/ui-shared/features/settings/settings-dialog"
import { useBackHandler } from "@shiro/ui-shared/hooks/use-back-handler"
import { BACK_PRIORITY } from "@shiro/ui-shared/lib/back-navigation"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { mobileReaderStore } from "../reader/mobile-reader-store"

export function MobileDashboardScreen() {
  const { t } = useTranslation()
  const { isTauri, bootstrap, cloudLogout } = useAppRuntime()
  const [activeMenu, setActiveMenu] = useState<string>("bookshelf")
  const [headerOverlay, setHeaderOverlay] = useState<HTMLDivElement | null>(
    null
  )
  const [bookSearch, setBookSearch] = useState("")
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)

  const searching = searchOpen && activeMenu === "bookshelf"
  useBackHandler(searching, BACK_PRIORITY.search, () => setSearchOpen(false))
  useBackHandler(activeMenu !== "bookshelf", BACK_PRIORITY.navigation, () =>
    setActiveMenu("bookshelf")
  )

  const menus: MobileMenu[] = [
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
    <div className="flex h-dvh flex-col overflow-hidden bg-background">
      <header className="group/header box-content flex h-14 shrink-0 items-center gap-2 border-b pt-[env(safe-area-inset-top)] pr-[max(1rem,env(safe-area-inset-right))] pl-[max(1rem,env(safe-area-inset-left))] transition-colors has-data-[slot=selection-bar]:bg-muted/60">
        {searching ? (
          <div className="flex min-w-0 flex-1 items-center gap-1 group-has-data-[slot=selection-bar]/header:hidden">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t("local.common.back")}
              onClick={() => setSearchOpen(false)}
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
          <div className="flex min-w-0 flex-1 items-center gap-2 group-has-data-[slot=selection-bar]/header:hidden">
            <h1 className="truncate text-base font-semibold tracking-tight">
              {activeTitle}
            </h1>
          </div>
        )}
        <div
          ref={setHeaderOverlay}
          className="hidden min-w-0 flex-1 items-center group-has-data-[slot=selection-bar]/header:flex"
        />
        {!searching && (
          <div className="flex shrink-0 items-center gap-1 group-has-data-[slot=selection-bar]/header:hidden">
            {activeMenu === "bookshelf" && (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="relative"
                aria-label={t("local.dashboard.search")}
                onClick={() => setSearchOpen(true)}
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
      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-y-contain pl-[env(safe-area-inset-left)] pr-[env(safe-area-inset-right)]">
        {activeMenu === "bookshelf" && (
          <BookshelfBooks
            headerOverlay={headerOverlay}
            search={bookSearch}
            onSearchChange={setBookSearch}
            onSearchTag={(query) => {
              setBookSearch(query)
              setSearchOpen(true)
            }}
            onOpenBook={(book) => {
              mobileReaderStore.openBook(book)
            }}
            onBooksDeleted={(ids) => {
              mobileReaderStore.evictBookIds(ids)
            }}
          />
        )}
        {activeMenu === "trash" && (
          <TrashList
            onBooksPermanentlyDeleted={(ids) => {
              mobileReaderStore.evictBookIds(ids)
            }}
          />
        )}
      </div>
      <MobileNav menus={menus} label={t("local.dashboard.navigation")} />
      {isTauri && (
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      )}
    </div>
  )
}

type MobileMenu = {
  id: string
  name: string
  icon: ReactNode
  isActive?: boolean | undefined
  onSelect?: () => void
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

function MobileAccountMenu({
  username,
  subtitle,
  pendingRestart,
  onOpenSettings,
  onLogout,
}: {
  username: string | null
  subtitle?: string | null | undefined
  pendingRestart?: boolean | undefined
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

function MobileNav({
  menus,
  label,
}: {
  menus: MobileMenu[]
  label: string
}) {
  return (
    <nav
      aria-label={label}
      className="flex shrink-0 border-t bg-sidebar pl-[env(safe-area-inset-left)] pr-[env(safe-area-inset-right)] pb-[env(safe-area-inset-bottom)]"
    >
      {menus.map((item) => (
        <button
          key={item.id}
          type="button"
          aria-current={item.isActive ? "page" : undefined}
          onClick={item.onSelect}
          className="flex h-14 min-w-0 flex-1 flex-col items-center justify-center gap-0.5 px-1 text-[11px] font-medium outline-none transition-colors [&_svg]:size-5 [&_svg]:shrink-0"
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
