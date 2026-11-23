import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react"
import {
  Asterisk,
  BookOpen,
  CircleAlert,
  CornerDownLeft,
  Hash,
  LoaderCircle,
  RotateCcw,
  Search,
  SearchX,
  Tags,
  TextQuote,
  X,
} from "lucide-react"
import { cn } from "cn"
import { BookCover } from "@/features/books/book-cover"
import { useBookTagIndex } from "@/features/books/use-book-tag-index"
import { tagNameColor } from "@/features/tags/tag-color-picker"
import {
  applySuggestion,
  buildSuggestions,
  commitSearchDraft,
  composeSearchValue,
  formatTagToken,
  getSearchContext,
  partitionSearchChips,
  tokenizeBookSearch,
  type BookTagIndex,
  type IndexedTag,
  type SearchContext,
  type SearchSuggestion,
  type SuggestionGroup,
  type TagSegment,
} from "@/lib/book-search"
import { formatCountMessage, useTranslation } from "@/lib/i18n"

/**
 * Header search field for the bookshelf. Typing filters in real time; a
 * suggestion panel offers tags, tag values and titles, and `#name:"value"`
 * tokens are tinted with the tag color right inside the field.
 */
export function BookSearch({
  value,
  onChange,
  className,
  autoFocus = false,
  fullWidth = false,
}: {
  value: string
  onChange: (value: string) => void
  className?: string
  /** Focus the field on mount (phone search mode). */
  autoFocus?: boolean
  /** Fill the available width instead of collapsing to a compact field. */
  fullWidth?: boolean
}) {
  const { t } = useTranslation()
  const listboxId = useId()
  const trackRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const mirrorRef = useRef<HTMLDivElement>(null)
  const pendingCaret = useRef<number | null>(null)
  const scrollSyncFrame = useRef<number | null>(null)
  const prevChipCount = useRef(0)
  const [focused, setFocused] = useState(false)
  const [dismissed, setDismissed] = useState(false)
  const [draftCaret, setDraftCaret] = useState(0)
  const [editingTagStart, setEditingTagStart] = useState<number | null>(null)
  const [activeKey, setActiveKey] = useState<string | null>(null)
  const [indexRequested, setIndexRequested] = useState(false)

  const { books, index, loaded, total, isLoading, isTagsError, retry } =
    useBookTagIndex(indexRequested || value.includes("#"))
  const { chips, draft, prefixLength } = useMemo(
    () => partitionSearchChips(value, index, focused, editingTagStart),
    [value, index, focused, editingTagStart]
  )
  const effectiveDraftCaret = Math.min(draftCaret, draft.length)
  const contextQuery = useMemo(
    () =>
      chips.length === 0
        ? draft
        : `${chips.map((chip) => chip.raw).join(" ")} ${draft}`,
    [chips, draft]
  )
  const contextCaret = prefixLength + effectiveDraftCaret
  const context = useMemo(
    () => getSearchContext(contextQuery, contextCaret),
    [contextQuery, contextCaret]
  )
  const groups = useMemo(
    () => buildSuggestions(contextQuery, context, index, books),
    [contextQuery, context, index, books]
  )
  const items = useMemo(() => groups.flatMap((group) => group.items), [groups])
  // While completing a tag the first hint is preselected; plain words only get one once navigated.
  const autoPick = context.mode === "tag-name" || context.mode === "tag-value"
  const activeItem =
    items.find((item) => item.key === activeKey) ??
    (autoPick ? (items[0] ?? null) : null)
  const open = focused && !dismissed
  const shortcut = useShortcutLabel()

  function syncMirrorScroll() {
    if (mirrorRef.current && inputRef.current) {
      mirrorRef.current.scrollLeft = inputRef.current.scrollLeft
    }
  }

  function scheduleMirrorSync() {
    syncMirrorScroll()
    if (scrollSyncFrame.current !== null) {
      cancelAnimationFrame(scrollSyncFrame.current)
    }
    scrollSyncFrame.current = requestAnimationFrame(() => {
      scrollSyncFrame.current = null
      syncMirrorScroll()
    })
  }

  useEffect(() => {
    if (autoFocus) inputRef.current?.focus()
  }, [autoFocus])

  // Ctrl/⌘ + K or "/" focuses the field from anywhere on the page.
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isModK =
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        event.key.toLowerCase() === "k"
      const isSlash =
        event.key === "/" &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !isEditableTarget(event.target)
      if (!isModK && !isSlash) return
      const input = inputRef.current
      if (!input || input.offsetParent === null) return
      if (
        document.querySelector(
          '[role="dialog"], [role="alertdialog"], [role="menu"]'
        )
      ) {
        return
      }
      event.preventDefault()
      input.focus()
      input.select()
    }
    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [])

  // Keep mirror scroll aligned during width transitions and viewport resizes.
  useEffect(() => {
    const input = inputRef.current
    if (!input || typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(syncMirrorScroll)
    observer.observe(input)
    return () => {
      observer.disconnect()
      if (scrollSyncFrame.current !== null) {
        cancelAnimationFrame(scrollSyncFrame.current)
        scrollSyncFrame.current = null
      }
    }
  }, [])

  // Apply a caret position requested by a suggestion once the new value is rendered.
  useLayoutEffect(() => {
    const input = inputRef.current
    const track = trackRef.current
    const shouldScrollTrack =
      chips.length > prevChipCount.current || pendingCaret.current !== null
    prevChipCount.current = chips.length
    if (input && pendingCaret.current !== null) {
      input.setSelectionRange(pendingCaret.current, pendingCaret.current)
      pendingCaret.current = null
    }
    if (track && shouldScrollTrack) {
      track.scrollLeft = track.scrollWidth
    }
    syncMirrorScroll()
  })

  function readCaret() {
    const input = inputRef.current
    if (input) setDraftCaret(input.selectionStart ?? input.value.length)
    scheduleMirrorSync()
  }

  function choose(suggestion: SearchSuggestion) {
    const edit = applySuggestion(contextQuery, context, suggestion, index)
    const committed = commitSearchDraft([], edit.query, edit.caret, index)
    pendingCaret.current = committed.draftCaret
    setDraftCaret(committed.draftCaret)
    setEditingTagStart(committed.editingTagStart)
    setActiveKey(null)
    setDismissed(false)
    onChange(committed.query)
    inputRef.current?.focus()
  }

  function removeChip(chip: TagSegment) {
    const remaining = chips.filter((item) => item.start !== chip.start)
    const nextPrefixLength =
      remaining.length === 0
        ? 0
        : remaining.map((item) => item.raw).join(" ").length + 1
    if (editingTagStart !== null) {
      const draftOffset = editingTagStart - prefixLength
      setEditingTagStart(draftOffset >= 0 ? nextPrefixLength + draftOffset : null)
    }
    onChange(composeSearchValue(remaining, draft))
  }

  function editChip(chip: TagSegment) {
    const remaining = chips.filter((item) => item.start !== chip.start)
    const nextDraft =
      draft === "" ? chip.raw : `${draft.trimEnd()} ${chip.raw}`
    const chipOffset = nextDraft.length - chip.raw.length
    const nextPrefixLength =
      remaining.length === 0
        ? 0
        : remaining.map((item) => item.raw).join(" ").length + 1
    const nextCaret =
      chip.quoted && chip.closed
        ? chipOffset + (chip.tokenEnd - chip.start) - 1
        : nextDraft.length
    pendingCaret.current = nextCaret
    setDraftCaret(nextCaret)
    setEditingTagStart(nextPrefixLength + chipOffset)
    setActiveKey(null)
    setDismissed(false)
    onChange(composeSearchValue(remaining, nextDraft))
    inputRef.current?.focus()
  }

  function insertHash() {
    if (context.mode === "tag-name" && context.prefix === "") {
      setDismissed(false)
      inputRef.current?.focus()
      return
    }
    const lead = draft === "" || /\s$/.test(draft) ? draft : `${draft} `
    const nextDraft = `${lead}#`
    const committed = commitSearchDraft(chips, nextDraft, nextDraft.length, index)
    pendingCaret.current = committed.draftCaret
    setDraftCaret(committed.draftCaret)
    setEditingTagStart(committed.editingTagStart)
    setDismissed(false)
    onChange(committed.query)
    inputRef.current?.focus()
  }

  function clear() {
    pendingCaret.current = 0
    setDraftCaret(0)
    setEditingTagStart(null)
    setActiveKey(null)
    onChange("")
    inputRef.current?.focus()
  }

  function moveActive(step: number) {
    if (items.length === 0) return
    const current = activeItem ? items.indexOf(activeItem) : -1
    const next =
      current === -1
        ? step > 0
          ? 0
          : items.length - 1
        : (current + step + items.length) % items.length
    setActiveKey(items[next].key)
    document
      .getElementById(optionId(listboxId, items[next].key))
      ?.scrollIntoView({ block: "nearest" })
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.nativeEvent.isComposing) return
    if (
      event.key === '"' &&
      context.mode === "tag-value" &&
      context.segment.quoted &&
      context.segment.closed &&
      contextCaret === context.segment.tokenEnd - 1 &&
      contextQuery[contextCaret - 1] !== "\\"
    ) {
      event.preventDefault()
      const nextCaret = effectiveDraftCaret + 1
      inputRef.current?.setSelectionRange(nextCaret, nextCaret)
      setDraftCaret(nextCaret)
      scheduleMirrorSync()
      return
    }
    switch (event.key) {
      case "Backspace":
        if (
          chips.length > 0 &&
          inputRef.current?.selectionStart === 0 &&
          inputRef.current?.selectionEnd === 0
        ) {
          event.preventDefault()
          removeChip(chips[chips.length - 1])
        }
        break
      case "ArrowDown":
      case "ArrowUp":
        event.preventDefault()
        if (!open) setDismissed(false)
        else moveActive(event.key === "ArrowDown" ? 1 : -1)
        break
      case "Enter":
        if (open && activeItem) {
          event.preventDefault()
          choose(activeItem)
        } else {
          setDismissed(true)
          if (editingTagStart !== null) setEditingTagStart(null)
        }
        break
      case "Tab":
        if (open && activeItem && context.mode !== "idle") {
          event.preventDefault()
          choose(activeItem)
        }
        break
      case "Escape":
        event.preventDefault()
        if (open) setDismissed(true)
        else if (value !== "") clear()
        else inputRef.current?.blur()
        break
    }
  }

  const activeDescendant =
    open && activeItem ? optionId(listboxId, activeItem.key) : undefined

  return (
    <div
      className={cn(
        "relative flex min-w-0 flex-1 justify-end group-has-data-[slot=selection-bar]/header:hidden",
        className
      )}
    >
      <div
        className={cn(
          "relative w-full max-w-60 transition-[max-width] duration-300 ease-out",
          (focused || value !== "") && "max-w-lg",
          fullWidth && "max-w-none"
        )}
      >
        <div
          data-focused={focused || undefined}
          className={cn(
            "group/field flex h-9 items-center gap-2 rounded-full border border-transparent bg-muted/70 pr-1.5 pl-3 text-sm transition-[background-color,border-color,box-shadow] duration-200 hover:bg-muted",
            "data-focused:border-ring/50 data-focused:bg-background data-focused:shadow-lg data-focused:ring-4 data-focused:shadow-black/5 data-focused:ring-ring/15"
          )}
          onMouseDown={(event) => {
            if (event.target !== inputRef.current) {
              event.preventDefault()
              if (
                !(event.target instanceof Element && event.target.closest("button"))
              ) {
                inputRef.current?.focus()
                if (dismissed) setDismissed(false)
              }
            }
          }}
        >
          {isLoading && open ? (
            <LoaderCircle
              aria-hidden="true"
              className="size-4 shrink-0 animate-spin text-muted-foreground"
            />
          ) : (
            <Search
              aria-hidden="true"
              className="size-4 shrink-0 text-muted-foreground transition-colors group-data-focused/field:text-foreground"
            />
          )}
          <div
            ref={trackRef}
            className="flex h-full min-w-0 flex-1 items-center gap-1.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
          >
            {chips.map((chip) => (
              <TagFilterChip
                key={chip.start}
                segment={chip}
                index={index}
                onEdit={() => editChip(chip)}
                onRemove={() => removeChip(chip)}
              />
            ))}
            <div className="relative h-full min-w-16 flex-1">
              <div
                ref={mirrorRef}
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 flex items-center overflow-hidden whitespace-pre"
              >
                <SearchMirror
                  value={draft}
                  index={index}
                  focused={focused}
                  caret={effectiveDraftCaret}
                />
              </div>
              <input
                ref={inputRef}
                type="text"
                role="combobox"
                aria-label={t("local.search.label")}
                aria-expanded={open}
                aria-controls={open ? listboxId : undefined}
                aria-autocomplete="list"
                aria-activedescendant={activeDescendant}
                autoComplete="off"
                spellCheck={false}
                value={draft}
                placeholder={
                  chips.length === 0 ? t("local.search.placeholder") : ""
                }
                className="relative h-full w-full min-w-0 bg-transparent text-transparent caret-foreground outline-none placeholder:text-muted-foreground selection:bg-primary/15"
                onChange={(event) => {
                  const nextDraft = event.target.value
                  const rawCaret =
                    event.target.selectionStart ?? nextDraft.length
                  const committed = commitSearchDraft(
                    chips,
                    nextDraft,
                    rawCaret,
                    index
                  )
                  if (committed.promotedCount > 0) {
                    pendingCaret.current = committed.draftCaret
                  }
                  setDraftCaret(committed.draftCaret)
                  setEditingTagStart(committed.editingTagStart)
                  setActiveKey(null)
                  setDismissed(false)
                  onChange(committed.query)
                  scheduleMirrorSync()
                }}
                onClick={() => {
                  if (dismissed) setDismissed(false)
                }}
                onSelect={readCaret}
                onKeyUp={scheduleMirrorSync}
                onScroll={syncMirrorScroll}
                onKeyDown={handleKeyDown}
                onFocus={() => {
                  setFocused(true)
                  setDismissed(false)
                  setIndexRequested(true)
                  readCaret()
                }}
                onBlur={() => {
                  setFocused(false)
                  setActiveKey(null)
                  setEditingTagStart(null)
                }}
              />
            </div>
          </div>
          {value !== "" ? (
            <button
              type="button"
              tabIndex={-1}
              aria-label={t("local.search.clear")}
              title={t("local.search.clear")}
              onClick={clear}
              className="flex size-6 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted-foreground/15 hover:text-foreground"
            >
              <X aria-hidden="true" className="size-3.5" />
            </button>
          ) : (
            <Kbd className="mr-0.5 hidden transition-opacity group-data-focused/field:opacity-0 sm:inline-flex">
              {shortcut}
            </Kbd>
          )}
        </div>

        {open && (
          <SuggestionPanel
            listboxId={listboxId}
            groups={groups}
            context={context}
            activeKey={activeItem?.key ?? null}
            isLoading={isLoading}
            isTagsError={isTagsError}
            loaded={loaded}
            total={total}
            onHover={setActiveKey}
            onLeave={() => {
              if (!autoPick) setActiveKey(null)
            }}
            onChoose={choose}
            onInsertHash={insertHash}
            onRetry={retry}
          />
        )}
      </div>
    </div>
  )
}

/* -------------------------------------------------------------- mirror -- */

/** Renders the query behind the transparent input so tag tokens can be tinted. */
function SearchMirror({
  value,
  index,
  focused,
  caret,
}: {
  value: string
  index: BookTagIndex | null
  focused: boolean
  caret: number
}) {
  const segments = useMemo(() => tokenizeBookSearch(value), [value])
  return (
    <span className="text-foreground">
      {segments.map((segment) =>
        segment.kind === "tag" ? (
          <MirrorTag
            key={segment.start}
            segment={segment}
            index={index}
            editing={focused && segment.start < caret && caret <= segment.end}
          />
        ) : (
          <span key={segment.start}>{segment.raw}</span>
        )
      )}
      {/* Matches the input's trailing caret room so both scroll the same. */}
      <span className="inline-block w-px" />
    </span>
  )
}

function MirrorTag({
  segment,
  index,
  editing,
}: {
  segment: TagSegment
  index: BookTagIndex | null
  editing: boolean
}) {
  const entry = index?.tagsByName.get(segment.name)?.[0]
  // Only flag a name once the user has left it (or moved on to a value), not while typing it.
  const unknown =
    index !== null &&
    segment.name !== "" &&
    entry === undefined &&
    (!editing || segment.hasColon)
  const tinted = segment.complete && (entry !== undefined || index === null)
  const accent = entry ? tagAccent(entry.tag.color) : "var(--muted-foreground)"
  const style = {
    "--tag-accent": accent,
    color: unknown ? undefined : accent,
    backgroundColor: tinted
      ? "color-mix(in oklab, var(--tag-accent) 12%, transparent)"
      : undefined,
    boxShadow: tinted
      ? "inset 0 0 0 1px color-mix(in oklab, var(--tag-accent) 25%, transparent)"
      : undefined,
  } as CSSProperties

  const nameEnd = segment.nameEnd - segment.start
  return (
    <span
      style={style}
      className={cn(
        "rounded-md transition-colors",
        unknown &&
          "text-destructive underline decoration-destructive/60 decoration-wavy underline-offset-4",
        !segment.complete &&
          !unknown &&
          "underline decoration-current/40 decoration-dashed underline-offset-4"
      )}
    >
      <span className="opacity-55">#</span>
      {segment.raw.slice(1, nameEnd)}
      {segment.hasColon && (
        <>
          <span className="opacity-55">{segment.raw.slice(nameEnd, nameEnd + 1)}</span>
          <span className="text-foreground">{segment.raw.slice(nameEnd + 1)}</span>
        </>
      )}
      {!segment.hasColon && segment.raw.slice(nameEnd)}
    </span>
  )
}

/* --------------------------------------------------------------- panel -- */

function SuggestionPanel({
  listboxId,
  groups,
  context,
  activeKey,
  isLoading,
  isTagsError,
  loaded,
  total,
  onHover,
  onLeave,
  onChoose,
  onInsertHash,
  onRetry,
}: {
  listboxId: string
  groups: SuggestionGroup[]
  context: SearchContext
  activeKey: string | null
  isLoading: boolean
  isTagsError: boolean
  loaded: number
  total: number
  onHover: (key: string) => void
  onLeave: () => void
  onChoose: (suggestion: SearchSuggestion) => void
  onInsertHash: () => void
  onRetry: () => void
}) {
  const { t } = useTranslation()
  const keyword =
    context.mode === "text"
      ? context.term
      : context.mode === "idle"
        ? ""
        : context.prefix
  const progress = total === 0 ? 1 : loaded / total
  const duplicateValueGroups =
    groups.filter((group) => group.id === "values").length > 1

  return (
    <div
      className="absolute top-full right-0 z-50 mt-2 w-[min(28rem,calc(100vw-2rem))] origin-top-right overflow-hidden rounded-2xl border bg-popover/95 text-popover-foreground shadow-2xl ring-1 shadow-black/10 ring-foreground/5 backdrop-blur-xl duration-150 animate-in fade-in-0 zoom-in-95 slide-in-from-top-1"
      // Keep focus in the input while clicking inside the panel.
      onMouseDown={(event) => event.preventDefault()}
    >
      {isLoading && (
        <div className="h-0.5 w-full bg-muted">
          <div
            className="h-full bg-primary/70 transition-[width] duration-300"
            style={{ width: `${Math.max(progress * 100, 6)}%` }}
          />
        </div>
      )}

      <div
        id={listboxId}
        role="listbox"
        aria-label={t("local.search.label")}
        onMouseLeave={onLeave}
        className="max-h-[min(26rem,60vh)] overflow-y-auto overscroll-contain p-1.5"
      >
        {groups.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
            <div
              className={cn(
                "flex size-10 items-center justify-center rounded-xl",
                !isLoading && isTagsError
                  ? "bg-destructive/10 text-destructive"
                  : "bg-muted text-muted-foreground"
              )}
            >
              {isLoading ? (
                <LoaderCircle aria-hidden="true" className="size-5 animate-spin" />
              ) : isTagsError ? (
                <CircleAlert aria-hidden="true" className="size-5" />
              ) : (
                <SearchX aria-hidden="true" className="size-5" />
              )}
            </div>
            <p className="text-sm text-muted-foreground">
              {isLoading
                ? t("local.common.loading")
                : isTagsError
                  ? t("local.tag.load_failed")
                  : t("local.search.no_suggestions")}
            </p>
            {!isLoading && isTagsError && (
              <button
                type="button"
                onClick={onRetry}
                className="mt-1 inline-flex h-7 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium text-foreground transition-colors hover:bg-muted"
              >
                <RotateCcw aria-hidden="true" className="size-3" />
                {t("local.common.retry")}
              </button>
            )}
          </div>
        ) : (
          groups.map((group) => (
            <div
              key={`${group.id}:${group.entry?.tag.id ?? ""}`}
              role="group"
              aria-label={groupLabel(group, t)}
              className="not-first:mt-1.5 not-first:border-t not-first:pt-1.5"
            >
              <GroupHeading
                group={group}
                showPath={duplicateValueGroups}
              />
              {group.items.map((item) => (
                <SuggestionRow
                  key={item.key}
                  id={optionId(listboxId, item.key)}
                  item={item}
                  keyword={keyword}
                  active={item.key === activeKey}
                  onHover={() => onHover(item.key)}
                  onChoose={() => onChoose(item)}
                />
              ))}
            </div>
          ))
        )}
      </div>

      <div className="flex items-center justify-between gap-3 border-t bg-muted/40 px-3 py-2 text-[11px] text-muted-foreground">
        {isLoading ? (
          <span className="flex items-center gap-1.5 tabular-nums">
            <LoaderCircle aria-hidden="true" className="size-3 animate-spin" />
            {t("local.search.indexing")
              .replace("{loaded}", String(loaded))
              .replace("{total}", String(total))}
          </span>
        ) : (
          <button
            type="button"
            onClick={onInsertHash}
            className="group/hint flex min-w-0 items-center gap-1.5 rounded-md text-left transition-colors hover:text-foreground"
          >
            <Tags aria-hidden="true" className="size-3.5 shrink-0" />
            <span className="truncate">{t("local.search.syntax_hint")}</span>
            <code className="shrink-0 rounded-md border bg-background px-1.5 py-0.5 font-mono text-[10.5px] text-foreground/80 shadow-xs transition-colors group-hover/hint:border-ring/50">
              {t("local.search.syntax_example")}
            </code>
          </button>
        )}
        <div className="hidden shrink-0 items-center gap-2.5 sm:flex">
          <span className="flex items-center gap-1">
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd>
            {t("local.search.hint.navigate")}
          </span>
          <span className="flex items-center gap-1">
            <Kbd>
              <CornerDownLeft aria-hidden="true" className="size-2.5" />
            </Kbd>
            {t("local.search.hint.select")}
          </span>
          <span className="flex items-center gap-1">
            <Kbd>Esc</Kbd>
            {t("local.search.hint.close")}
          </span>
        </div>
      </div>
    </div>
  )
}

function GroupHeading({
  group,
  showPath = false,
}: {
  group: SuggestionGroup
  showPath?: boolean
}) {
  const { t } = useTranslation()
  const icon =
    group.id === "tags" ? (
      <Hash aria-hidden="true" className="size-3" />
    ) : group.id === "values" ? (
      <TextQuote aria-hidden="true" className="size-3" />
    ) : (
      <BookOpen aria-hidden="true" className="size-3" />
    )
  return (
    <div className="flex items-center gap-1.5 px-2.5 pt-1.5 pb-1 text-[11px] font-medium tracking-wide text-muted-foreground">
      {icon}
      {group.id === "values" && group.entry ? (
        <span className="flex min-w-0 items-center gap-1">
          {showPath && group.entry.path.length > 0 && (
            <span className="truncate opacity-70">
              {group.entry.path.join(" › ")} ›
            </span>
          )}
          <span
            className="truncate"
            style={{ color: tagAccent(group.entry.tag.color) }}
          >
            {group.entry.displayName}
          </span>
          <span className="shrink-0">· {t("local.search.section.values")}</span>
        </span>
      ) : (
        <span>{groupLabel(group, t)}</span>
      )}
    </div>
  )
}

function SuggestionRow({
  id,
  item,
  keyword,
  active,
  onHover,
  onChoose,
}: {
  id: string
  item: SearchSuggestion
  keyword: string
  active: boolean
  onHover: () => void
  onChoose: () => void
}) {
  const { t } = useTranslation()
  const countLabel = (count: number) =>
    formatCountMessage(count, "local.search.book_count_one", "local.search.book_count", t)

  let leading: ReactNode
  let title: ReactNode
  let subtitle: ReactNode
  let trailing: ReactNode = null

  if (item.kind === "book") {
    leading = (
      <BookCover
        bookId={item.book.id}
        className="h-10 w-7 shrink-0 rounded-[5px] shadow-xs"
        iconClassName="size-3.5"
      />
    )
    title = <Highlight text={item.book.title} keyword={keyword} />
    subtitle = <span className="uppercase">{item.book.type}</span>
  } else {
    const accent = tagAccent(item.entry.tag.color)
    leading = (
      <span
        aria-hidden="true"
        style={{ "--tag-accent": accent } as CSSProperties}
        className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-[color-mix(in_oklab,var(--tag-accent)_14%,transparent)] text-(--tag-accent) ring-1 ring-[color-mix(in_oklab,var(--tag-accent)_22%,transparent)] ring-inset"
      >
        {item.kind === "tag" ? (
          <Hash className="size-4" />
        ) : item.kind === "value" ? (
          <TextQuote className="size-4" />
        ) : (
          <Asterisk className="size-4" />
        )}
      </span>
    )
    trailing = (
      <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground tabular-nums">
        {countLabel(item.count)}
      </span>
    )
    if (item.kind === "tag") {
      title = (
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate" style={{ color: accent }}>
            <Highlight text={item.entry.displayName} keyword={keyword} />
          </span>
          {item.entry.tag.allows_value && (
            <span className="shrink-0 rounded-[4px] border px-1 py-px text-[10px] leading-none font-normal text-muted-foreground">
              {t("local.search.valued")}
            </span>
          )}
        </span>
      )
      subtitle = <TagPath entry={item.entry} />
    } else if (item.kind === "value") {
      title = (
        <span className="truncate">
          <Highlight text={item.value} keyword={keyword} />
        </span>
      )
      subtitle = (
        <code className="truncate font-mono text-[11px]">
          {formatTagToken(item.entry.tokenName, item.value)}
        </code>
      )
    } else {
      title = <span className="truncate">{t("local.search.any_value")}</span>
      subtitle = (
        <code className="truncate font-mono text-[11px]">#{item.entry.tokenName}</code>
      )
    }
  }

  return (
    <div
      id={id}
      role="option"
      aria-selected={active}
      data-active={active || undefined}
      onMouseMove={active ? undefined : onHover}
      onClick={onChoose}
      className="group/row relative flex cursor-pointer items-center gap-3 rounded-xl px-2.5 py-1.5 transition-colors data-active:bg-accent data-active:text-accent-foreground"
    >
      {leading}
      <div className="grid min-w-0 flex-1 gap-0.5">
        <div className="flex min-w-0 items-center text-sm font-medium">{title}</div>
        {subtitle && (
          <div className="flex min-w-0 items-center text-xs text-muted-foreground">
            {subtitle}
          </div>
        )}
      </div>
      {trailing}
      <CornerDownLeft
        aria-hidden="true"
        className="size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-data-active/row:opacity-100"
      />
    </div>
  )
}

function TagPath({ entry }: { entry: IndexedTag }) {
  if (entry.path.length === 0) {
    return <code className="truncate font-mono text-[11px]">#{entry.tokenName}</code>
  }
  return (
    <span className="flex min-w-0 items-center gap-1 truncate">
      {entry.path.map((name, position) => (
        <span key={position} className="flex shrink-0 items-center gap-1">
          <span className="truncate">{name}</span>
          <span className="opacity-50">›</span>
        </span>
      ))}
      <span className="truncate text-foreground/70">{entry.displayName}</span>
    </span>
  )
}

/** Emphasises the first case-insensitive occurrence of the keyword. */
function Highlight({ text, keyword }: { text: string; keyword: string }) {
  const position =
    keyword === "" ? -1 : text.toLocaleLowerCase().indexOf(keyword.toLocaleLowerCase())
  if (position === -1) return <>{text}</>
  return (
    <>
      {text.slice(0, position)}
      <mark className="rounded-[3px] bg-amber-300/40 px-px text-inherit">
        {text.slice(position, position + keyword.length)}
      </mark>
      {text.slice(position + keyword.length)}
    </>
  )
}

function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded-md border border-b-2 bg-background px-1 font-sans text-[10px] font-medium text-muted-foreground",
        className
      )}
    >
      {children}
    </kbd>
  )
}

/* ------------------------------------------------------ active filters -- */

/** Shared rounded pill styling for tag filters in both the search bar and toolbar. */
function TagFilterChip({
  segment,
  index,
  onEdit,
  onRemove,
  className,
}: {
  segment: TagSegment
  index: BookTagIndex | null
  onEdit?: () => void
  onRemove: () => void
  className?: string
}) {
  const { t } = useTranslation()
  const entry = index?.tagsByName.get(segment.name)?.[0]
  const accent = entry ? tagAccent(entry.tag.color) : "var(--muted-foreground)"
  const name = entry?.displayName ?? segment.name
  const label = segment.hasColon ? `${name}: ${segment.value}` : name

  const body = (
    <>
      <Hash aria-hidden="true" className="size-3 shrink-0 opacity-70" />
      <span className="truncate font-medium">{name}</span>
      {segment.hasColon && (
        <span className="truncate text-foreground/80">{segment.value}</span>
      )}
    </>
  )

  return (
    <span
      style={{ "--tag-accent": accent } as CSSProperties}
      className={cn(
        "inline-flex h-6 max-w-48 shrink-0 items-center gap-1 rounded-full bg-[color-mix(in_oklab,var(--tag-accent)_12%,transparent)] pr-0.5 pl-2 text-xs text-(--tag-accent) ring-1 ring-[color-mix(in_oklab,var(--tag-accent)_25%,transparent)] duration-150 ring-inset animate-in fade-in-0 zoom-in-95",
        className
      )}
    >
      {onEdit ? (
        <button
          type="button"
          tabIndex={-1}
          title={label}
          onClick={onEdit}
          className="flex min-w-0 cursor-pointer items-center gap-1 rounded-l-full outline-none transition-opacity hover:opacity-80"
        >
          {body}
        </button>
      ) : (
        body
      )}
      <button
        type="button"
        tabIndex={onEdit ? -1 : undefined}
        aria-label={t("local.search.remove_filter").replace("{name}", label)}
        title={t("local.search.remove_filter").replace("{name}", label)}
        onClick={onRemove}
        className="flex size-5 shrink-0 items-center justify-center rounded-full opacity-70 transition hover:bg-[color-mix(in_oklab,var(--tag-accent)_20%,transparent)] hover:opacity-100"
      >
        <X aria-hidden="true" className="size-3" />
      </button>
    </span>
  )
}

/** Removable chips for the tag filters currently applied, shown above the results. */
export function ActiveSearchFilters({
  segments,
  index,
  onRemove,
  onClear,
}: {
  segments: TagSegment[]
  index: BookTagIndex | null
  onRemove: (segment: TagSegment) => void
  onClear: () => void
}) {
  const { t } = useTranslation()
  if (segments.length === 0) return null
  return (
    <ul className="flex min-w-0 flex-wrap items-center gap-1.5">
      {segments.map((segment) => (
        <li key={segment.start} className="flex min-w-0">
          <TagFilterChip
            segment={segment}
            index={index}
            className="max-w-56"
            onRemove={() => onRemove(segment)}
          />
        </li>
      ))}
      {segments.length > 1 && (
        <li>
          <button
            type="button"
            onClick={onClear}
            className="h-6 rounded-full px-2 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            {t("local.search.clear_filters")}
          </button>
        </li>
      )}
    </ul>
  )
}

/* ------------------------------------------------------------- helpers -- */

function groupLabel(
  group: SuggestionGroup,
  t: ReturnType<typeof useTranslation>["t"]
): string {
  if (group.id === "tags") return t("local.search.section.tags")
  if (group.id === "books") return t("local.search.section.books")
  return `${group.entry?.displayName ?? ""} · ${t("local.search.section.values")}`
}

/** Tag colors work as accents; the default black falls back to the theme's foreground. */
function tagAccent(color: string): string {
  return tagNameColor(color) ?? "var(--foreground)"
}

function optionId(listboxId: string, key: string): string {
  return `${listboxId}-${encodeURIComponent(key)}`
}

function useShortcutLabel(): string {
  return useMemo(
    () =>
      typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.userAgent)
        ? "⌘K"
        : "Ctrl K",
    []
  )
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  )
}
