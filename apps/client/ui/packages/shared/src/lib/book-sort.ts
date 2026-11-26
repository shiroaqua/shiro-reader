import type { Book } from "@shiro/ui-shared/lib/types"

export type BookSortField = "title" | "created_at" | "updated_at"
export type BookSortOrder = "asc" | "desc"
export type BookSort = { field: BookSortField; order: BookSortOrder }

export const DEFAULT_BOOK_SORT: BookSort = { field: "updated_at", order: "desc" }

const STORAGE_KEY = "shiro-reader.book-sort"

const compareTimestamp = (a: string, b: string) =>
  (Date.parse(a) || 0) - (Date.parse(b) || 0)

// Returns a sorted copy using natural numeric collation for titles and epoch-zero fallback for invalid timestamps.
export function sortBooks(books: readonly Book[], { field, order }: BookSort, locale?: string) {
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: "base" })
  const direction = order === "asc" ? 1 : -1
  const compare =
    field === "title"
      ? (a: Book, b: Book) =>
          direction * collator.compare(a.title, b.title) ||
          compareTimestamp(b.updated_at, a.updated_at)
      : (a: Book, b: Book) =>
          direction * compareTimestamp(a[field], b[field]) ||
          collator.compare(a.title, b.title)
  return [...books].sort(compare)
}

export function parseBookSort(value: string | null): BookSort {
  if (value) {
    const [field, order] = value.split(":")
    if (
      (field === "title" || field === "created_at" || field === "updated_at") &&
      (order === "asc" || order === "desc")
    )
      return { field, order }
  }
  return DEFAULT_BOOK_SORT
}

export function loadBookSort(): BookSort {
  try {
    return parseBookSort(localStorage.getItem(STORAGE_KEY))
  } catch {
    return DEFAULT_BOOK_SORT
  }
}

export function saveBookSort({ field, order }: BookSort) {
  try {
    localStorage.setItem(STORAGE_KEY, `${field}:${order}`)
  } catch {
    // Storage may be unavailable (e.g. private mode); sorting still works for the session.
  }
}
