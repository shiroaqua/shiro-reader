import { useCallback, useEffect, useMemo, useState } from "react"
import { readerBridge } from "@shiro/ui-shared/api/reader-bridge"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@shiro/ui-shared/components/ui/sheet"
import { useOptionalAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import {
  ExternalLinkConfirmDialog,
  type PendingExternalLink,
} from "@shiro/ui-shared/features/reader/components/external-link-confirm-dialog"
import { ReaderViewport } from "@shiro/ui-shared/features/reader/components/reader-viewport"
import { ReaderSettingsPanel } from "@shiro/ui-shared/features/reader/components/settings-panel/ReaderSettingsPanel"
import { TocTree } from "@shiro/ui-shared/features/reader/components/toc-tree"
import type {
  EngineCapabilities,
  ReaderDocumentInstance,
  RelocationSnapshot,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import type { SelectionContext } from "@shiro/ui-shared/features/reader/contracts/selection"
import type { SafeViewportInsets } from "@shiro/ui-shared/features/reader/selection/geometry"
import { setAndroidSelectionSuppression } from "@shiro/ui-shared/features/reader/selection/selection-observer"
import {
  shouldUseFontFaceFallback,
  useEffectiveReaderProfile,
} from "@shiro/ui-shared/features/reader/settings/reader-settings-store"
import { compileReaderStyle } from "@shiro/ui-shared/features/reader/settings/style-compiler"
import type { ReaderTabSession } from "@shiro/ui-shared/features/reader/workspace/tab-session-store"
import { useBackHandler } from "@shiro/ui-shared/hooks/use-back-handler"
import { BACK_PRIORITY } from "@shiro/ui-shared/lib/back-navigation"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { DEFAULT_APP_THEME } from "@shiro/ui-shared/lib/theme"
import { MobileReaderChrome } from "./MobileReaderChrome"
import { mobileReaderStore, useMobileReaderStore } from "./mobile-reader-store"

const MOBILE_FALLBACK_CAPS: EngineCapabilities = {
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

function readSafeCssVarPx(name: string): number {
  if (typeof document === "undefined" || typeof getComputedStyle === "undefined") {
    return 0
  }
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim()
  if (!raw) return 0
  const parsed = Number.parseFloat(raw)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0
}

function readSafeViewportInsets(): SafeViewportInsets {
  if (typeof window !== "undefined") {
    try {
      const json = window.__SHIRO_ANDROID_READER__?.getSafeAreaInsetsJson?.()
      if (json) {
        const parsed = JSON.parse(json) as Partial<
          Record<"top" | "right" | "bottom" | "left", unknown>
        >
        const num = (v: unknown): number =>
          typeof v === "number" && Number.isFinite(v) && v > 0 ? v : 0
        return {
          top: num(parsed.top),
          right: num(parsed.right),
          bottom: num(parsed.bottom),
          left: num(parsed.left),
        }
      }
    } catch {
      // Fall back to CSS variables when native JSON getter is unavailable.
    }
  }
  return {
    top: readSafeCssVarPx("--shiro-safe-top"),
    right: readSafeCssVarPx("--shiro-safe-right"),
    bottom: readSafeCssVarPx("--shiro-safe-bottom"),
    left: readSafeCssVarPx("--shiro-safe-left"),
  }
}

function syncAndroidReaderSystemUi(
  immersive: boolean,
  hideSystemBars: boolean,
  isDarkSurface: boolean,
  backgroundColorHex: string
): void {
  if (typeof window === "undefined") return
  try {
    window.__SHIRO_ANDROID_READER__?.setReaderSystemUi?.(
      immersive,
      hideSystemBars,
      isDarkSurface,
      backgroundColorHex
    )
  } catch {
    // Ignore native bridge call failures outside Android WebView.
  }
}

export function MobileReaderOverlay() {
  const readerState = useMobileReaderStore()

  const activeTab = useMemo(() => {
    if (!readerState.isOpen || readerState.tabs.length === 0) return null
    return (
      readerState.tabs.find((tab) => tab.tabId === readerState.activeTabId) ??
      readerState.tabs[0] ??
      null
    )
  }, [readerState])

  if (!readerState.isOpen || !activeTab) {
    return null
  }

  return (
    <MobileReaderOverlayContent
      tabs={readerState.tabs}
      activeTab={activeTab}
    />
  )
}

function MobileReaderOverlayContent({
  tabs,
  activeTab,
}: {
  readonly tabs: readonly ReaderTabSession[]
  readonly activeTab: ReaderTabSession
}) {
  const { t } = useTranslation()
  const runtime = useOptionalAppRuntime()
  const appTheme = runtime?.bootstrap?.theme ?? DEFAULT_APP_THEME

  const [instance, setInstance] = useState<ReaderDocumentInstance | null>(null)
  const [activeSelection, setActiveSelection] =
    useState<SelectionContext | null>(null)
  const [chromeVisible, setChromeVisible] = useState(false)
  const [tocOpen, setTocOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [pendingExternalLink, setPendingExternalLink] =
    useState<PendingExternalLink | null>(null)
  const [viewportSize, setViewportSize] = useState<{
    readonly width: number
    readonly height: number
  }>({ width: 390, height: 844 })
  const [safeInsets, setSafeInsets] = useState<SafeViewportInsets>({
    top: 0,
    right: 0,
    bottom: 0,
    left: 0,
  })

  const { snapshot: settingsSnapshot, effectiveProfile } =
    useEffectiveReaderProfile(activeTab.bookHash, true)

  const compiledStyle = useMemo(
    () =>
      compileReaderStyle(
        effectiveProfile,
        appTheme,
        settingsSnapshot.customFonts,
        settingsSnapshot.fontUrlBase,
        true,
        viewportSize.width,
        {
          isFixedLayout: instance?.metadata.isFixedLayout ?? false,
          useFontFaceFallback: shouldUseFontFaceFallback(
            settingsSnapshot.fontUrlBase
          ),
          viewportHeightPx: viewportSize.height,
        }
      ),
    [
      effectiveProfile,
      appTheme,
      settingsSnapshot.customFonts,
      settingsSnapshot.fontUrlBase,
      viewportSize,
      instance,
    ]
  )

  useEffect(() => {
    syncAndroidReaderSystemUi(
      true,
      !chromeVisible,
      compiledStyle.isDarkSurface,
      compiledStyle.surfaceBackgroundHex
    )
  }, [
    chromeVisible,
    compiledStyle.isDarkSurface,
    compiledStyle.surfaceBackgroundHex,
  ])

  useEffect(() => {
    return () => {
      syncAndroidReaderSystemUi(false, false, false, "#ffffff")
      setAndroidSelectionSuppression(false)
    }
  }, [])

  useEffect(() => {
    const refresh = () => setSafeInsets(readSafeViewportInsets())
    refresh()
    window.addEventListener("resize", refresh)
    window.addEventListener("shiro:safe-area-change", refresh)
    return () => {
      window.removeEventListener("resize", refresh)
      window.removeEventListener("shiro:safe-area-change", refresh)
    }
  }, [])

  const closeOverlayAndResetDrawers = useCallback(() => {
    setTocOpen(false)
    setSettingsOpen(false)
    setChromeVisible(false)
    setActiveSelection(null)
    mobileReaderStore.closeOverlay()
  }, [])

  useBackHandler(activeSelection !== null, BACK_PRIORITY.readerSelection, () => {
    if (!activeSelection) return false
    activeSelection.clearSelection()
    setActiveSelection(null)
    return true
  })

  useBackHandler(true, BACK_PRIORITY.readerWorkspace, () => {
    closeOverlayAndResetDrawers()
    return true
  })

  const capabilities = instance?.capabilities ?? MOBILE_FALLBACK_CAPS
  const activeRelocation = activeTab.lastRelocation ?? null
  const activeFraction = activeRelocation?.fraction ?? 0
  const activeHistory = activeTab.history
  const activeChapterTitle = useMemo(() => {
    const idx = activeRelocation?.activeTocIndex
    if (!instance || idx === null || idx === undefined) return null
    return instance.navigation.toc[idx]?.title ?? null
  }, [instance, activeRelocation])

  const pushHistoryEntry = useCallback(
    (snapshot?: RelocationSnapshot | null) => {
      const reloc = snapshot ?? activeTab.lastRelocation
      if (!reloc) return
      mobileReaderStore.pushHistory(activeTab.tabId, {
        locator: reloc.locator,
        fraction: reloc.fraction,
      })
    },
    [activeTab]
  )

  const pushActiveHistory = useCallback(() => {
    pushHistoryEntry(null)
  }, [pushHistoryEntry])

  const handleViewportSizeChange = useCallback(
    (widthPx: number, heightPx: number) => {
      setViewportSize((prev) =>
        prev.width === widthPx && prev.height === heightPx
          ? prev
          : { width: widthPx, height: heightPx }
      )
    },
    []
  )

  const handleGoBack = useCallback(() => {
    if (!instance) return
    const prev = mobileReaderStore.popHistory(activeTab.tabId)
    if (prev) {
      void instance.goToLocator(prev.locator)
    }
  }, [activeTab.tabId, instance])

  return (
    <div
      data-mobile-reader-overlay=""
      data-dark-surface={compiledStyle.isDarkSurface ? "true" : "false"}
      role="region"
      aria-label={activeTab.title}
      style={{
        backgroundColor: compiledStyle.surfaceBackgroundHex,
        colorScheme: compiledStyle.isDarkSurface ? "dark" : "light",
        ...compiledStyle.viewportCssVars,
      }}
      className="fixed inset-0 z-40 flex flex-col overflow-hidden"
    >
      <div
        style={{
          paddingTop: "var(--shiro-safe-top, env(safe-area-inset-top, 0px))",
          paddingBottom:
            "var(--shiro-safe-bottom, env(safe-area-inset-bottom, 0px))",
          paddingLeft:
            "var(--shiro-safe-left, env(safe-area-inset-left, 0px))",
          paddingRight:
            "var(--shiro-safe-right, env(safe-area-inset-right, 0px))",
        }}
        className="relative min-h-0 min-w-0 flex-1 overflow-hidden"
      >
        <ReaderViewport
          key={activeTab.tabId}
          tabId={activeTab.tabId}
          bookId={activeTab.bookId}
          bookHash={activeTab.bookHash}
          format={activeTab.format}
          title={activeTab.title}
          reattachSessionId={activeTab.sessionId}
          manageSessionLifecycleInFrontend={true}
          keepBackendSessionOnUnmount={true}
          initialLocator={activeTab.lastRelocation?.locator}
          initialViewState={activeTab.viewState}
          compiledStyle={compiledStyle}
          tapZonesEnabled={effectiveProfile.behavior.tapZonesEnabled}
          safeInsets={safeInsets}
          onViewportSizeChange={handleViewportSizeChange}
          onInstanceReady={setInstance}
          onSessionChange={(sid) => {
            const exists = mobileReaderStore
              .getSnapshot()
              .tabs.some((tab) => tab.tabId === activeTab.tabId)
            if (!exists) {
              void readerBridge.epubClose(sid).catch(() => {})
              return
            }
            mobileReaderStore.updateSessionId(activeTab.tabId, sid)
          }}
          onRelocate={(reloc) =>
            mobileReaderStore.updateRelocation(activeTab.tabId, reloc)
          }
          onExternalLink={(url) => setPendingExternalLink({ url })}
          onInternalLink={(_href, before) => {
            pushHistoryEntry(before)
          }}
          onUnsupportedFormat={() => {
            mobileReaderStore.closeTab(activeTab.tabId)
          }}
          onSelectionChange={setActiveSelection}
          onToggleChrome={() => setChromeVisible((prev) => !prev)}
        />
      </div>

      <MobileReaderChrome
        visible={chromeVisible}
        tabs={tabs}
        activeTabId={activeTab.tabId}
        fraction={activeFraction}
        pageStats={activeRelocation?.pageStats ?? null}
        activeChapterTitle={activeChapterTitle}
        canGoBack={activeHistory.length > 0}
        backFraction={activeHistory[activeHistory.length - 1]?.fraction}
        disabled={!instance}
        onBackToLibrary={closeOverlayAndResetDrawers}
        onSelectTab={(tabId) => {
          mobileReaderStore.activateTab(tabId)
        }}
        onCloseTab={(tabId) => {
          mobileReaderStore.closeTab(tabId)
        }}
        onOpenToc={() => setTocOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
        onCommitFraction={(fraction) => {
          if (!instance) return
          pushActiveHistory()
          void instance.goToFraction(fraction)
        }}
        onGoBack={handleGoBack}
      />

      <Sheet open={tocOpen} onOpenChange={setTocOpen}>
        <SheetContent
          side="bottom"
          showCloseButton={false}
          finalFocus={false}
          style={{
            ...(compiledStyle.viewportCssVars as React.CSSProperties),
            colorScheme: compiledStyle.isDarkSurface ? "dark" : "light",
          }}
          data-dark-surface={compiledStyle.isDarkSurface ? "true" : "false"}
          className="max-h-[82dvh] gap-0 overflow-hidden rounded-t-2xl pb-[max(0.75rem,var(--shiro-safe-bottom,env(safe-area-inset-bottom,0px)))]"
        >
          <div
            aria-hidden="true"
            className="mx-auto mt-2 h-1 w-10 shrink-0 rounded-full bg-muted-foreground/25"
          />
          <SheetHeader className="border-b px-4 py-3">
            <SheetTitle className="text-sm font-semibold">
              {t("local.reader.toc.title")}
            </SheetTitle>
          </SheetHeader>
          <TocTree
            toc={instance?.navigation.toc ?? []}
            activeTocIndex={activeRelocation?.activeTocIndex ?? null}
            onSelectNode={(node) => {
              if (!node.locator || !instance) return
              pushActiveHistory()
              void instance.goToLocator(node.locator)
              setTocOpen(false)
            }}
            onExternalLink={(url, title) => {
              setTocOpen(false)
              setPendingExternalLink({ url, title })
            }}
            className="min-h-0 flex-1"
          />
        </SheetContent>
      </Sheet>

      <Sheet open={settingsOpen} onOpenChange={setSettingsOpen}>
        <SheetContent
          side="bottom"
          showCloseButton={false}
          finalFocus={false}
          style={{
            ...(compiledStyle.viewportCssVars as React.CSSProperties),
            colorScheme: compiledStyle.isDarkSurface ? "dark" : "light",
          }}
          data-dark-surface={compiledStyle.isDarkSurface ? "true" : "false"}
          className="max-h-[82dvh] gap-0 overflow-hidden rounded-t-2xl pb-[max(0.75rem,var(--shiro-safe-bottom,env(safe-area-inset-bottom,0px)))]"
        >
          <div
            aria-hidden="true"
            className="mx-auto mt-2 h-1 w-10 shrink-0 rounded-full bg-muted-foreground/25"
          />
          <SheetHeader className="border-b px-4 py-3">
            <SheetTitle className="text-sm font-semibold">
              {t("local.reader.settings.title")}
            </SheetTitle>
          </SheetHeader>
          <ReaderSettingsPanel
            bookHash={activeTab.bookHash}
            capabilities={capabilities}
            isMobile={true}
            surfaceCssVars={compiledStyle.viewportCssVars}
            isDarkSurface={compiledStyle.isDarkSurface}
            className="min-h-0 flex-1 overflow-y-auto"
          />
        </SheetContent>
      </Sheet>

      <ExternalLinkConfirmDialog
        link={pendingExternalLink}
        onClose={() => setPendingExternalLink(null)}
        surfaceCssVars={compiledStyle.viewportCssVars}
        isDarkSurface={compiledStyle.isDarkSurface}
      />
    </div>
  )
}
