import { useEffect, useRef } from "react"
import { ArrowLeft, BookOpen, List, SlidersHorizontal, X } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { ProgressScrubber } from "@shiro/ui-shared/features/reader/components/progress-scrubber"
import type { ReaderPageStats } from "@shiro/ui-shared/features/reader/contracts/engine"
import type { ReaderTabSession } from "@shiro/ui-shared/features/reader/workspace/tab-session-store"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"

export interface MobileReaderChromeProps {
  readonly visible: boolean
  readonly tabs: readonly ReaderTabSession[]
  readonly activeTabId: string | null
  readonly fraction: number
  readonly pageStats: ReaderPageStats | null
  readonly activeChapterTitle?: string | null | undefined
  readonly canGoBack: boolean
  readonly backFraction?: number | null | undefined
  readonly disabled?: boolean | undefined
  readonly onBackToLibrary: () => void
  readonly onSelectTab: (tabId: string) => void
  readonly onCloseTab: (tabId: string) => void
  readonly onOpenToc: () => void
  readonly onOpenSettings: () => void
  readonly onCommitFraction: (fraction: number) => void
  readonly onGoBack: () => void
}

export function MobileReaderChrome({
  visible,
  tabs,
  activeTabId,
  fraction,
  pageStats,
  activeChapterTitle,
  canGoBack,
  backFraction,
  disabled = false,
  onBackToLibrary,
  onSelectTab,
  onCloseTab,
  onOpenToc,
  onOpenSettings,
  onCommitFraction,
  onGoBack,
}: MobileReaderChromeProps) {
  const { t } = useTranslation()
  const tabsScrollRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!visible) return
    const activeEl = tabsScrollRef.current?.querySelector<HTMLElement>(
      '[aria-selected="true"]'
    )
    if (activeEl && typeof activeEl.scrollIntoView === "function") {
      activeEl.scrollIntoView({ block: "nearest", inline: "nearest" })
    }
  }, [visible, activeTabId, tabs.length])

  if (!visible) return null

  return (
    <>
      <header
        data-reader-chrome="top"
        style={{
          paddingTop: "var(--shiro-safe-top, env(safe-area-inset-top, 0px))",
          paddingLeft: "var(--shiro-safe-left, env(safe-area-inset-left, 0px))",
          paddingRight: "var(--shiro-safe-right, env(safe-area-inset-right, 0px))",
          backgroundColor: "var(--reader-chrome-bg, var(--background))",
          borderColor: "var(--reader-chrome-border, var(--border))",
          color: "var(--reader-fg, inherit)",
        }}
        className="pointer-events-auto absolute inset-x-0 top-0 z-20 border-b backdrop-blur-md"
      >
        <div className="flex h-11 items-center gap-1.5 px-2 select-none">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="shrink-0"
            aria-label={t("local.reader.chrome.back_to_library")}
            onClick={onBackToLibrary}
          >
            <ArrowLeft aria-hidden="true" className="size-4" />
          </Button>
          <div
            ref={tabsScrollRef}
            role="tablist"
            aria-label={t("local.reader.tabs.label")}
            className="flex h-full min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none]"
          >
            {tabs.map((tab) => {
              const isActive = tab.tabId === activeTabId
              return (
                <div
                  key={tab.tabId}
                  role="tab"
                  tabIndex={0}
                  aria-selected={isActive}
                  data-reader-tab-id={tab.tabId}
                  onClick={() => onSelectTab(tab.tabId)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault()
                      onSelectTab(tab.tabId)
                    }
                  }}
                  className={cn(
                    "group flex h-7 max-w-48 min-w-28 shrink-0 cursor-pointer items-center gap-1.5 rounded-md border px-2 text-xs transition-colors",
                    isActive
                      ? "border-primary/40 bg-background/80 font-medium shadow-xs"
                      : "border-transparent opacity-75 active:bg-[var(--reader-hover-bg,rgba(0,0,0,0.05))] active:opacity-100"
                  )}
                >
                  <BookOpen
                    className="size-3.5 shrink-0 opacity-70"
                    aria-hidden="true"
                  />
                  <span className="min-w-0 flex-1 truncate">{tab.title}</span>
                  <button
                    type="button"
                    aria-label={`${t("local.reader.tabs.close")}: ${tab.title}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      onCloseTab(tab.tabId)
                    }}
                    className="rounded p-0.5 opacity-60 active:bg-[var(--reader-hover-strong-bg,rgba(0,0,0,0.1))] active:opacity-100"
                  >
                    <X className="size-3" aria-hidden="true" />
                  </button>
                </div>
              )
            })}
          </div>
        </div>
      </header>

      <footer
        data-reader-chrome="bottom"
        style={{
          paddingBottom:
            "var(--shiro-safe-bottom, env(safe-area-inset-bottom, 0px))",
          paddingLeft: "var(--shiro-safe-left, env(safe-area-inset-left, 0px))",
          paddingRight: "var(--shiro-safe-right, env(safe-area-inset-right, 0px))",
          backgroundColor: "var(--reader-chrome-bg, var(--background))",
          borderColor: "var(--reader-chrome-border, var(--border))",
          color: "var(--reader-fg, inherit)",
        }}
        className="pointer-events-auto absolute inset-x-0 bottom-0 z-20 border-t backdrop-blur-md"
      >
        <ProgressScrubber
          fraction={fraction}
          pageStats={pageStats}
          activeChapterTitle={activeChapterTitle}
          canGoBack={canGoBack}
          backFraction={backFraction}
          disabled={disabled}
          onCommitFraction={onCommitFraction}
          onGoBack={onGoBack}
        />
        <div className="grid grid-cols-2 gap-2 px-3 pt-1 pb-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onOpenToc}
            className="justify-center gap-1.5"
          >
            <List aria-hidden="true" className="size-4" />
            <span>{t("local.reader.toc.title")}</span>
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onOpenSettings}
            className="justify-center gap-1.5"
          >
            <SlidersHorizontal aria-hidden="true" className="size-4" />
            <span>{t("local.reader.settings.title")}</span>
          </Button>
        </div>
      </footer>
    </>
  )
}
