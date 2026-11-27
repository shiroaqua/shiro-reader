import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { X } from "lucide-react"
import {
  readerBridge,
  type ReaderWindowSnapshot,
  type TabDropPreviewEvent,
} from "@shiro/ui-shared/api/reader-bridge"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { ApiErrorAlerts } from "@shiro/ui-shared/components/api-error-alerts"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import {
  ExternalLinkConfirmDialog,
  type PendingExternalLink,
} from "@shiro/ui-shared/features/reader/components/external-link-confirm-dialog"
import { ProgressScrubber } from "@shiro/ui-shared/features/reader/components/progress-scrubber"
import { ReaderViewport } from "@shiro/ui-shared/features/reader/components/reader-viewport"
import { ReaderSettingsPanel } from "@shiro/ui-shared/features/reader/components/settings-panel/ReaderSettingsPanel"
import { TocTree } from "@shiro/ui-shared/features/reader/components/toc-tree"
import type {
  EngineCapabilities,
  ReaderDocumentInstance,
  RelocationSnapshot,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import { compileReaderStyle } from "@shiro/ui-shared/features/reader/settings/style-compiler"
import {
  shouldUseFontFaceFallback,
  useEffectiveReaderProfile,
} from "@shiro/ui-shared/features/reader/settings/reader-settings-store"
import {
  MAX_NAVIGATION_HISTORY,
  type NavigationHistoryEntry,
} from "@shiro/ui-shared/features/reader/workspace/tab-session-store"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { DEFAULT_APP_THEME } from "@shiro/ui-shared/lib/theme"
import { ReaderTabStrip } from "./ReaderTabStrip"

const REFLOW_DEFAULT_CAPS: EngineCapabilities = {
  supportsReflowTypography: true,
  supportsPublisherStyleOverride: true,
  supportsFlowSwitch: true,
  supportsColumnSwitch: true,
  supportsTextSelection: true,
  supportsToc: true,
  supportsFixedZoom: false,
  supportsFullTextTranslation: true,
  supportsBilingualInterleave: true,
}

const STATE_SYNC_THROTTLE_MS = 250

export interface DesktopReaderChromeProps {
  readonly windowId: string
  readonly embedded?: boolean | undefined
  readonly onCloseWindowFallback?: (() => void) | undefined
  readonly onSwitchPreviewWindow?: ((windowId: string) => void) | undefined
}

export function DesktopReaderChrome({
  windowId,
  embedded = false,
  onCloseWindowFallback,
  onSwitchPreviewWindow,
}: DesktopReaderChromeProps) {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const appTheme = runtime?.bootstrap?.theme ?? DEFAULT_APP_THEME

  const [snapshot, setSnapshot] = useState<ReaderWindowSnapshot | null>(null)
  const [dropPreview, setDropPreview] = useState<TabDropPreviewEvent | null>(
    null
  )
  const [tocOpen, setTocOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [zenMode, setZenMode] = useState(false)
  const [zenTopHover, setZenTopHover] = useState(false)
  const [zenBottomHover, setZenBottomHover] = useState(false)
  const [tabContextMenuOpen, setTabContextMenuOpen] = useState(false)
  const [pendingExternalLink, setPendingExternalLink] =
    useState<PendingExternalLink | null>(null)
  const [viewportSize, setViewportSize] = useState<{
    readonly widthPx: number
    readonly heightPx: number
  }>({ widthPx: 1120, heightPx: 760 })

  const [instance, setInstance] = useState<ReaderDocumentInstance | null>(null)
  const [relocationByTab, setRelocationByTab] = useState<
    Readonly<Record<string, RelocationSnapshot>>
  >({})
  const [historyByTab, setHistoryByTab] = useState<
    Readonly<Record<string, readonly NavigationHistoryEntry[]>>
  >({})

  const syncTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingStateSyncMapRef = useRef<Map<string, RelocationSnapshot>>(
    new Map()
  )
  const zenModeRef = useRef(zenMode)
  const topBarHoveredRef = useRef(false)
  const topBarRef = useRef<HTMLDivElement | null>(null)
  const snapshotRef = useRef<ReaderWindowSnapshot | null>(snapshot)

  useLayoutEffect(() => {
    zenModeRef.current = zenMode
    snapshotRef.current = snapshot
  }, [zenMode, snapshot])

  const applyWindowSnapshot = useCallback((next: ReaderWindowSnapshot) => {
    setSnapshot(next)
    const liveIds = new Set(next.tabs.map((tab) => tab.tabId))
    for (const pendingTabId of pendingStateSyncMapRef.current.keys()) {
      if (!liveIds.has(pendingTabId)) {
        pendingStateSyncMapRef.current.delete(pendingTabId)
      }
    }
    setRelocationByTab((prev) => {
      const keys = Object.keys(prev)
      if (!keys.some((k) => !liveIds.has(k))) return prev
      const nextMap: Record<string, RelocationSnapshot> = {}
      for (const k of keys) {
        if (liveIds.has(k)) {
          const val = prev[k]
          if (val) nextMap[k] = val
        }
      }
      return nextMap
    })
    setHistoryByTab((prev) => {
      const keys = Object.keys(prev)
      if (!keys.some((k) => !liveIds.has(k))) return prev
      const nextMap: Record<string, readonly NavigationHistoryEntry[]> = {}
      for (const k of keys) {
        if (liveIds.has(k)) {
          const val = prev[k]
          if (val) nextMap[k] = val
        }
      }
      return nextMap
    })
  }, [])

  const handleViewportSizeChange = useCallback(
    (widthPx: number, heightPx: number) => {
      setViewportSize((prev) =>
        prev.widthPx === widthPx && prev.heightPx === heightPx
          ? prev
          : { widthPx, heightPx }
      )
    },
    []
  )

  const activeTab = useMemo(() => {
    if (!snapshot) return null
    return (
      snapshot.tabs.find((tab) => tab.tabId === snapshot.activeTabId) ??
      snapshot.tabs[0] ??
      null
    )
  }, [snapshot])

  const { snapshot: settingsSnapshot, effectiveProfile } =
    useEffectiveReaderProfile(activeTab?.bookHash ?? "", false)

  const compiledStyle = useMemo(
    () =>
      compileReaderStyle(
        effectiveProfile,
        appTheme,
        settingsSnapshot.customFonts,
        settingsSnapshot.fontUrlBase,
        false,
        viewportSize.widthPx,
        {
          isFixedLayout: instance?.metadata.isFixedLayout ?? false,
          viewportHeightPx: viewportSize.heightPx,
          useFontFaceFallback: shouldUseFontFaceFallback(
            settingsSnapshot.fontUrlBase
          ),
        }
      ),
    [
      effectiveProfile,
      appTheme,
      settingsSnapshot.customFonts,
      settingsSnapshot.fontUrlBase,
      viewportSize.widthPx,
      viewportSize.heightPx,
      instance,
    ]
  )

  const flushPendingTabSync = useCallback(
    async (onlyTabId?: string): Promise<void> => {
      if (onlyTabId !== undefined) {
        const pending = pendingStateSyncMapRef.current.get(onlyTabId)
        pendingStateSyncMapRef.current.delete(onlyTabId)
        if (
          pendingStateSyncMapRef.current.size === 0 &&
          syncTimerRef.current !== null
        ) {
          clearTimeout(syncTimerRef.current)
          syncTimerRef.current = null
        }
        if (!pending) return
        await readerBridge
          .readerUpdateTabState({
            windowId,
            tabId: onlyTabId,
            currentLocator: pending.locator,
            fraction: pending.fraction,
          })
          .catch(() => {})
        return
      }

      if (syncTimerRef.current !== null) {
        clearTimeout(syncTimerRef.current)
        syncTimerRef.current = null
      }
      if (pendingStateSyncMapRef.current.size === 0) return
      const entries = Array.from(pendingStateSyncMapRef.current.entries())
      pendingStateSyncMapRef.current.clear()
      await Promise.all(
        entries.map(([tabId, reloc]) =>
          readerBridge
            .readerUpdateTabState({
              windowId,
              tabId,
              currentLocator: reloc.locator,
              fraction: reloc.fraction,
            })
            .catch(() => {})
        )
      )
    },
    [windowId]
  )

  useEffect(() => {
    let active = true
    let unlistenWindow: (() => void) | undefined
    let unlistenPreview: (() => void) | undefined

    const load = async () => {
      try {
        unlistenWindow = await readerBridge.listenReaderWindowState((next) => {
          if (active && next.windowId === windowId) {
            applyWindowSnapshot(next)
          }
        })
        unlistenPreview = await readerBridge.listenReaderTabDropPreview(
          (preview) => {
            if (active && preview.windowId === windowId) {
              setDropPreview(
                preview.insertIndex === null ? null : preview
              )
            }
          }
        )
        const initial = await readerBridge.readerGetWindowSnapshot(windowId)
        if (active) {
          applyWindowSnapshot(initial)
        }
      } catch {
        // Ignore if window closed mid-fetch.
      }
    }

    void load()
    return () => {
      active = false
      unlistenWindow?.()
      unlistenPreview?.()
      void flushPendingTabSync()
    }
  }, [applyWindowSnapshot, flushPendingTabSync, windowId])

  useEffect(() => {
    if (typeof document === "undefined") return
    document.title = activeTab
      ? `${activeTab.title} - ShiroReader`
      : "ShiroReader"
  }, [activeTab])

  const toggleZenMode = useCallback(() => {
    const next = !zenModeRef.current
    zenModeRef.current = next
    setZenMode(next)
    topBarHoveredRef.current = false
    setZenTopHover(false)
    setZenBottomHover(false)
    if (next) {
      setTocOpen(false)
      setSettingsOpen(false)
    }
    if (isTauriRuntime()) {
      void getCurrentWindow().setFullscreen(next).catch(() => {})
    }
  }, [])

  const handleTabContextMenuOpenChange = useCallback((open: boolean) => {
    setTabContextMenuOpen(open)
    if (!open) {
      let stillHovered: boolean
      try {
        stillHovered = topBarRef.current?.matches(":hover") === true
      } catch {
        stillHovered = false
      }
      topBarHoveredRef.current = stillHovered
      if (zenModeRef.current && !stillHovered) {
        setZenTopHover(false)
      }
    }
  }, [])

  const handleCloseWindowFallback = useCallback(() => {
    const currentTabs = snapshotRef.current?.tabs ?? []
    if (!isTauriRuntime() && currentTabs.length > 0) {
      void (async () => {
        for (const tab of currentTabs) {
          pendingStateSyncMapRef.current.delete(tab.tabId)
          await readerBridge.readerCloseTab(windowId, tab.tabId).catch(() => {})
        }
        onCloseWindowFallback?.()
      })()
      return
    }
    onCloseWindowFallback?.()
  }, [onCloseWindowFallback, windowId])

  useEffect(() => {
    if (typeof window === "undefined") return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "F11") {
        event.preventDefault()
        toggleZenMode()
      } else if (
        event.key === "Escape" &&
        zenMode &&
        !event.defaultPrevented &&
        !tabContextMenuOpen &&
        !pendingExternalLink
      ) {
        toggleZenMode()
      }
    }
    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [pendingExternalLink, tabContextMenuOpen, toggleZenMode, zenMode])

  const scheduleTabRelocationSync = useCallback(
    (tabId: string, relocation: RelocationSnapshot) => {
      pendingStateSyncMapRef.current.set(tabId, relocation)
      if (syncTimerRef.current !== null) return
      syncTimerRef.current = setTimeout(() => {
        syncTimerRef.current = null
        void flushPendingTabSync()
      }, STATE_SYNC_THROTTLE_MS)
    },
    [flushPendingTabSync]
  )

  const handleCloseOtherTabs = async (keepTabId: string) => {
    if (!snapshot) return
    const toClose = snapshot.tabs.filter((tab) => tab.tabId !== keepTabId)
    for (const tab of toClose) {
      pendingStateSyncMapRef.current.delete(tab.tabId)
      try {
        const next = await readerBridge.readerCloseTab(windowId, tab.tabId)
        applyWindowSnapshot(next)
      } catch {
        // Ignore error if tab/window already closed.
      }
    }
  }

  const handleCloseRightTabs = async (tabId: string) => {
    if (!snapshot) return
    const idx = snapshot.tabs.findIndex((t) => t.tabId === tabId)
    if (idx < 0) return
    const toClose = snapshot.tabs.slice(idx + 1)
    for (const tab of toClose) {
      pendingStateSyncMapRef.current.delete(tab.tabId)
      try {
        const next = await readerBridge.readerCloseTab(windowId, tab.tabId)
        applyWindowSnapshot(next)
      } catch {
        // Ignore error if tab/window already closed.
      }
    }
  }

  const handleReportMetrics = useCallback(
    (heightPx: number, dragWidthPx: number) => {
      void readerBridge
        .readerReportTabStripMetrics(windowId, heightPx, dragWidthPx)
        .catch(() => {})
    },
    [windowId]
  )

  const activeRelocation = activeTab
    ? relocationByTab[activeTab.tabId] ?? null
    : null
  const activeFraction = activeRelocation?.fraction ?? activeTab?.fraction ?? 0
  const activeHistory = activeTab ? historyByTab[activeTab.tabId] ?? [] : []
  const activeChapterTitle = useMemo(() => {
    if (!instance || activeRelocation?.activeTocIndex === null || activeRelocation?.activeTocIndex === undefined) {
      return null
    }
    return instance.navigation.toc[activeRelocation.activeTocIndex]?.title ?? null
  }, [instance, activeRelocation])

  const pushTabHistory = (
    tabId: string,
    snapshotOverride?: RelocationSnapshot | null
  ) => {
    const currentLoc =
      snapshotOverride?.locator ??
      relocationByTab[tabId]?.locator ??
      snapshot?.tabs.find((t) => t.tabId === tabId)?.currentLocator ??
      null
    if (!currentLoc) return
    const currentFrac =
      snapshotOverride?.fraction ??
      relocationByTab[tabId]?.fraction ??
      snapshot?.tabs.find((t) => t.tabId === tabId)?.fraction ??
      0

    setHistoryByTab((prev) => {
      const existing = prev[tabId] ?? []
      const next = [...existing, { locator: currentLoc, fraction: currentFrac }]
      if (next.length > MAX_NAVIGATION_HISTORY) {
        next.splice(0, next.length - MAX_NAVIGATION_HISTORY)
      }
      return { ...prev, [tabId]: next }
    })
  }

  const handleGoBack = () => {
    if (!activeTab || !instance) return
    const stack = historyByTab[activeTab.tabId] ?? []
    const last = stack[stack.length - 1]
    if (!last) return
    setHistoryByTab((prev) => ({
      ...prev,
      [activeTab.tabId]: (prev[activeTab.tabId] ?? []).slice(0, -1),
    }))
    void instance.goToLocator(last.locator).catch(() => {})
  }

  const closeTabAndMaybeFallback = useCallback(
    (tabId: string) => {
      pendingStateSyncMapRef.current.delete(tabId)
      void readerBridge
        .readerCloseTab(windowId, tabId)
        .then((next) => {
          applyWindowSnapshot(next)
          if (next.tabs.length === 0) {
            onCloseWindowFallback?.()
          }
        })
        .catch(() => {})
    },
    [applyWindowSnapshot, onCloseWindowFallback, windowId]
  )

  const showTopChrome = !zenMode || zenTopHover
  const showBottomChrome = !zenMode || zenBottomHover
  const capabilities = instance?.capabilities ?? REFLOW_DEFAULT_CAPS

  return (
    <div
      data-reader-root=""
      data-dark-surface={compiledStyle.isDarkSurface ? "true" : "false"}
      style={{
        ...(compiledStyle.viewportCssVars as React.CSSProperties),
        colorScheme: compiledStyle.isDarkSurface ? "dark" : "light",
        backgroundColor: compiledStyle.surfaceBackgroundHex,
        color: compiledStyle.viewportCssVars["--reader-fg"],
      }}
      className="relative flex h-dvh w-screen flex-col overflow-hidden"
    >
      {zenMode && (
        <div
          aria-hidden="true"
          onMouseEnter={() => setZenTopHover(true)}
          className="fixed inset-x-0 top-0 z-40 h-6"
        />
      )}

      {showTopChrome && snapshot && (
        <div
          ref={topBarRef}
          onMouseEnter={() => {
            topBarHoveredRef.current = true
          }}
          onMouseLeave={() => {
            topBarHoveredRef.current = false
            if (zenMode && !tabContextMenuOpen) setZenTopHover(false)
          }}
          className={zenMode ? "fixed inset-x-0 top-0 z-50 shadow-md" : "shrink-0"}
        >
          <ReaderTabStrip
            windowId={windowId}
            tabs={snapshot.tabs}
            activeTabId={snapshot.activeTabId}
            otherWindows={snapshot.otherWindows}
            canCrossWindowPointerHitTest={
              snapshot.canCrossWindowPointerHitTest
            }
            dropPreview={dropPreview}
            tocOpen={tocOpen}
            settingsOpen={settingsOpen}
            zenMode={zenMode}
            surfaceCssVars={compiledStyle.viewportCssVars}
            isDarkSurface={compiledStyle.isDarkSurface}
            onContextMenuOpenChange={handleTabContextMenuOpenChange}
            onActivateTab={(tabId) => {
              void (async () => {
                await flushPendingTabSync()
                const next = await readerBridge.readerActivateTab(
                  windowId,
                  tabId
                )
                applyWindowSnapshot(next)
              })().catch(() => {})
            }}
            onReorderTabs={(tabIds) => {
              void readerBridge
                .readerReorderTabs(windowId, tabIds)
                .then(applyWindowSnapshot)
                .catch(() => {})
            }}
            onCloseTab={closeTabAndMaybeFallback}
            onCloseOtherTabs={(tabId) => {
              void handleCloseOtherTabs(tabId)
            }}
            onCloseRightTabs={(tabId) => {
              void handleCloseRightTabs(tabId)
            }}
            onMoveTabToWindow={(tabId, targetWindowId) => {
              const wasLastTab = (snapshotRef.current?.tabs.length ?? 0) <= 1
              void (async () => {
                await flushPendingTabSync(tabId)
                await readerBridge.readerMoveTabToWindow(
                  windowId,
                  tabId,
                  targetWindowId
                )
                if (!isTauriRuntime() && wasLastTab) {
                  if (onSwitchPreviewWindow) {
                    onSwitchPreviewWindow(targetWindowId)
                  } else {
                    onCloseWindowFallback?.()
                  }
                }
              })().catch(() => {})
            }}
            onTearOffTab={(tabId) => {
              void (async () => {
                await flushPendingTabSync(tabId)
                const nextWin = await readerBridge.readerTearOffTab(
                  windowId,
                  tabId
                )
                if (!isTauriRuntime() && nextWin.windowId !== windowId) {
                  onSwitchPreviewWindow?.(nextWin.windowId)
                }
              })().catch(() => {})
            }}
            onDragStart={(wid, tabId) => {
              void readerBridge.readerDragStart(wid, tabId).catch(() => {})
            }}
            onDragOver={(wid, cx, cy) => {
              void readerBridge.readerDragOver(wid, cx, cy).catch(() => {})
            }}
            onDragCommit={(wid, cx, cy) => {
              void (async () => {
                await flushPendingTabSync()
                await readerBridge.readerDragCommit(wid, cx, cy)
              })().catch(() => {})
            }}
            onDragCancel={(wid) => {
              void readerBridge.readerDragCancel(wid).catch(() => {})
            }}
            onReportMetrics={handleReportMetrics}
            onToggleToc={() => setTocOpen((v) => !v)}
            onToggleSettings={() => setSettingsOpen((v) => !v)}
            onToggleZen={toggleZenMode}
            onCloseWindowFallback={handleCloseWindowFallback}
          />
        </div>
      )}

      {showTopChrome && !snapshot && (
        <header
          style={{
            backgroundColor: "var(--reader-chrome-bg, var(--background))",
            borderColor: "var(--reader-chrome-border, var(--border))",
            color: "var(--reader-fg, var(--foreground))",
          }}
          className={
            zenMode
              ? "fixed inset-x-0 top-0 z-50 flex h-10 items-center justify-between border-b px-2 select-none shadow-md"
              : "flex h-10 shrink-0 items-center justify-between border-b px-2 select-none"
          }
        >
          <div data-tauri-drag-region="" className="h-full flex-1" />
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t("local.reader.window.close")}
            onClick={() => {
              if (isTauriRuntime()) {
                void getCurrentWindow().close().catch(() => {})
              } else {
                onCloseWindowFallback?.()
              }
            }}
          >
            <X aria-hidden="true" />
          </Button>
        </header>
      )}

      <div className="flex min-h-0 flex-1 overflow-hidden">
        {tocOpen && activeTab && (
          <aside
            aria-label={t("local.reader.toc.title")}
            style={{
              backgroundColor: "var(--reader-chrome-bg, var(--background))",
              borderColor: "var(--reader-chrome-border, var(--border))",
            }}
            className="flex w-72 shrink-0 flex-col border-r"
          >
            <div className="flex items-center justify-between border-b px-3 py-2 text-xs font-semibold">
              <span>{t("local.reader.toc.title")}</span>
            </div>
            <TocTree
              toc={instance?.navigation.toc ?? []}
              activeTocIndex={activeRelocation?.activeTocIndex ?? null}
              onSelectNode={(node) => {
                if (!node.locator || !instance) return
                pushTabHistory(activeTab.tabId)
                void instance.goToLocator(node.locator).catch(() => {})
              }}
              onExternalLink={(url, title) =>
                setPendingExternalLink({ url, title })
              }
              className="min-h-0 flex-1"
            />
          </aside>
        )}

        <main className="relative flex min-h-0 min-w-0 flex-1">
          {activeTab ? (
            <ReaderViewport
              key={activeTab.tabId}
              tabId={activeTab.tabId}
              bookId={activeTab.bookId}
              bookHash={activeTab.bookHash}
              format={activeTab.format}
              title={activeTab.title}
              reattachSessionId={activeTab.sessionId}
              manageSessionLifecycleInFrontend={false}
              keepBackendSessionOnUnmount={true}
              initialLocator={
                activeRelocation?.locator ??
                activeTab.currentLocator ??
                undefined
              }
              initialViewState={{
                translationMode: activeTab.translationMode,
                targetLanguage: activeTab.targetLanguage,
              }}
              compiledStyle={compiledStyle}
              tapZonesEnabled={effectiveProfile.behavior.tapZonesEnabled}
              onViewportSizeChange={handleViewportSizeChange}
              onInstanceReady={setInstance}
              onSessionChange={(sid) => {
                void readerBridge
                  .readerUpdateTabState({
                    windowId,
                    tabId: activeTab.tabId,
                    sessionId: sid,
                  })
                  .catch(() => {})
              }}
              onRelocate={(reloc) => {
                setRelocationByTab((prev) => ({
                  ...prev,
                  [activeTab.tabId]: reloc,
                }))
                scheduleTabRelocationSync(activeTab.tabId, reloc)
              }}
              onExternalLink={(url) => setPendingExternalLink({ url })}
              onInternalLink={(_href, before) =>
                pushTabHistory(activeTab.tabId, before)
              }
              onUnsupportedFormat={() =>
                closeTabAndMaybeFallback(activeTab.tabId)
              }
              onKeyCommand={(_event, cmd) => {
                if (cmd === "toggle_zen") {
                  toggleZenMode()
                } else if (
                  cmd === "escape" &&
                  zenMode &&
                  !tabContextMenuOpen &&
                  !pendingExternalLink
                ) {
                  toggleZenMode()
                }
              }}
            />
          ) : (
            <div className="grid flex-1 place-items-center text-sm opacity-60">
              {t("local.reader.empty")}
            </div>
          )}
        </main>

        {settingsOpen && activeTab && (
          <aside
            aria-label={t("local.reader.settings.title")}
            style={{
              backgroundColor: "var(--reader-chrome-bg, var(--background))",
              borderColor: "var(--reader-chrome-border, var(--border))",
            }}
            className="flex w-80 shrink-0 flex-col border-l"
          >
            <div className="flex items-center justify-between border-b px-3 py-2 text-xs font-semibold">
              <span>{t("local.reader.settings.title")}</span>
            </div>
            <ReaderSettingsPanel
              bookHash={activeTab.bookHash}
              capabilities={capabilities}
              isMobile={false}
              surfaceCssVars={compiledStyle.viewportCssVars}
              isDarkSurface={compiledStyle.isDarkSurface}
              className="min-h-0 flex-1"
            />
          </aside>
        )}
      </div>

      {zenMode && (
        <div
          aria-hidden="true"
          onMouseEnter={() => setZenBottomHover(true)}
          className="fixed inset-x-0 bottom-0 z-40 h-6"
        />
      )}

      {showBottomChrome && activeTab && (
        <footer
          onMouseLeave={() => {
            if (zenMode) setZenBottomHover(false)
          }}
          style={{
            backgroundColor: "var(--reader-chrome-bg, var(--background))",
            borderColor: "var(--reader-chrome-border, var(--border))",
          }}
          className={
            zenMode
              ? "fixed inset-x-0 bottom-0 z-50 border-t shadow-md"
              : "shrink-0 border-t"
          }
        >
          <ProgressScrubber
            fraction={activeFraction}
            pageStats={activeRelocation?.pageStats ?? null}
            activeChapterTitle={activeChapterTitle}
            canGoBack={activeHistory.length > 0}
            backFraction={activeHistory[activeHistory.length - 1]?.fraction}
            disabled={!instance}
            onCommitFraction={(fraction) => {
              if (!instance) return
              pushTabHistory(activeTab.tabId)
              void instance.goToFraction(fraction).catch(() => {})
            }}
            onGoBack={handleGoBack}
          />
        </footer>
      )}

      <ExternalLinkConfirmDialog
        link={pendingExternalLink}
        onClose={() => setPendingExternalLink(null)}
        surfaceCssVars={compiledStyle.viewportCssVars}
        isDarkSurface={compiledStyle.isDarkSurface}
      />

      {!embedded && (
        <div
          data-floating-dock=""
          className="pointer-events-none fixed right-4 bottom-16 z-[100] flex max-w-[calc(100vw-2rem)] flex-col items-end gap-2"
        >
          <ApiErrorAlerts />
        </div>
      )}
    </div>
  )
}
