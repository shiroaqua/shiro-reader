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
} from "lucide-react"
import { useLibraryController } from "@/hooks/useLibraryController"

export function DashboardScreen() {
  const controller = useLibraryController()
  const menus = [
    {
      icon: <Search />,
      name: "搜索",
    },
    {
      icon: <LibraryBig />,
      name: "书架",
      content: (
        <SidebarMenuSub>
          {controller.shelves.map((bookshelf) => (
            <SidebarMenuSubItem key={bookshelf.id}>
              <SidebarMenuSubButton>
                {bookshelf.name}
              </SidebarMenuSubButton>
            </SidebarMenuSubItem>
          ))}
        </SidebarMenuSub>
      ),
    },
    {
      icon: <Clock />,
      name: "最近",
    },
    {
      icon: <Tag />,
      name: "标签",
    },
    {
      icon: <NotebookPen />,
      name: "笔记",
    },
    {
      icon: <ChartNoAxesCombined />,
      name: "统计",
    },

    {
      icon: <Trash />,
      name: "回收站",
    },
  ]

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarGroup>
          <SidebarContent>
            {menus.map((item) => (
              <SidebarMenu key={item.name}>
                <Collapsible>
                  <SidebarMenuItem>
                    <CollapsibleTrigger
                      render={<SidebarMenuButton tooltip={item.name} />}
                    >
                      {item.icon}
                      <span>{item.name}</span>
                    </CollapsibleTrigger>
                    <CollapsibleContent>{item.content}</CollapsibleContent>
                  </SidebarMenuItem>
                </Collapsible>
              </SidebarMenu>
            ))}
          </SidebarContent>
        </SidebarGroup>
      </Sidebar>
      <main>
        <SidebarTrigger />
      </main>
    </SidebarProvider>
  )
}
