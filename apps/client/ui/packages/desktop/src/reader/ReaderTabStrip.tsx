import type { CSSProperties } from "react"
import { Fragment, useEffect, useRef, useState } from "react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import {
  BookOpen,
  Copy,
  List,
  Maximize2,
  Minimize2,
  Minus,
  SlidersHorizontal,
  X,
} from "lucide-react"
import type {
  OtherReaderWindowSummary,
  ReaderWindowTabDto,
  TabDropPreviewEvent,
} from "@shiro/ui-shared/api/reader-bridge"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@shiro/ui-shared/components/ui/context-menu"
import { Separator } from "@shiro/ui-shared/components/ui/separator"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import { useTabDrag } from "./use-tab-drag"

export interface ReaderTabStripProps {
  readonly windowId: string
  readonly tabs: readonly ReaderWindowTabDto[]
  readonly activeTabId: string | null
  readonly otherWindows: readonly OtherReaderWindowSummary[]
  readonly canCrossWindowPointerHitTest: boolean
  readonly dropPreview: TabDropPreviewEvent | null
  readonly tocOpen: boolean
  readonly settingsOpen: boolean
  readonly zenMode: boolean
  readonly onActivateTab: (tabId: string) => void
  readonly onReorderTabs: (tabIds: string[]) => void
  readonly onCloseTab: (tabId: string) => void
  readonly onCloseOtherTabs: (tabId: string) => void
  readonly onCloseRightTabs: (tabId: string) => void
  readonly onMoveTabToWindow: (tabId: string, targetWindowId: string) => void
  readonly onTearOffTab: (tabId: string) => void
  readonly onDragStart: (windowId: string, tabId: string) => void
  readonly onDragOver: (windowId: string, clientX: number, clientY: number) => void
  readonly onDragCommit: (windowId: string, clientX: number, clientY: number) => void
  readonly onDragCancel: (windowId: string) => void
  readonly onReportMetrics: (heightPx: number, dragWidthPx: number) => void
  readonly onToggleToc: () => void
  readonly onToggleSettings: () => void
  readonly onToggleZen: () => void
  readonly surfaceCssVars?: Readonly<Record<string, string>> | undefined
  readonly isDarkSurface?: boolean | undefined
  readonly onContextMenuOpenChange?: ((open: boolean) => void) | undefined
  readonly onCloseWindowFallback?: (() => void) | undefined
}

export function ReaderTabStrip({
  windowId,
  tabs,
  activeTabId,
  otherWindows,
  canCrossWindowPointerHitTest,
  dropPreview,
  tocOpen,
  settingsOpen,
  zenMode,
  onActivateTab,
  onReorderTabs,
  onCloseTab,
  onCloseOtherTabs,
  onCloseRightTabs,
  onMoveTabToWindow,
  onTearOffTab,
  onDragStart,
  onDragOver,
  onDragCommit,
  onDragCancel,
  onReportMetrics,
  onToggleToc,
  onToggleSettings,
  onToggleZen,
  surfaceCssVars,
  isDarkSurface,
  onContextMenuOpenChange,
  onCloseWindowFallback,
}: ReaderTabStripProps) {
  const { t } = useTranslation()
  const stripRef = useRef<HTMLDivElement | null>(null)
  const controlsRef = useRef<HTMLDivElement | null>(null)
  const tabsScrollRef = useRef<HTMLDivElement | null>(null)
  const [isMaximized, setIsMaximized] = useState(false)
  const inTauri = isTauriRuntime()

  const {
    dragPhase,
    draggingTabId,
    orderedTabs,
    handleTabPointerDown,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel,
  } = useTabDrag({
    windowId,
    tabs,
    canCrossWindowPointerHitTest,
    stripRef,
    onActivateTab,
    onReorderTabs,
    onDragStart,
    onDragOver,
    onDragCommit,
    onDragCancel,
    onTearOffTab: (_wid, tabId) => onTearOffTab(tabId),
  })

  useEffect(() => {
    const stripEl = stripRef.current
    if (!stripEl) return

    const report = () => {
      const rect = stripEl.getBoundingClientRect()
      const tabsRect = tabsScrollRef.current?.getBoundingClientRect()
      const controlsWidth =
        controlsRef.current?.getBoundingClientRect().width ?? 0
      const height = rect.height > 0 ? rect.height : 42
      const rawDragWidth =
        tabsRect && tabsRect.width > 0 && tabsRect.right > rect.left
          ? tabsRect.right - rect.left
          : rect.width - controlsWidth - 16
      const dragWidth = Math.max(120, rawDragWidth)
      onReportMetrics(height, dragWidth)
    }

    report()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(report)
    observer.observe(stripEl)
    if (tabsScrollRef.current) observer.observe(tabsScrollRef.current)
    if (controlsRef.current) observer.observe(controlsRef.current)
    return () => observer.disconnect()
  }, [onReportMetrics])

  useEffect(() => {
    if (!inTauri) return
    let active = true
    let unlisten: (() => void) | undefined
    const win = getCurrentWindow()

    const syncMaximized = async () => {
      try {
        const max = await win.isMaximized()
        if (active) setIsMaximized(max)
      } catch {
        // Ignore in mocked/headless environments.
      }
    }

    void syncMaximized()
    void win
      .onResized(() => {
        void syncMaximized()
      })
      .then((fn) => {
        if (active) unlisten = fn
        else fn()
      })
      .catch(() => {})

    return () => {
      active = false
      unlisten?.()
    }
  }, [inTauri])

  const handleToggleMaximize = () => {
    if (!inTauri) return
    void getCurrentWindow()
      .toggleMaximize()
      .then(async () => {
        const max = await getCurrentWindow().isMaximized()
        setIsMaximized(max)
      })
      .catch(() => {})
  }

  const handleMinimize = () => {
    if (!inTauri) return
    void getCurrentWindow().minimize().catch(() => {})
  }

  const handleCloseWindow = () => {
    if (inTauri) {
      void getCurrentWindow().close().catch(() => {})
    } else {
      onCloseWindowFallback?.()
    }
  }

  const previewInsertIndex =
    dropPreview && dropPreview.windowId === windowId
      ? dropPreview.insertIndex
      : null

  return (
    <header
      ref={stripRef}
      data-tauri-drag-region=""
      className="relative flex h-10 shrink-0 items-center justify-between gap-2 border-b px-2 select-none"
      style={{
        backgroundColor: "var(--reader-chrome-bg, var(--background))",
        borderColor: "var(--reader-chrome-border, var(--border))",
        color: "var(--reader-fg, var(--foreground))",
      }}
    >
      <div
        ref={tabsScrollRef}
        role="tablist"
        aria-label={t("local.reader.tabs.label")}
        data-tauri-drag-region=""
        onWheel={(event) => {
          const el = tabsScrollRef.current
          if (!el || Math.abs(event.deltaY) < 1) return
          el.scrollLeft += event.deltaY
        }}
        className="flex h-full min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none]"
      >
        {orderedTabs.map((tab, index) => {
          const isActive = tab.tabId === activeTabId
          const isDragging = tab.tabId === draggingTabId
          const showPreviewBefore = previewInsertIndex === index

          return (
            <Fragment key={tab.tabId}>
              {showPreviewBefore && (
                <div
                  data-testid="reader-tab-drop-slot"
                  className="h-7 w-1 shrink-0 rounded-full bg-primary transition-all"
                />
              )}

              <ContextMenu onOpenChange={onContextMenuOpenChange}>
                <ContextMenuTrigger
                  render={
                    <div
                      role="tab"
                      tabIndex={0}
                      aria-selected={isActive}
                      data-reader-tab-id={tab.tabId}
                      onPointerDown={(event) =>
                        handleTabPointerDown(tab.tabId, event)
                      }
                      onMouseDown={(event) => {
                        // Suppress Windows WebView2 middle-click autoscroll icon before auxclick closes the tab.
                        if (event.button === 1) {
                          event.preventDefault()
                        }
                      }}
                      onPointerMove={handlePointerMove}
                      onPointerUp={handlePointerUp}
                      onPointerCancel={handlePointerCancel}
                      onAuxClick={(event) => {
                        if (event.button === 1) {
                          event.preventDefault()
                          onCloseTab(tab.tabId)
                        }
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault()
                          onActivateTab(tab.tabId)
                        }
                      }}
                      className={cn(
                        "group flex h-7 max-w-48 min-w-28 shrink-0 cursor-pointer items-center gap-1.5 rounded-md border px-2 text-xs transition-colors",
                        isActive
                          ? "border-primary/40 bg-background/80 font-medium shadow-xs"
                          : "border-transparent opacity-75 hover:bg-[var(--reader-hover-bg,rgba(0,0,0,0.05))] hover:opacity-100",
                        isDragging &&
                          dragPhase !== "pending" &&
                          "opacity-50 ring-1 ring-primary"
                      )}
                    />
                  }
                >
                  <BookOpen
                    className="size-3.5 shrink-0 opacity-70"
                    aria-hidden="true"
                  />
                  <span className="min-w-0 flex-1 truncate">{tab.title}</span>
                  <button
                    type="button"
                    aria-label={`${t("local.reader.tabs.close")}: ${tab.title}`}
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={(event) => {
                      event.stopPropagation()
                      onCloseTab(tab.tabId)
                    }}
                    className="rounded p-0.5 opacity-60 hover:bg-[var(--reader-hover-strong-bg,rgba(0,0,0,0.1))] hover:opacity-100"
                  >
                    <X className="size-3" aria-hidden="true" />
                  </button>
                </ContextMenuTrigger>

                <ContextMenuContent
                  style={
                    surfaceCssVars
                      ? {
                          ...(surfaceCssVars as CSSProperties),
                          colorScheme: isDarkSurface ? "dark" : "light",
                        }
                      : undefined
                  }
                  data-dark-surface={
                    isDarkSurface === undefined
                      ? undefined
                      : isDarkSurface
                        ? "true"
                        : "false"
                  }
                >
                  <ContextMenuItem
                    disabled={tabs.length <= 1}
                    onClick={() => onTearOffTab(tab.tabId)}
                  >
                    <Copy aria-hidden="true" />
                    {t("local.reader.tabs.move_new_window")}
                  </ContextMenuItem>
                  <Separator className="my-1" />
                  {otherWindows.length === 0 ? (
                    <ContextMenuItem disabled>
                      {t("local.reader.tabs.no_other_windows")}
                    </ContextMenuItem>
                  ) : (
                    otherWindows.map((other) => (
                      <ContextMenuItem
                        key={other.windowId}
                        onClick={() =>
                          onMoveTabToWindow(tab.tabId, other.windowId)
                        }
                      >
                        <span className="truncate">
                          {`${t("local.reader.tabs.move_to_window")}: ${other.activeTitle || other.windowId} (${other.tabCount})`}
                        </span>
                      </ContextMenuItem>
                    ))
                  )}
                  <Separator className="my-1" />
                  <ContextMenuItem onClick={() => onCloseTab(tab.tabId)}>
                    {t("local.reader.tabs.close")}
                  </ContextMenuItem>
                  <ContextMenuItem
                    disabled={tabs.length <= 1}
                    onClick={() => onCloseOtherTabs(tab.tabId)}
                  >
                    {t("local.reader.tabs.close_others")}
                  </ContextMenuItem>
                  <ContextMenuItem
                    disabled={index >= tabs.length - 1}
                    onClick={() => onCloseRightTabs(tab.tabId)}
                  >
                    {t("local.reader.tabs.close_right")}
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
            </Fragment>
          )
        })}

        {previewInsertIndex !== null &&
          previewInsertIndex >= orderedTabs.length && (
            <div
              data-testid="reader-tab-drop-slot"
              className="h-7 w-1 shrink-0 rounded-full bg-primary transition-all"
            />
          )}

        <div
          data-tauri-drag-region=""
          data-testid="reader-tab-drag-spacer"
          className="h-full min-w-8 flex-1"
        />
      </div>

      {dragPhase === "tear_off_preview" && tabs.length > 1 && (
        <div
          data-testid="reader-tear-off-preview-banner"
          className="pointer-events-none absolute top-11 left-1/2 z-50 -translate-x-1/2 rounded-full border bg-popover px-3 py-1 text-xs font-medium text-popover-foreground shadow-md"
        >
          {t("local.reader.tabs.tear_off_hint")}
        </div>
      )}

      <div
        ref={controlsRef}
        className="flex shrink-0 items-center gap-1"
      >
        <Button
          type="button"
          variant={tocOpen ? "secondary" : "ghost"}
          size="icon-xs"
          aria-label={t("local.reader.toc.title")}
          aria-pressed={tocOpen}
          onClick={onToggleToc}
        >
          <List aria-hidden="true" />
        </Button>
        <Button
          type="button"
          variant={settingsOpen ? "secondary" : "ghost"}
          size="icon-xs"
          aria-label={t("local.reader.settings.title")}
          aria-pressed={settingsOpen}
          onClick={onToggleSettings}
        >
          <SlidersHorizontal aria-hidden="true" />
        </Button>
        <Button
          type="button"
          variant={zenMode ? "secondary" : "ghost"}
          size="icon-xs"
          aria-label={
            zenMode
              ? t("local.reader.chrome.zen_exit")
              : t("local.reader.chrome.zen_enter")
          }
          aria-pressed={zenMode}
          onClick={onToggleZen}
        >
          {zenMode ? (
            <Minimize2 aria-hidden="true" />
          ) : (
            <Maximize2 aria-hidden="true" />
          )}
        </Button>

        <Separator orientation="vertical" className="mx-0.5 h-4" />

        {inTauri && (
          <>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("local.reader.window.minimize")}
              onClick={handleMinimize}
            >
              <Minus aria-hidden="true" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={
                isMaximized
                  ? t("local.reader.window.restore")
                  : t("local.reader.window.maximize")
              }
              onClick={handleToggleMaximize}
            >
              {isMaximized ? (
                <Minimize2 aria-hidden="true" />
              ) : (
                <Maximize2 aria-hidden="true" />
              )}
            </Button>
          </>
        )}
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={t("local.reader.window.close")}
          onClick={handleCloseWindow}
        >
          <X aria-hidden="true" />
        </Button>
      </div>
    </header>
  )
}
