import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react"
import { AlertCircle, RotateCcw } from "lucide-react"
import { READER_ERROR_CODES } from "@shiro/ui-shared/api/reader-bridge"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { Skeleton } from "@shiro/ui-shared/components/ui/skeleton"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import type {
  BookFormat,
  EngineCapabilities,
  EphemeralReaderViewState,
  FixedZoomMode,
  FootnotePayload,
  NavigationLocator,
  ReaderDocumentInstance,
  RelocationSnapshot,
} from "../contracts/engine"
import type { SelectionContext } from "../contracts/selection"
import type { CompiledReaderStyle } from "../contracts/settings"
import { readerEngineRegistry } from "../engine/engine-registry"
import { UnsupportedFormatError } from "../engine/unsupported-pdf-engine"
import {
  SelectionActionRegistry,
  selectionActionRegistry,
} from "../selection/action-registry"
import type { SafeViewportInsets } from "../selection/geometry"
import {
  resolveReaderKeyCommand,
  resolveViewportTapCommand,
  type ReaderKeyCommand,
  type ReaderTapCommand,
} from "../workspace/input-commands"
import {
  ExternalLinkConfirmDialog,
  type PendingExternalLink,
} from "./external-link-confirm-dialog"
import { FixedZoomControls } from "./fixed-zoom-controls"
import { FootnotePopover } from "./footnote-popover"
import {
  SelectionToolbar,
  type SelectionToolbarViewportBounds,
} from "./selection-toolbar"

const DEFAULT_FALLBACK_CAPS: EngineCapabilities = {
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

export interface ReaderViewportProps {
  readonly tabId: string
  readonly bookId: string
  readonly bookHash: string
  readonly format: BookFormat
  readonly title: string
  readonly reattachSessionId?: string | null | undefined
  readonly manageSessionLifecycleInFrontend?: boolean | undefined
  readonly keepBackendSessionOnUnmount?: boolean | undefined
  readonly initialLocator?: NavigationLocator | undefined
  readonly initialViewState?: EphemeralReaderViewState | undefined
  readonly compiledStyle: CompiledReaderStyle
  readonly tapZonesEnabled?: boolean | undefined
  readonly safeInsets?: SafeViewportInsets | undefined
  readonly selectionRegistry?: SelectionActionRegistry | undefined
  readonly className?: string | undefined
  readonly onInstanceReady?: ((instance: ReaderDocumentInstance | null) => void) | undefined
  readonly onRelocate?: ((snapshot: RelocationSnapshot) => void) | undefined
  readonly onSessionChange?: ((sessionId: string) => void) | undefined
  readonly onSelectionChange?: ((selection: SelectionContext | null) => void) | undefined
  readonly onToggleChrome?: (() => void) | undefined
  readonly onViewportTap?: ((normX: number, normY: number, command: ReaderTapCommand) => void) | undefined
  readonly onKeyCommand?: ((event: KeyboardEvent, command: ReaderKeyCommand) => void) | undefined
  readonly onExternalLink?: ((url: string) => void) | undefined
  readonly onInternalLink?: ((href: string, before: RelocationSnapshot | null) => void) | undefined
  readonly onUnsupportedFormat?: (() => void) | undefined
  readonly onViewportWidthChange?: ((widthPx: number) => void) | undefined
  readonly onViewportSizeChange?: ((widthPx: number, heightPx: number) => void) | undefined
}

export function ReaderViewport({
  tabId,
  bookId,
  bookHash,
  format,
  title,
  reattachSessionId,
  manageSessionLifecycleInFrontend = true,
  keepBackendSessionOnUnmount = true,
  initialLocator,
  initialViewState,
  compiledStyle,
  tapZonesEnabled = false,
  safeInsets,
  selectionRegistry,
  className,
  onInstanceReady,
  onRelocate,
  onSessionChange,
  onSelectionChange,
  onToggleChrome,
  onViewportTap,
  onKeyCommand,
  onExternalLink,
  onInternalLink,
  onUnsupportedFormat,
  onViewportWidthChange,
  onViewportSizeChange,
}: ReaderViewportProps) {
  const { t } = useTranslation()
  const rootRef = useRef<HTMLDivElement | null>(null)
  const hostRef = useRef<HTMLDivElement | null>(null)
  const instanceRef = useRef<ReaderDocumentInstance | null>(null)
  const [instance, setInstance] = useState<ReaderDocumentInstance | null>(null)
  const [relocation, setRelocation] = useState<RelocationSnapshot | null>(null)
  const [selection, setSelection] = useState<SelectionContext | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [pendingExternalLink, setPendingExternalLink] =
    useState<PendingExternalLink | null>(null)
  const [activeFootnote, setActiveFootnote] = useState<FootnotePayload | null>(
    null
  )
  const [fixedZoom, setFixedZoom] = useState<FixedZoomMode>("fit-page")
  const [viewportBounds, setViewportBounds] =
    useState<SelectionToolbarViewportBounds | null>(null)
  const [openAttempt, setOpenAttempt] = useState(0)

  // Keep latest callbacks and style in refs so open effect only restarts when document identity changes.
  const latestStyleRef = useRef(compiledStyle)
  const tapZonesEnabledRef = useRef(tapZonesEnabled)
  const reattachSessionIdRef = useRef(reattachSessionId)
  const initialLocatorRef = useRef(initialLocator)
  const initialViewStateRef = useRef(initialViewState)
  const titleRef = useRef(title)
  const selectionRef = useRef(selection)
  const selectionRegistryRef = useRef(selectionRegistry)
  const tRef = useRef(t)
  const callbacksRef = useRef({
    onInstanceReady,
    onRelocate,
    onSessionChange,
    onSelectionChange,
    onToggleChrome,
    onViewportTap,
    onKeyCommand,
    onExternalLink,
    onInternalLink,
    onUnsupportedFormat,
  })

  useLayoutEffect(() => {
    latestStyleRef.current = compiledStyle
    tapZonesEnabledRef.current = tapZonesEnabled
    reattachSessionIdRef.current = reattachSessionId
    initialLocatorRef.current = initialLocator
    initialViewStateRef.current = initialViewState
    titleRef.current = title
    selectionRef.current = selection
    selectionRegistryRef.current = selectionRegistry
    tRef.current = t
    callbacksRef.current = {
      onInstanceReady,
      onRelocate,
      onSessionChange,
      onSelectionChange,
      onToggleChrome,
      onViewportTap,
      onKeyCommand,
      onExternalLink,
      onInternalLink,
      onUnsupportedFormat,
    }
  })

  useLayoutEffect(() => {
    const el = rootRef.current
    if (!el) return

    const updateBounds = () => {
      const rect = el.getBoundingClientRect()
      if (rect.width <= 0 || rect.height <= 0) return
      setViewportBounds((prev) =>
        prev &&
        prev.left === rect.left &&
        prev.top === rect.top &&
        prev.width === rect.width &&
        prev.height === rect.height
          ? prev
          : {
              left: rect.left,
              top: rect.top,
              width: rect.width,
              height: rect.height,
            }
      )
    }

    updateBounds()
    window.addEventListener("resize", updateBounds)
    let observer: ResizeObserver | undefined
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(updateBounds)
      observer.observe(el)
    }
    return () => {
      window.removeEventListener("resize", updateBounds)
      observer?.disconnect()
    }
  }, [selection])

  useEffect(() => {
    const el = hostRef.current
    if (!el || (!onViewportWidthChange && !onViewportSizeChange)) return

    const reportSize = () => {
      const rect = el.getBoundingClientRect()
      const width = el.clientWidth || rect.width
      const height = el.clientHeight || rect.height
      if (width > 0) onViewportWidthChange?.(width)
      if (width > 0 && height > 0) onViewportSizeChange?.(width, height)
    }
    reportSize()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(reportSize)
    observer.observe(el)
    return () => observer.disconnect()
  }, [onViewportWidthChange, onViewportSizeChange])

  useEffect(() => {
    const container = hostRef.current
    if (!container) return

    const abortController = new AbortController()
    let activeInstance: ReaderDocumentInstance | null = null
    let disposed = false

    setIsLoading(true)
    setErrorMessage(null)
    setSelection(null)
    setRelocation(null)
    setActiveFootnote(null)
    setFixedZoom("fit-page")
    setInstance(null)
    instanceRef.current = null
    callbacksRef.current.onInstanceReady?.(null)

    void readerEngineRegistry
      .open(format, {
        tabId,
        bookId,
        bookHash,
        fallbackTitle: titleRef.current,
        preloadedNavigation: null,
        reattachSessionId: reattachSessionIdRef.current ?? undefined,
        manageSessionLifecycleInFrontend,
        initialLocator: initialLocatorRef.current,
        initialViewState: initialViewStateRef.current,
        initialStyle: latestStyleRef.current,
        container,
        outerContainer: rootRef.current ?? container,
        signal: abortController.signal,
        events: {
          onRelocate: (snapshot) => {
            if (disposed) return
            setRelocation(snapshot)
            callbacksRef.current.onRelocate?.(snapshot)
          },
          onSelectionChange: (sel) => {
            if (disposed) return
            setSelection(sel)
            callbacksRef.current.onSelectionChange?.(sel)
          },
          onSessionChange: (sid) => {
            if (!disposed) callbacksRef.current.onSessionChange?.(sid)
          },
          onViewportTap: (normX, normY) => {
            if (disposed || !activeInstance) return
            const flow = latestStyleRef.current.paginatorAttributes.flow
            const command = resolveViewportTapCommand(
              normX,
              flow,
              tapZonesEnabledRef.current
            )
            if (command === "go_left") {
              void activeInstance.goLeft()
            } else if (command === "go_right") {
              void activeInstance.goRight()
            } else if (command === "toggle_chrome") {
              callbacksRef.current.onToggleChrome?.()
            }
            callbacksRef.current.onViewportTap?.(normX, normY, command)
          },
          onExternalLink: (url) => {
            if (disposed) return
            if (callbacksRef.current.onExternalLink) {
              callbacksRef.current.onExternalLink(url)
            } else {
              setPendingExternalLink({ url })
            }
          },
          onInternalLink: (href, before) => {
            if (disposed) return
            setActiveFootnote(null)
            callbacksRef.current.onInternalLink?.(href, before)
          },
          onFootnote: (footnote) => {
            if (disposed) return
            setActiveFootnote(footnote)
          },
          onCopySelection: async (ctx) => {
            const reg = selectionRegistryRef.current ?? selectionActionRegistry
            const copyPlugin = reg.getById("builtin.copy")
            if (!copyPlugin) return
            const res = await copyPlugin.execute(ctx)
            if (res.outcome === "dismiss") {
              ctx.clearSelection()
            }
          },
          onKeyCommand: (event) => {
            if (disposed || !activeInstance) return
            const cmd = resolveReaderKeyCommand(event)
            if (cmd === "go_left") {
              void activeInstance.goLeft()
            } else if (cmd === "go_right") {
              void activeInstance.goRight()
            } else if (cmd === "go_prev") {
              void activeInstance.goPrev()
            } else if (cmd === "go_next") {
              void activeInstance.goNext()
            } else if (cmd === "go_start") {
              void activeInstance.goToFraction(0)
            } else if (cmd === "go_end") {
              void activeInstance.goToFraction(1)
            } else if (cmd === "escape" && selectionRef.current) {
              activeInstance.clearSelection()
              setSelection(null)
              event.preventDefault()
              return
            }
            callbacksRef.current.onKeyCommand?.(event, cmd)
          },
          onFatalError: (err) => {
            if (disposed) return
            setErrorMessage(
              err.message || tRef.current("local.reader.error.open_failed")
            )
          },
        },
      })
      .then((opened) => {
        if (disposed || abortController.signal.aborted) {
          void opened.destroy({
            keepBackendSession:
              !manageSessionLifecycleInFrontend || keepBackendSessionOnUnmount,
          })
          return
        }
        activeInstance = opened
        instanceRef.current = opened
        opened.applyStyle(latestStyleRef.current)
        setInstance(opened)
        setIsLoading(false)
        callbacksRef.current.onInstanceReady?.(opened)
      })
      .catch((err: unknown) => {
        if (
          disposed ||
          abortController.signal.aborted ||
          (err instanceof Error && err.name === "AbortError")
        ) {
          return
        }
        const isUnsupported =
          err instanceof UnsupportedFormatError ||
          (err instanceof CommandError &&
            err.payload.code === READER_ERROR_CODES.UNSUPPORTED_FORMAT) ||
          (typeof err === "object" &&
            err !== null &&
            "code" in err &&
            (err as { readonly code?: unknown }).code ===
              READER_ERROR_CODES.UNSUPPORTED_FORMAT)
        if (isUnsupported && callbacksRef.current.onUnsupportedFormat) {
          callbacksRef.current.onUnsupportedFormat()
          return
        }
        const message =
          err instanceof Error && err.message
            ? err.message
            : tRef.current("local.reader.error.open_failed")
        setErrorMessage(message)
        setIsLoading(false)
      })

    return () => {
      disposed = true
      abortController.abort()
      const current = activeInstance
      activeInstance = null
      instanceRef.current = null
      setInstance(null)
      setRelocation(null)
      setSelection(null)
      setActiveFootnote(null)
      callbacksRef.current.onInstanceReady?.(null)
      callbacksRef.current.onSelectionChange?.(null)
      if (current) {
        void current.destroy({
          keepBackendSession:
            !manageSessionLifecycleInFrontend || keepBackendSessionOnUnmount,
        })
      }
    }
  }, [
    tabId,
    bookId,
    bookHash,
    format,
    manageSessionLifecycleInFrontend,
    keepBackendSessionOnUnmount,
    openAttempt,
  ])

  useEffect(() => {
    const active = instanceRef.current
    if (!active) return
    active.applyStyle(compiledStyle)
  }, [compiledStyle])

  const pageNumberLabel = useMemo(() => {
    if (!compiledStyle.showPageNumber || isLoading || errorMessage || !relocation) {
      return null
    }
    const isFixedLayout = instance?.metadata.isFixedLayout === true
    if (!isFixedLayout && compiledStyle.paginatorAttributes.flow === "scrolled") {
      const frac = Number.isFinite(relocation.fraction)
        ? Math.min(1, Math.max(0, relocation.fraction))
        : 0
      return `${Math.round(frac * 100)}%`
    }
    if (relocation.pageStats) {
      return `${relocation.pageStats.current} / ${relocation.pageStats.total}`
    }
    if (isFixedLayout && relocation.sectionTotal > 0) {
      const current = Math.min(
        relocation.sectionTotal,
        Math.max(1, relocation.sectionIndex + 1)
      )
      return `${current} / ${relocation.sectionTotal}`
    }
    return null
  }, [
    compiledStyle.showPageNumber,
    compiledStyle.paginatorAttributes.flow,
    errorMessage,
    instance,
    isLoading,
    relocation,
  ])

  const rootStyle = useMemo<CSSProperties>(() => {
    const vars: Record<string, string> = {
      ...compiledStyle.viewportCssVars,
    }
    return {
      ...(vars as CSSProperties),
      colorScheme: compiledStyle.isDarkSurface ? "dark" : "light",
      backgroundColor: compiledStyle.surfaceBackgroundHex,
      color: compiledStyle.viewportCssVars["--reader-fg"],
      paddingInline: `${compiledStyle.viewportPaddingInlinePx}px`,
    }
  }, [compiledStyle])

  return (
    <div
      ref={rootRef}
      data-reader-root=""
      data-reader-surface={compiledStyle.surface}
      data-dark-surface={compiledStyle.isDarkSurface ? "true" : "false"}
      style={rootStyle}
      className={cn(
        "relative flex h-full w-full min-h-0 min-w-0 flex-1 overflow-hidden select-none",
        className
      )}
    >
      <div
        ref={hostRef}
        data-testid="reader-engine-host"
        className="h-full w-full min-h-0 min-w-0 flex-1"
      />

      {pageNumberLabel !== null && (
        <div
          data-testid="reader-page-number-overlay"
          className="pointer-events-none absolute right-3 bottom-2 z-10 select-none text-[11px] leading-none tabular-nums opacity-60"
          style={{ color: "var(--reader-muted, var(--reader-fg, inherit))" }}
        >
          {pageNumberLabel}
        </div>
      )}

      {!isLoading &&
        !errorMessage &&
        instance?.capabilities.supportsFixedZoom &&
        instance.setFixedZoom && (
          <FixedZoomControls
            zoom={fixedZoom}
            onChangeZoom={(next) => {
              setFixedZoom(next)
              instance.setFixedZoom?.(next)
            }}
          />
        )}

      {isLoading && !errorMessage && (
        <div
          data-testid="reader-loading-skeleton"
          className="pointer-events-none absolute inset-0 flex flex-col justify-center gap-4 p-8"
          style={{ backgroundColor: compiledStyle.surfaceBackgroundHex }}
        >
          <Skeleton className="h-6 w-2/5 opacity-40" />
          <Skeleton className="h-4 w-full opacity-30" />
          <Skeleton className="h-4 w-11/12 opacity-30" />
          <Skeleton className="h-4 w-4/5 opacity-30" />
          <p className="mt-2 text-xs opacity-60">{t("local.reader.loading")}</p>
        </div>
      )}

      {errorMessage && (
        <div
          role="alert"
          className="absolute inset-0 z-20 flex flex-col items-center justify-center gap-3 p-6 text-center"
          style={{ backgroundColor: compiledStyle.surfaceBackgroundHex }}
        >
          <AlertCircle className="size-8 text-destructive" aria-hidden="true" />
          <p className="max-w-md text-sm font-medium">{errorMessage}</p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setOpenAttempt((prev) => prev + 1)}
          >
            <RotateCcw className="size-3.5" aria-hidden="true" />
            {t("local.reader.error.retry")}
          </Button>
        </div>
      )}

      <FootnotePopover
        footnote={activeFootnote}
        viewportBounds={viewportBounds}
        safeInsets={safeInsets}
        viewportCssVars={compiledStyle.viewportCssVars}
        isDarkSurface={compiledStyle.isDarkSurface}
        onClose={() => setActiveFootnote(null)}
        onJumpToSource={(footnote) => {
          setActiveFootnote(null)
          callbacksRef.current.onInternalLink?.(footnote.href, relocation)
          if (footnote.locator && instanceRef.current) {
            void instanceRef.current.goToLocator(footnote.locator)
          }
        }}
        onNavigateLink={(href) => {
          setActiveFootnote(null)
          if (/^(?:https?:|mailto:|tel:)/i.test(href)) {
            if (callbacksRef.current.onExternalLink) {
              callbacksRef.current.onExternalLink(href)
            } else {
              setPendingExternalLink({ url: href })
            }
            return
          }
          const hashIdx = href.indexOf("#")
          const pathPart = hashIdx >= 0 ? href.slice(0, hashIdx) : href
          const fragment = hashIdx >= 0 ? href.slice(hashIdx + 1) : ""
          const active = instanceRef.current
          if (active && pathPart) {
            void active.goToLocator({
              kind: "epub",
              spine_index: null,
              href: pathPart,
              target: fragment
                ? { kind: "fragment", fragment }
                : { kind: "start" },
            })
          }
        }}
      />

      <ExternalLinkConfirmDialog
        link={pendingExternalLink}
        onClose={() => setPendingExternalLink(null)}
        surfaceCssVars={compiledStyle.viewportCssVars}
        isDarkSurface={compiledStyle.isDarkSurface}
      />

      <SelectionToolbar
        selection={selection}
        capabilities={instance?.capabilities ?? DEFAULT_FALLBACK_CAPS}
        registry={selectionRegistry}
        safeInsets={safeInsets}
        viewportBounds={viewportBounds}
        viewportCssVars={compiledStyle.viewportCssVars}
        isDarkSurface={compiledStyle.isDarkSurface}
      />
    </div>
  )
}
