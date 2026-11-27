import { useEffect, useMemo, useRef, useState } from "react"
import { ChevronDown, ChevronRight, ExternalLink } from "lucide-react"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { cn } from "cn"
import {
  TOC_FLAG_DEFAULT_COLLAPSED,
  TOC_FLAG_EXTERNAL_LINK,
  type ReaderTocNode,
} from "../contracts/engine"

export interface TocTreeProps {
  readonly toc: readonly ReaderTocNode[]
  readonly activeTocIndex: number | null
  readonly onSelectNode: (node: ReaderTocNode, index: number) => void
  readonly onExternalLink?: ((target: string, title?: string) => void) | undefined
  readonly className?: string | undefined
}

function createInitialCollapsedSet(
  toc: readonly ReaderTocNode[]
): ReadonlySet<number> {
  const collapsed = new Set<number>()
  for (let i = 0; i < toc.length; i++) {
    const node = toc[i]
    if (
      node &&
      node.subtree_end > i + 1 &&
      (node.flags & TOC_FLAG_DEFAULT_COLLAPSED) !== 0
    ) {
      collapsed.add(i)
    }
  }
  return collapsed
}

export function TocTree({
  toc,
  activeTocIndex,
  onSelectNode,
  onExternalLink,
  className,
}: TocTreeProps) {
  const { t } = useTranslation()
  const activeButtonRef = useRef<HTMLButtonElement | null>(null)
  const [prevToc, setPrevToc] = useState(toc)
  const [collapsed, setCollapsed] = useState<ReadonlySet<number>>(() =>
    createInitialCollapsedSet(toc)
  )

  if (toc !== prevToc) {
    setPrevToc(toc)
    setCollapsed(createInitialCollapsedSet(toc))
  }

  useEffect(() => {
    if (activeTocIndex === null) return
    activeButtonRef.current?.scrollIntoView({ block: "nearest" })
  }, [activeTocIndex])

  // O(N) linear pass skipping collapsed subtrees directly via subtree_end.
  const visibleIndices = useMemo(() => {
    const result: number[] = []
    for (let i = 0; i < toc.length; ) {
      const node = toc[i]
      if (!node) break
      result.push(i)
      if (collapsed.has(i) && node.subtree_end > i + 1) {
        i = Math.max(i + 1, node.subtree_end)
      } else {
        i += 1
      }
    }
    return result
  }, [toc, collapsed])

  const toggleNodeCollapse = (index: number) => {
    setCollapsed((prev) => {
      const next = new Set(prev)
      if (next.has(index)) {
        next.delete(index)
      } else {
        next.add(index)
      }
      return next
    })
  }

  if (toc.length === 0) {
    return (
      <div className={cn("p-4 text-center text-xs opacity-65", className)}>
        {t("local.reader.toc.empty")}
      </div>
    )
  }

  return (
    <nav
      aria-label={t("local.reader.toc.title")}
      className={cn("flex flex-col gap-0.5 overflow-y-auto p-2", className)}
    >
      {visibleIndices.map((index) => {
        const node = toc[index]
        if (!node) return null

        const hasChildren = node.subtree_end > index + 1
        const isCollapsed = collapsed.has(index)
        const isActive = activeTocIndex === index
        const hasActiveDescendant =
          isCollapsed &&
          hasChildren &&
          activeTocIndex !== null &&
          activeTocIndex > index &&
          activeTocIndex < node.subtree_end
        const isExternal = (node.flags & TOC_FLAG_EXTERNAL_LINK) !== 0
        const hasLocator = node.locator !== null && !isExternal

        return (
          <div
            key={`${index}-${node.title}`}
            style={{ paddingInlineStart: `${Math.min(node.depth, 6) * 14}px` }}
            className="flex items-center gap-1"
          >
            {hasChildren ? (
              <button
                type="button"
                aria-label={
                  isCollapsed
                    ? t("local.reader.toc.expand")
                    : t("local.reader.toc.collapse")
                }
                aria-expanded={!isCollapsed}
                onClick={(event) => {
                  event.stopPropagation()
                  toggleNodeCollapse(index)
                }}
                className="flex size-6 shrink-0 items-center justify-center rounded-md opacity-70 hover:bg-[var(--reader-hover-bg,rgba(0,0,0,0.05))] hover:opacity-100"
              >
                {isCollapsed ? (
                  <ChevronRight className="size-3.5" aria-hidden="true" />
                ) : (
                  <ChevronDown className="size-3.5" aria-hidden="true" />
                )}
              </button>
            ) : (
              <span className="size-6 shrink-0" aria-hidden="true" />
            )}

            <button
              ref={isActive || hasActiveDescendant ? activeButtonRef : undefined}
              type="button"
              data-toc-index={index}
              data-active={isActive ? "true" : undefined}
              data-has-active-descendant={hasActiveDescendant ? "true" : undefined}
              aria-current={isActive ? "true" : undefined}
              disabled={!hasLocator && !isExternal}
              onClick={() => {
                if (isExternal) {
                  const externalUrl = node.external_url?.trim()
                  if (externalUrl) {
                    onExternalLink?.(externalUrl, node.title)
                  } else {
                    onExternalLink?.(node.title)
                  }
                  return
                }
                if (hasLocator) {
                  onSelectNode(node, index)
                }
              }}
              className={cn(
                "flex min-w-0 flex-1 items-center justify-between gap-2 rounded-lg px-2 py-1.5 text-left text-xs transition-colors",
                "hover:bg-[var(--reader-hover-bg,rgba(0,0,0,0.05))] disabled:cursor-default disabled:opacity-50",
                isActive && "bg-primary/15 font-semibold",
                hasActiveDescendant && !isActive && "bg-primary/10 font-medium"
              )}
            >
              <span className="flex min-w-0 items-center gap-1.5">
                {hasActiveDescendant && (
                  <span
                    data-testid="toc-collapsed-active-dot"
                    aria-hidden="true"
                    className="size-1.5 shrink-0 rounded-full bg-primary"
                  />
                )}
                <span className="truncate">{node.title}</span>
              </span>
              {isExternal && (
                <ExternalLink
                  className="size-3 shrink-0 opacity-60"
                  aria-label={t("local.reader.toc.external_link")}
                />
              )}
            </button>
          </div>
        )
      })}
    </nav>
  )
}
