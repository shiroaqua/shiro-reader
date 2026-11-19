import type { Book, BookTag, Tag, TagTreeNode } from "@/lib/types"
import { translateTagName, type Locale } from "@/lib/i18n"

/**
 * Client-side book search.
 *
 * Query syntax: free words match titles (case-insensitive, every word must
 * match), and `#name` / `#name:"value"` filters by tag binding. All conditions
 * are combined with AND. A tag filter matches only books bound to that exact
 * tag (no subtree), and its value must be equal, case-sensitively.
 */

export type TextSegment = {
  kind: "text"
  start: number
  end: number
  raw: string
}

export type SpaceSegment = {
  kind: "space"
  start: number
  end: number
  raw: string
}

export type TagSegment = {
  kind: "tag"
  start: number
  end: number
  /** Index where the well-formed tag token ended (before any glued trailing text). */
  tokenEnd: number
  raw: string
  name: string
  /** Index just after the name, i.e. where `:` sits when present. */
  nameEnd: number
  hasColon: boolean
  /** Index where the value text starts (after the opening quote when quoted). */
  valueStart: number
  value: string
  quoted: boolean
  closed: boolean
  /** Complete filters take part in matching; pending ones are still being typed. */
  complete: boolean
}

export type SearchSegment = TextSegment | SpaceSegment | TagSegment

export type TagFilter = {
  name: string
  value: string | null
}

export type ParsedBookSearch = {
  terms: string[]
  tagFilters: TagFilter[]
  isEmpty: boolean
}

const TAG_NAME_STOP = /[\s:"]/

export function tokenizeBookSearch(query: string): SearchSegment[] {
  const segments: SearchSegment[] = []
  let index = 0
  while (index < query.length) {
    const start = index
    if (/\s/.test(query[index])) {
      while (index < query.length && /\s/.test(query[index])) index += 1
      segments.push({ kind: "space", start, end: index, raw: query.slice(start, index) })
      continue
    }
    if (query[index] !== "#") {
      while (index < query.length && !/\s/.test(query[index])) index += 1
      segments.push({ kind: "text", start, end: index, raw: query.slice(start, index) })
      continue
    }

    index += 1
    while (index < query.length && !TAG_NAME_STOP.test(query[index])) index += 1
    const nameEnd = index
    const name = query.slice(start + 1, nameEnd)
    let hasColon = false
    let quoted = false
    let closed = false
    let value = ""
    let valueStart = nameEnd
    if (query[index] === ":") {
      hasColon = true
      index += 1
      valueStart = index
      if (query[index] === '"') {
        quoted = true
        index += 1
        valueStart = index
        while (index < query.length) {
          const char = query[index]
          if (char === "\\" && index + 1 < query.length) {
            value += query[index + 1]
            index += 2
            continue
          }
          if (char === '"') {
            closed = true
            index += 1
            break
          }
          value += char
          index += 1
        }
      } else {
        while (index < query.length && !/\s/.test(query[index])) index += 1
        value = query.slice(valueStart, index)
      }
    }
    const tokenEnd = index
    // Swallow anything glued to the token so it stays one segment.
    while (index < query.length && !/\s/.test(query[index])) index += 1
    const hasTrailing = index > tokenEnd
    segments.push({
      kind: "tag",
      start,
      end: index,
      tokenEnd,
      raw: query.slice(start, index),
      name,
      nameEnd,
      hasColon,
      valueStart,
      value,
      quoted,
      closed,
      complete:
        !hasTrailing &&
        name.length > 0 &&
        (!hasColon || (value.length > 0 && (!quoted || closed))),
    })
  }
  return segments
}

export function parseBookSearch(query: string): ParsedBookSearch {
  const segments = tokenizeBookSearch(query)
  const terms = segments
    .filter((segment): segment is TextSegment => segment.kind === "text")
    .map((segment) => segment.raw)
  const tagFilters = segments
    .filter(
      (segment): segment is TagSegment =>
        segment.kind === "tag" && segment.complete
    )
    .map((segment) => ({
      name: segment.name,
      value: segment.hasColon ? segment.value : null,
    }))
  return { terms, tagFilters, isEmpty: terms.length === 0 && tagFilters.length === 0 }
}

export function formatTagToken(name: string, value?: string | null): string {
  if (value === undefined || value === null) return `#${name}`
  return `#${name}:"${value.replace(/[\\"]/g, (char) => `\\${char}`)}"`
}

/* ---------------------------------------------------------------- index -- */

export type IndexedTag = {
  tag: Tag
  displayName: string
  /** The name written into the query; falls back to the raw name when the display name has spaces. */
  tokenName: string
  /** Display names of the ancestors, root first. */
  path: string[]
}

export type BookTagIndex = {
  tags: IndexedTag[]
  tagsByName: Map<string, IndexedTag[]>
  /** Missing entries mean the book's bindings have not loaded yet. */
  bindingsByBook: Map<string, BookTag[]>
  bookCountByTag: Map<string, number>
  valuesByTag: Map<string, Map<string, number>>
}

export function buildBookTagIndex(
  tree: TagTreeNode[],
  bindings: ReadonlyMap<string, BookTag[]>,
  locale: Locale
): BookTagIndex {
  const tags: IndexedTag[] = []
  const walk = (nodes: TagTreeNode[], path: string[]) => {
    for (const node of nodes) {
      const { children, ...tag } = node
      const displayName = translateTagName(tag.name, locale)
      tags.push({
        tag,
        displayName,
        tokenName: TAG_NAME_STOP.test(displayName) ? tag.name : displayName,
        path,
      })
      walk(children, [...path, displayName])
    }
  }
  walk(tree, [])

  const tagsByName = new Map<string, IndexedTag[]>()
  for (const entry of tags) {
    for (const name of new Set([entry.tag.name, entry.displayName, entry.tokenName])) {
      tagsByName.set(name, [...(tagsByName.get(name) ?? []), entry])
    }
  }

  const bookCountByTag = new Map<string, number>()
  const valuesByTag = new Map<string, Map<string, number>>()
  for (const list of bindings.values()) {
    for (const binding of list) {
      bookCountByTag.set(binding.tag_id, (bookCountByTag.get(binding.tag_id) ?? 0) + 1)
      if (binding.value === null || binding.value === "") continue
      const values = valuesByTag.get(binding.tag_id) ?? new Map<string, number>()
      values.set(binding.value, (values.get(binding.value) ?? 0) + 1)
      valuesByTag.set(binding.tag_id, values)
    }
  }

  return { tags, tagsByName, bindingsByBook: new Map(bindings), bookCountByTag, valuesByTag }
}

export function matchesBookSearch(
  book: Book,
  search: ParsedBookSearch,
  index: BookTagIndex | null
): boolean {
  const title = book.title.toLocaleLowerCase()
  if (!search.terms.every((term) => title.includes(term.toLocaleLowerCase()))) {
    return false
  }
  if (search.tagFilters.length === 0) return true
  const bindings = index?.bindingsByBook.get(book.id)
  if (!index || !bindings) return false
  return search.tagFilters.every((filter) => {
    const ids = new Set((index.tagsByName.get(filter.name) ?? []).map((entry) => entry.tag.id))
    return bindings.some(
      (binding) =>
        ids.has(binding.tag_id) &&
        (filter.value === null || binding.value === filter.value)
    )
  })
}

/* ---------------------------------------------------------- suggestions -- */

export type SearchContext =
  | { mode: "idle"; caret: number }
  | { mode: "text"; segment: TextSegment; term: string }
  | { mode: "tag-name"; segment: TagSegment; prefix: string }
  | { mode: "tag-value"; segment: TagSegment; prefix: string }

/** Works out what the user is typing at the caret. */
export function getSearchContext(query: string, caret: number): SearchContext {
  const clamped = Math.max(0, Math.min(caret, query.length))
  const segment = tokenizeBookSearch(query).find(
    (item) => item.kind !== "space" && item.start < clamped && clamped <= item.end
  )
  if (!segment || segment.kind === "space") return { mode: "idle", caret: clamped }
  if (segment.kind === "text") {
    return { mode: "text", segment, term: segment.raw }
  }
  if (!segment.hasColon || clamped <= segment.nameEnd) {
    return {
      mode: "tag-name",
      segment,
      prefix: query.slice(segment.start + 1, Math.min(clamped, segment.nameEnd)),
    }
  }
  // Once the caret moves past a closing quote, the tag value is finished.
  if (segment.quoted && segment.closed && clamped >= segment.tokenEnd) {
    return { mode: "idle", caret: clamped }
  }
  return { mode: "tag-value", segment, prefix: segment.value }
}

export type SearchSuggestion =
  | { kind: "tag"; key: string; entry: IndexedTag; count: number }
  | { kind: "any-value"; key: string; entry: IndexedTag; count: number }
  | { kind: "value"; key: string; entry: IndexedTag; value: string; count: number }
  | { kind: "book"; key: string; book: Book }

export type SuggestionGroup = {
  id: "tags" | "values" | "books"
  entry?: IndexedTag
  items: SearchSuggestion[]
}

const TAG_LIMIT = 8
const VALUE_LIMIT = 8
const BOOK_LIMIT = 5

export function buildSuggestions(
  query: string,
  context: SearchContext,
  index: BookTagIndex | null,
  books: Book[]
): SuggestionGroup[] {
  const tagGroup = (keyword: string): SuggestionGroup | null => {
    if (!index) return null
    const needle = keyword.toLocaleLowerCase()
    const items = index.tags
      .filter(
        (entry) =>
          needle === "" ||
          entry.displayName.toLocaleLowerCase().includes(needle) ||
          entry.tag.name.toLocaleLowerCase().includes(needle)
      )
      .map((entry) => ({
        entry,
        count: index.bookCountByTag.get(entry.tag.id) ?? 0,
        starts: entry.displayName.toLocaleLowerCase().startsWith(needle),
      }))
      .sort(
        (a, b) =>
          Number(b.starts) - Number(a.starts) ||
          b.count - a.count ||
          a.entry.displayName.localeCompare(b.entry.displayName)
      )
      .slice(0, TAG_LIMIT)
      .map(({ entry, count }): SearchSuggestion => ({
        kind: "tag",
        key: `tag:${entry.tag.id}`,
        entry,
        count,
      }))
    return items.length > 0 ? { id: "tags", items } : null
  }

  if (context.mode === "idle") {
    return [tagGroup("")].filter((group) => group !== null)
  }

  if (context.mode === "tag-name") {
    return [tagGroup(context.prefix)].filter((group) => group !== null)
  }

  if (context.mode === "tag-value") {
    if (!index) return []
    const entries = index.tagsByName.get(context.segment.name) ?? []
    const needle = context.prefix.toLocaleLowerCase()
    return entries.flatMap((entry): SuggestionGroup[] => {
      const values = [
        ...(index.valuesByTag.get(entry.tag.id) ?? new Map<string, number>()),
      ]
        .filter(([value]) => value.toLocaleLowerCase().includes(needle))
        .map(([value, count]) => ({
          value,
          count,
          starts: needle !== "" && value.toLocaleLowerCase().startsWith(needle),
        }))
        .sort(
          (a, b) =>
            Number(b.starts) - Number(a.starts) ||
            b.count - a.count ||
            a.value.localeCompare(b.value)
        )
        .slice(0, VALUE_LIMIT)
        .map(({ value, count }): SearchSuggestion => ({
          kind: "value",
          key: `value:${entry.tag.id}:${value}`,
          entry,
          value,
          count,
        }))
      const items: SearchSuggestion[] =
        needle === ""
          ? [
              {
                kind: "any-value",
                key: `any:${entry.tag.id}`,
                entry,
                count: index.bookCountByTag.get(entry.tag.id) ?? 0,
              },
              ...values,
            ]
          : values
      return items.length > 0 ? [{ id: "values", entry, items }] : []
    })
  }

  const parsed = parseBookSearch(query)
  const bookItems = books
    .filter((book) =>
      parsed.tagFilters.length > 0 && index !== null
        ? matchesBookSearch(book, parsed, index)
        : parsed.terms.every((term) =>
            book.title.toLocaleLowerCase().includes(term.toLocaleLowerCase())
          )
    )
    .slice(0, BOOK_LIMIT)
    .map((book): SearchSuggestion => ({ kind: "book", key: `book:${book.id}`, book }))
  const groups: SuggestionGroup[] = []
  const tags = tagGroup(context.term)
  if (tags) groups.push({ ...tags, items: tags.items.slice(0, 4) })
  if (bookItems.length > 0) groups.push({ id: "books", items: bookItems })
  return groups
}

export type SuggestionEdit = {
  query: string
  caret: number
}

function formatBookTitleQuery(title: string): string {
  // Strip leading `#` from whitespace-separated words so titles like "Book #2"
  // stay plain text tokens instead of becoming broken tag filters.
  const sanitized = title.replace(/(^|\s)#+/g, "$1").trim()
  return sanitized || title
}

/** Replaces the segment under the caret with the chosen suggestion. */
export function applySuggestion(
  query: string,
  context: SearchContext,
  suggestion: SearchSuggestion,
  index: BookTagIndex | null
): SuggestionEdit {
  if (suggestion.kind === "book") {
    const keptTags = tokenizeBookSearch(query)
      .filter((segment): segment is TagSegment => segment.kind === "tag")
      .map((segment) => segment.raw)
    const next = `${[...keptTags, formatBookTitleQuery(suggestion.book.title)].join(" ")} `
    return { query: next, caret: next.length }
  }

  const start = context.mode === "idle" ? context.caret : context.segment.start
  const end = context.mode === "idle" ? context.caret : context.segment.end
  const before = query.slice(0, start)
  const after = query.slice(end).replace(/^\s+/, "")
  const lead = before === "" || /\s$/.test(before) ? before : `${before} `

  let token: string
  let caretOffset: number | null = null
  if (suggestion.kind === "tag") {
    const hasValues =
      suggestion.entry.tag.allows_value &&
      (index?.valuesByTag.get(suggestion.entry.tag.id)?.size ?? 0) > 0
    if (hasValues) {
      token = `#${suggestion.entry.tokenName}:""`
      // Park the caret between the quotes so value hints show up right away.
      caretOffset = token.length - 1
    } else {
      token = formatTagToken(suggestion.entry.tokenName)
    }
  } else if (suggestion.kind === "any-value") {
    token = formatTagToken(suggestion.entry.tokenName)
  } else {
    token = formatTagToken(suggestion.entry.tokenName, suggestion.value)
  }

  if (caretOffset !== null) {
    const next = `${lead}${token}${after === "" ? "" : ` ${after}`}`
    return { query: next, caret: lead.length + caretOffset }
  }
  const next = `${lead}${token} ${after}`
  return { query: next, caret: lead.length + token.length + 1 }
}

/** Removes one segment (e.g. a tag filter chip) without touching other segments. */
export function removeSegment(query: string, segment: SearchSegment): string {
  const before = query.slice(0, segment.start).trimEnd()
  const after = query.slice(segment.end).trimStart()
  if (before === "") return after
  if (after === "") return before
  return `${before} ${after}`
}

/** Removes every complete tag filter from the query, keeping free-text terms. */
export function removeTagSegments(query: string): string {
  let result = query
  const tags = tokenizeBookSearch(query).filter(
    (segment): segment is TagSegment => segment.kind === "tag" && segment.complete
  )
  for (let i = tags.length - 1; i >= 0; i -= 1) {
    result = removeSegment(result, tags[i])
  }
  return result
}

/* --------------------------------------------------------------- chips -- */

export type SearchPartition = {
  chips: TagSegment[]
  draft: string
  /** Length of the `"chip1 chip2 "` prefix before `draft`, or 0 when there are no chips. */
  prefixLength: number
}

export function isKnownCompleteTag(
  segment: TagSegment,
  index: BookTagIndex | null
): boolean {
  return (
    segment.complete &&
    index !== null &&
    (index.tagsByName.get(segment.name)?.length ?? 0) > 0
  )
}

export function composeSearchValue(
  chips: readonly Pick<TagSegment, "raw">[],
  draft: string
): string {
  const chipText = chips.map((chip) => chip.raw).join(" ")
  if (chipText === "") return draft
  if (draft === "") return chipText
  return `${chipText} ${draft}`
}

/**
 * Splits a query string into committed tag chips and the editable draft text.
 * Known, complete tags become chips unless `focused` and the tag at
 * `editingTagStart` is currently being edited in the input.
 */
export function partitionSearchChips(
  value: string,
  index: BookTagIndex | null,
  focused: boolean,
  editingTagStart: number | null
): SearchPartition {
  const segments = tokenizeBookSearch(value)
  const chips: TagSegment[] = []
  for (const segment of segments) {
    if (
      segment.kind === "tag" &&
      isKnownCompleteTag(segment, index) &&
      (!focused || segment.start !== editingTagStart)
    ) {
      chips.push(segment)
    }
  }
  let draft = value
  if (chips.length > 0) {
    let prefixEnd = 0
    let isLeadingPrefix = true
    for (const chip of chips) {
      while (prefixEnd < value.length && /\s/.test(value[prefixEnd])) {
        prefixEnd += 1
      }
      if (chip.start !== prefixEnd) {
        isLeadingPrefix = false
        break
      }
      prefixEnd = chip.end
    }
    if (isLeadingPrefix) {
      if (prefixEnd < value.length && /\s/.test(value[prefixEnd])) {
        prefixEnd += 1
      }
      draft = value.slice(prefixEnd)
    } else {
      for (let i = chips.length - 1; i >= 0; i -= 1) {
        draft = removeSegment(draft, chips[i])
      }
    }
  }
  const prefixLength =
    chips.length === 0 ? 0 : chips.map((chip) => chip.raw).join(" ").length + 1
  return { chips, draft, prefixLength }
}

function spliceOutTag(
  text: string,
  segment: TagSegment,
  caret: number
): {
  text: string
  caret: number
  removedStart: number
  removedLength: number
} {
  let removeStart = segment.start
  let removeEnd = segment.end
  if (removeEnd < text.length && /\s/.test(text[removeEnd])) {
    removeEnd += 1
  } else if (removeStart > 0 && /\s/.test(text[removeStart - 1])) {
    removeStart -= 1
  }
  const removedLength = removeEnd - removeStart
  const nextText = `${text.slice(0, removeStart)}${text.slice(removeEnd)}`
  const nextCaret =
    caret <= removeStart
      ? caret
      : caret >= removeEnd
        ? caret - removedLength
        : removeStart
  return {
    text: nextText,
    caret: nextCaret,
    removedStart: removeStart,
    removedLength,
  }
}

/**
 * Promotes any newly finished known tags in `nextDraft` into `chips`, keeps
 * the tag currently under `caret` editable, and returns the normalized query
 * plus updated draft caret and `editingTagStart` offset.
 */
export function commitSearchDraft(
  chips: readonly Pick<TagSegment, "raw">[],
  nextDraft: string,
  caret: number,
  index: BookTagIndex | null
): {
  query: string
  draftCaret: number
  editingTagStart: number | null
  promotedCount: number
} {
  const clamped = Math.max(0, Math.min(caret, nextDraft.length))
  const segments = tokenizeBookSearch(nextDraft)
  const activeTag =
    segments.find(
      (segment): segment is TagSegment =>
        segment.kind === "tag" &&
        segment.start < clamped &&
        clamped <= segment.end
    ) ?? null
  const promoted = segments.filter(
    (segment): segment is TagSegment =>
      segment.kind === "tag" &&
      segment !== activeTag &&
      isKnownCompleteTag(segment, index)
  )

  let remainingDraft = nextDraft
  let adjustedCaret = clamped
  let activeStartInDraft = activeTag ? activeTag.start : null
  for (let i = promoted.length - 1; i >= 0; i -= 1) {
    const spliced = spliceOutTag(remainingDraft, promoted[i], adjustedCaret)
    remainingDraft = spliced.text
    adjustedCaret = spliced.caret
    if (
      activeStartInDraft !== null &&
      activeStartInDraft >= spliced.removedStart + spliced.removedLength
    ) {
      activeStartInDraft -= spliced.removedLength
    }
  }

  const allChips = [...chips, ...promoted]
  const nextPrefixLength =
    allChips.length === 0
      ? 0
      : allChips.map((chip) => chip.raw).join(" ").length + 1
  return {
    query: composeSearchValue(allChips, remainingDraft),
    draftCaret: adjustedCaret,
    editingTagStart:
      activeStartInDraft === null ? null : nextPrefixLength + activeStartInDraft,
    promotedCount: promoted.length,
  }
}
