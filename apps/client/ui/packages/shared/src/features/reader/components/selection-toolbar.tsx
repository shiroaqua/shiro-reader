import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react"
import { createPortal } from "react-dom"
import { Check } from "lucide-react"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import type { EngineCapabilities } from "../contracts/engine"
import type {
  SelectionActionPlugin,
  SelectionContext,
} from "../contracts/selection"
import {
  SelectionActionRegistry,
  selectionActionRegistry,
} from "../selection/action-registry"
import {
  computeToolbarPosition,
  type SafeViewportInsets,
} from "../selection/geometry"

export interface SelectionToolbarViewportBounds {
  readonly left: number
  readonly top: number
  readonly width: number
  readonly height: number
}

export interface SelectionToolbarProps {
  readonly selection: SelectionContext | null
  readonly capabilities: EngineCapabilities
  readonly registry?: SelectionActionRegistry | undefined
  readonly safeInsets?: SafeViewportInsets | undefined
  readonly viewportBounds?: SelectionToolbarViewportBounds | null | undefined
  readonly viewportCssVars?: Readonly<Record<string, string>> | undefined
  readonly isDarkSurface?: boolean | undefined
  readonly onDismiss?: (() => void) | undefined
}

export function SelectionToolbar({
  selection,
  capabilities,
  registry = selectionActionRegistry,
  safeInsets,
  viewportBounds,
  viewportCssVars,
  isDarkSurface = false,
  onDismiss,
}: SelectionToolbarProps) {
  const { t } = useTranslation()
  const toolbarRef = useRef<HTMLDivElement | null>(null)
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [toolbarSize, setToolbarSize] = useState<{
    readonly width: number
    readonly height: number
  }>({ width: 120, height: 40 })
  const [activeSubpanelId, setActiveSubpanelId] = useState<string | null>(null)
  const [executingId, setExecutingId] = useState<string | null>(null)
  const [copiedToastPos, setCopiedToastPos] = useState<{
    readonly left: number
    readonly top: number
  } | null>(null)

  const [prevSelection, setPrevSelection] = useState<SelectionContext | null>(
    selection
  )
  if (selection !== prevSelection) {
    setPrevSelection(selection)
    setActiveSubpanelId(null)
    setExecutingId(null)
    if (selection !== null) {
      setCopiedToastPos(null)
    }
  }

  useEffect(() => {
    return () => {
      if (copiedTimerRef.current !== null) {
        clearTimeout(copiedTimerRef.current)
        copiedTimerRef.current = null
      }
    }
  }, [])

  const actions = useMemo(() => {
    if (!selection) return []
    return registry.getAvailableActions(selection, capabilities)
  }, [selection, registry, capabilities])

  const activeSubpanelPlugin = useMemo(() => {
    if (!activeSubpanelId) return null
    return actions.find((plugin) => plugin.id === activeSubpanelId) ?? null
  }, [actions, activeSubpanelId])

  useLayoutEffect(() => {
    const el = toolbarRef.current
    if (!el || !selection) return

    const measure = () => {
      const rect = el.getBoundingClientRect()
      const width = rect.width > 0 ? rect.width : el.offsetWidth || 120
      const height = rect.height > 0 ? rect.height : el.offsetHeight || 40
      setToolbarSize((prev) =>
        prev.width === width && prev.height === height
          ? prev
          : { width, height }
      )
    }

    measure()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => {
      observer.disconnect()
    }
  }, [selection, actions.length, activeSubpanelId])

  useEffect(() => {
    if (!selection || typeof document === "undefined") return
    const handlePointerDownOutside = (event: PointerEvent) => {
      const el = toolbarRef.current
      if (!el) return
      if (event.target instanceof Node && el.contains(event.target)) {
        return
      }
      setActiveSubpanelId(null)
      selection.clearSelection()
      onDismiss?.()
    }
    document.addEventListener("pointerdown", handlePointerDownOutside)
    return () => {
      document.removeEventListener("pointerdown", handlePointerDownOutside)
    }
  }, [selection, onDismiss])

  const surfaceVarsStyle = (viewportCssVars ?? {}) as CSSProperties

  if (!selection || actions.length === 0 || typeof document === "undefined") {
    if (copiedToastPos && typeof document !== "undefined") {
      return createPortal(
        <div
          role="status"
          data-dark-surface={isDarkSurface ? "true" : "false"}
          style={{
            ...surfaceVarsStyle,
            ...(isDarkSurface === undefined
              ? undefined
              : { colorScheme: isDarkSurface ? "dark" : "light" }),
            position: "fixed",
            left: `${copiedToastPos.left}px`,
            top: `${copiedToastPos.top}px`,
            zIndex: 120,
            backgroundColor: "var(--reader-chrome-bg, var(--popover))",
            color: "var(--reader-fg, var(--popover-foreground))",
            borderColor: "var(--reader-chrome-border, var(--border))",
          }}
          className="pointer-events-none inline-flex items-center gap-1.5 rounded-xl border px-2.5 py-1.5 text-xs font-medium shadow-lg backdrop-blur-sm select-none"
        >
          <Check className="size-3.5 text-primary" aria-hidden="true" />
          <span>{t("local.reader.selection.copied")}</span>
        </div>,
        document.body
      )
    }
    return null
  }

  const viewportWidth =
    typeof window !== "undefined" ? window.innerWidth : 1024
  const viewportHeight =
    typeof window !== "undefined" ? window.innerHeight : 768
  const resolvedBounds =
    viewportBounds && viewportBounds.width > 0 && viewportBounds.height > 0
      ? viewportBounds
      : { left: 0, top: 0, width: viewportWidth, height: viewportHeight }

  const position = computeToolbarPosition(
    selection.anchor,
    toolbarSize,
    resolvedBounds,
    safeInsets,
    { width: viewportWidth, height: viewportHeight }
  )

  const handleActionClick = async (plugin: SelectionActionPlugin) => {
    if (executingId) return
    setExecutingId(plugin.id)
    try {
      const result = await plugin.execute(selection)
      switch (result.outcome) {
        case "dismiss":
          if (plugin.id === "builtin.copy") {
            setCopiedToastPos({ left: position.left, top: position.top })
            if (copiedTimerRef.current !== null) {
              clearTimeout(copiedTimerRef.current)
            }
            copiedTimerRef.current = setTimeout(() => {
              copiedTimerRef.current = null
              setCopiedToastPos(null)
            }, 1200)
          }
          setActiveSubpanelId(null)
          selection.clearSelection()
          onDismiss?.()
          break
        case "keep_open":
          break
        case "open_subpanel":
          setActiveSubpanelId((prev) => (prev === plugin.id ? null : plugin.id))
          break
      }
    } catch {
      setActiveSubpanelId(null)
    } finally {
      setExecutingId(null)
    }
  }

  return createPortal(
    <div
      ref={toolbarRef}
      role="toolbar"
      aria-label={t("local.reader.selection.toolbar_label")}
      data-placement={position.placement}
      data-dark-surface={isDarkSurface ? "true" : "false"}
      style={{
        ...surfaceVarsStyle,
        ...(isDarkSurface === undefined
          ? undefined
          : { colorScheme: isDarkSurface ? "dark" : "light" }),
        position: "fixed",
        left: `${position.left}px`,
        top: `${position.top}px`,
        zIndex: 120,
        backgroundColor: "var(--reader-chrome-bg, var(--popover))",
        color: "var(--reader-fg, var(--popover-foreground))",
        borderColor: "var(--reader-chrome-border, var(--border))",
      }}
      className={cn(
        "relative flex flex-col rounded-xl border shadow-lg backdrop-blur-sm select-none"
      )}
    >
      <div className="flex items-center gap-1 p-1">
        {actions.map((plugin) => (
          <button
            key={plugin.id}
            type="button"
            disabled={executingId === plugin.id}
            title={
              plugin.shortcutHint
                ? `${t(plugin.labelKey)} (${plugin.shortcutHint})`
                : t(plugin.labelKey)
            }
            aria-label={t(plugin.labelKey)}
            onMouseDown={(event) => {
              // Preserve the active selection inside the chapter iframe when clicking.
              event.preventDefault()
            }}
            onClick={() => {
              void handleActionClick(plugin)
            }}
            className={cn(
              "inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs font-medium transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              isDarkSurface ? "hover:bg-white/10" : "hover:bg-black/5",
              activeSubpanelId === plugin.id &&
                (isDarkSurface ? "bg-white/15" : "bg-black/10")
            )}
          >
            {plugin.renderIcon()}
            <span>{t(plugin.labelKey)}</span>
          </button>
        ))}
      </div>

      {activeSubpanelPlugin?.renderSubpanel && (
        <div
          data-testid="selection-toolbar-subpanel"
          style={{
            borderColor: "var(--reader-chrome-border, var(--border))",
          }}
          className="border-t p-2 text-xs"
        >
          {activeSubpanelPlugin.renderSubpanel(selection, {
            closeToolbar: () => {
              setActiveSubpanelId(null)
              selection.clearSelection()
              onDismiss?.()
            },
            collapseSubpanel: () => {
              setActiveSubpanelId(null)
            },
          })}
        </div>
      )}

      {position.placement !== "inside-top" && (
        <span
          aria-hidden="true"
          style={{
            left: `${position.arrowOffsetX}px`,
            backgroundColor: "var(--reader-chrome-bg, var(--popover))",
            borderColor: "var(--reader-chrome-border, var(--border))",
          }}
          className={cn(
            "pointer-events-none absolute size-2.5 -translate-x-1/2 rotate-45 border",
            position.placement === "top"
              ? "-bottom-1.5 border-t-0 border-l-0"
              : "-top-1.5 border-r-0 border-b-0"
          )}
        />
      )}
    </div>,
    document.body
  )
}
