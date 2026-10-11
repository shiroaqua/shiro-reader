import type { ReactNode } from "react"
import { useState } from "react"
import {
  Sidebar,
  SidebarContent,
  SidebarMenu,
  SidebarGroup,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarTrigger,
  SidebarProvider,
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
} from "lucide-react"
import { BookshelfBooks } from "@/components/bookshelf-books"
import { useTranslation } from "@/lib/i18n"

export function DashboardScreen() {
  const { t } = useTranslation()
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
        onClick={item.onSelect ? () => {
          item.onSelect?.()
          setOpenMobile(false)
        } : undefined}
      >
        {item.icon}
        <span>{item.name}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
