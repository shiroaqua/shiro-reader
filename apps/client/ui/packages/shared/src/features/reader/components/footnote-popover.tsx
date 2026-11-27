import { useEffect, useRef, type CSSProperties, type MouseEvent } from "react"
import { ArrowUpRight, BookOpenText, X } from "lucide-react"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import type { FootnoteKind, FootnotePayload } from "../contracts/engine"
import type { SafeViewportInsets } from "../selection/geometry"
import type { SelectionToolbarViewportBounds } from "./selection-toolbar"

export interface FootnotePopoverProps {
  readonly footnote: FootnotePayload | null
  readonly viewportBounds?: SelectionToolbarViewportBounds | null | undefined
  readonly safeInsets?: SafeViewportInsets | undefined
  readonly viewportCssVars?: Readonly<Record<string, string>> | undefined
  readonly isDarkSurface?: boolean | undefined
  readonly onClose: () => void
  readonly onJumpToSource: (footnote: FootnotePayload) => void
  readonly onNavigateLink: (href: string) => void
}

const POPOVER_MAX_WIDTH = 360
const POPOVER_MIN_WIDTH = 220
const POPOVER_MAX_HEIGHT = 240
const POPOVER_MARGIN = 12

function resolveFootnoteTitleKey(kind: FootnoteKind):
  | "local.reader.footnote.title_footnote"
  | "local.reader.footnote.title_endnote"
  | "local.reader.footnote.title_biblio"
  | "local.reader.footnote.title_definition"
  | "local.reader.footnote.title_note" {
  switch (kind) {
    case "footnote":
      return "local.reader.footnote.title_footnote"
    case "endnote":
      return "local.reader.footnote.title_endnote"
    case "biblioentry":
      return "local.reader.footnote.title_biblio"
    case "definition":
      return "local.reader.footnote.title_definition"
    case "note":
      return "local.reader.footnote.title_note"
  }
}

export function FootnotePopover({
  footnote,
  viewportBounds,
  safeInsets,
  viewportCssVars,
  isDarkSurface = false,
  onClose,
  onJumpToSource,
  onNavigateLink,
}: FootnotePopoverProps) {
  const { t } = useTranslation()
  const popoverRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!footnote) return

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault()
        onClose()
      }
    }
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null
      if (target && popoverRef.current?.contains(target)) {
        return
      }
      onClose()
    }

    document.addEventListener("keydown", onKeyDown)
    document.addEventListener("pointerdown", onPointerDown, true)
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.removeEventListener("pointerdown", onPointerDown, true)
    }
  }, [footnote, onClose])

  if (!footnote) return null

  const vw = Math.max(POPOVER_MIN_WIDTH + POPOVER_MARGIN * 2, viewportBounds?.width ?? 640)
  const vh = Math.max(200, viewportBounds?.height ?? 480)
  const safeTop = Math.max(0, safeInsets?.top ?? 0) + POPOVER_MARGIN
  const safeBottom = Math.max(0, safeInsets?.bottom ?? 0) + POPOVER_MARGIN
  const safeLeft = Math.max(0, safeInsets?.left ?? 0) + POPOVER_MARGIN
  const safeRight = Math.max(0, safeInsets?.right ?? 0) + POPOVER_MARGIN

  const width = Math.min(POPOVER_MAX_WIDTH, Math.max(POPOVER_MIN_WIDTH, vw - safeLeft - safeRight))
  const anchor = footnote.anchorRect

  let left = Math.round((vw - width) / 2)
  let top = Math.max(safeTop, vh - safeBottom - POPOVER_MAX_HEIGHT)

  if (anchor) {
    const centerX = anchor.left + anchor.width / 2
    left = Math.round(
      Math.min(vw - safeRight - width, Math.max(safeLeft, centerX - width / 2))
    )
    const spaceBelow = vh - safeBottom - (anchor.top + anchor.height + 8)
    const spaceAbove = anchor.top - 8 - safeTop
    if (spaceBelow >= 140 || spaceBelow >= spaceAbove) {
      top = Math.round(
        Math.min(vh - safeBottom - 120, Math.max(safeTop, anchor.top + anchor.height + 8))
      )
    } else {
      top = Math.round(
        Math.max(safeTop, anchor.top - 8 - Math.min(POPOVER_MAX_HEIGHT, spaceAbove))
      )
    }
  }

  const handleContentClick = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as Element | null
    const link = target?.closest?.("a[href]") as HTMLAnchorElement | null
    if (!link) return
    event.preventDefault()
    event.stopPropagation()
    const href = link.getAttribute("href")?.trim()
    if (!href) return
    onNavigateLink(href)
  }

  const surfaceVars = viewportCssVars as CSSProperties | undefined

  return (
    <div
      ref={popoverRef}
      role="dialog"
      aria-label={t(resolveFootnoteTitleKey(footnote.kind))}
      data-testid="reader-footnote-popover"
      data-reader-chrome="footnote"
      data-dark-surface={isDarkSurface ? "true" : "false"}
      style={{
        ...surfaceVars,
        colorScheme: isDarkSurface ? "dark" : "light",
        left: `${left}px`,
        top: `${top}px`,
        width: `${width}px`,
        maxHeight: `${POPOVER_MAX_HEIGHT}px`,
        backgroundColor: "var(--reader-chrome-bg, var(--popover))",
        borderColor: "var(--reader-chrome-border, var(--border))",
        color: "var(--reader-fg, var(--popover-foreground))",
      }}
      className="pointer-events-auto absolute z-30 flex flex-col overflow-hidden rounded-xl border shadow-xl backdrop-blur-md"
    >
      <div
        className="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-1.5"
        style={{ borderColor: "var(--reader-chrome-border, var(--border))" }}
      >
        <div className="flex min-w-0 items-center gap-1.5 text-xs font-medium">
          <BookOpenText className="size-3.5 shrink-0 opacity-75" aria-hidden="true" />
          <span className="truncate">{t(resolveFootnoteTitleKey(footnote.kind))}</span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {footnote.locator && (
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="h-6 gap-1 px-1.5 text-[11px]"
              onClick={() => onJumpToSource(footnote)}
            >
              <span>{t("local.reader.footnote.jump")}</span>
              <ArrowUpRight className="size-3" aria-hidden="true" />
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="size-6"
            aria-label={t("local.reader.footnote.close")}
            onClick={onClose}
          >
            <X className="size-3.5" aria-hidden="true" />
          </Button>
        </div>
      </div>

      <div
        data-testid="reader-footnote-content"
        onClick={handleContentClick}
        className="min-h-0 flex-1 overflow-y-auto px-3 py-2 text-xs leading-relaxed [&_a]:underline [&_a]:underline-offset-2 [&_p]:my-1 [&_p:first-child]:mt-0 [&_p:last-child]:mb-0"
        dangerouslySetInnerHTML={{ __html: footnote.html }}
      />
    </div>
  )
}
